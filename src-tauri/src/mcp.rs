//! 把 MCP 端點掛在 App 上。
//!
//! # 協定用官方的 `rmcp`，不自己寫
//!
//! 一度手寫過一份 92 行的 JSON-RPC 外殼。換掉了，理由是：
//!
//! - **通訊協定會演進**，而這個專案的重點不是 MCP。session、SSE、
//!   分頁、取消、`outputSchema`……每一項都是「用到才會發現沒做」。
//! - 手寫那份**沒有任何真的客戶端驗證過**，測試驗的只是我對規格的理解。
//!
//! 判斷仍然全部在 [`loom_mcp`]（同步、無 async、只相依 serde），
//! 所以 Agent 的行為照樣用 `cargo test` 驗。這裡只做轉接。
//!
//! # 為什麼是掛在 App 上，不是另外一支執行檔
//!
//! MCP 常見的作法是「客戶端啟動一支 server 程序」。那樣的話 Agent 操作的
//! 是**磁碟上的檔案**，跟使用者正開著的那份會分岔——他在畫面上改了還沒存檔，
//! Agent 卻看不到，兩邊各改各的。
//!
//! 掛在 App 上，Agent 操作的就是**眼前這份**：
//!
//! - 改的東西馬上出現在畫面上
//! - 走同一條 `History`，所以使用者按 ⌘Z 退得掉
//! - 不自動存檔，標題列會亮「未儲存」，由人決定
//!
//! 而且使用者不必額外跑任何東西。
//!
//! # 安全
//!
//! 這是一個**可以改使用者檔案的本機端點**，所以：
//!
//! | | |
//! | --- | --- |
//! | 只綁 `127.0.0.1` | 不接受區網連線 |
//! | 需要 token | 每次啟用重新產生，不落地 |
//! | 預設關閉 | 使用者要自己打開 |
//! | 擋掉帶 `Origin` 的請求 | 瀏覽器裡的網頁不能打它（DNS rebinding） |
//!
//! 最後一條容易漏：本機端點對網頁來說也是可達的。MCP 客戶端是原生程式，
//! 不會送 `Origin`，所以「有 `Origin` 就拒絕」是一條乾淨的界線。

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use loom_core::Project;
use loom_core::edit::Edit;
use loom_mcp::Workspace;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::Opened;

/// 執行中的端點。
pub struct Running {
    port: u16,
    token: String,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

#[derive(Default)]
pub struct Server(pub Option<Running>);

/// 前端要顯示的狀態。**不含 token**——狀態會被畫面到處傳，
/// 而 token 只該在使用者主動要求時出現一次（見 [`config_snippet`]）。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub running: bool,
    pub url: Option<String>,
}

// ── 工具：轉接到 loom-mcp ────────────────────────────────────────

/// 工具呼叫要去哪裡執行。
///
/// # 為什麼是一個 port
///
/// 沒有這層的話，整個 HTTP 端點只能在「有一個真的 Tauri App」的情況下驗證，
/// 而傳輸層正是我最沒把握的部分（token 檢查、`Origin` 檢查、rmcp 的接線）。
///
/// 有了它，測試可以拿一個假的 Desk 起一個真的 server，用真的 TCP 連線
/// 打進去——那才叫驗證過。
pub trait Desk: Send + Sync + 'static {
    fn call(&self, name: &str, args: &Value) -> Result<String, String>;
}

/// App 的實作：Agent 操作的就是使用者眼前那份。
///
/// 拿的是同一個 `History`，所以修改會進同一條復原鏈，畫面重畫時
/// 看到的也是同一份資料。
struct AppDesk(AppHandle);

/// `Opened` 借出來當 workspace 用。
struct Borrowed<'a>(&'a mut Opened);

impl Workspace for Borrowed<'_> {
    fn project(&self) -> Option<&Project> {
        self.0
            .history
            .as_ref()
            .map(loom_core::history::History::project)
    }

    fn edit(&mut self, edit: &Edit) -> Result<(), String> {
        self.0
            .history
            .as_mut()
            .ok_or("還沒有開啟任何專案")?
            .edit(edit)
            .map_err(|e| e.to_string())
    }
}

impl Desk for AppDesk {
    fn call(&self, name: &str, args: &Value) -> Result<String, String> {
        let state = self.0.state::<Mutex<Opened>>();
        let outcome = {
            let mut opened = state.lock().map_err(|_| "內部狀態毀損")?;
            loom_mcp::tools::call(&mut Borrowed(&mut opened), name, args)
        };

        // 通知畫面重畫。Agent 在改東西的時候使用者要看得到——
        // 看不到的話，等他發現時已經是一整批改完了，那就沒辦法逐項判斷。
        let _ = self.0.emit("loom://changed", ());
        outcome
    }
}

#[derive(Clone)]
struct Handler {
    desk: Arc<dyn Desk>,
}

impl ServerHandler for Handler {
    fn get_info(&self) -> InitializeResult {
        // rmcp 的型別都是 non_exhaustive（之後還會加欄位），所以一律走
        // 建構子與 with_* 而不是自己填 struct——那樣升級就會編不過。
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "diagram-loom",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(loom_mcp::INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        // 工具清單是 loom-mcp 給的一份 JSON。轉成 rmcp 的型別，
        // 不讓 rmcp 的型別漏進那個 crate——它要維持同步且無 async。
        let tools = loom_mcp::tools::list()
            .as_array()
            .map(|list| list.iter().map(to_tool).collect())
            .unwrap_or_default();
        Ok(ListToolsResult {
            tools,
            ..ListToolsResult::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| json!({}));
        let outcome = self.desk.call(&request.name, &args);

        // 工具失敗要回 `CallToolResult::error`，**不是** `Err`。
        // 回成協定層錯誤的話多數客戶端會直接中斷，模型連修正的機會都沒有；
        // 而這裡的錯誤訊息全都是寫來讓它自己修正的（附候選、附下一步）。
        Ok(match outcome {
            Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Err(message) => CallToolResult::error(vec![ContentBlock::text(message)]),
        }
        .into())
    }
}

fn to_tool(v: &Value) -> Tool {
    Tool::new(
        v["name"].as_str().unwrap_or_default().to_string(),
        v["description"].as_str().unwrap_or_default().to_string(),
        Arc::new(
            v.get("inputSchema")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
        ),
    )
}

// ── HTTP ────────────────────────────────────────────────────────

#[derive(Clone)]
struct Guard {
    token: String,
}

/// token 與 `Origin` 的檢查。rmcp 負責協定，這一層負責「誰進得來」。
async fn guard(State(guard): State<Guard>, request: Request, next: Next) -> Response {
    let headers = request.headers();

    // 瀏覽器一定會送 Origin，原生的 MCP 客戶端不會。這條界線擋掉
    // 「使用者開了一個惡意網頁，那個網頁去打本機端點」。
    if headers.contains_key("origin") {
        return (StatusCode::FORBIDDEN, "不接受來自瀏覽器的請求").into_response();
    }

    let ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| t == guard.token);
    if !ok {
        return (StatusCode::UNAUTHORIZED, "token 不對").into_response();
    }

    next.run(request).await
}

/// 組出整個端點：rmcp 的協定服務，外面包一層 token 與 `Origin` 的檢查。
///
/// 抽成函式是為了讓測試起得起同一份東西——傳輸層要被真的 HTTP 打過才算數。
fn router(token: &str, desk: Arc<dyn Desk>) -> axum::Router {
    let handler = Handler { desk };
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        {
            let mut config = StreamableHttpServerConfig::default();
            // 不開 session。這裡的狀態全在 App 的 `History` 裡，
            // 每次呼叫都是獨立的一問一答——多一層 session 只是多一個
            // 會過期、會對不上的東西。
            config.legacy_session_mode = false;
            // 有了上面那行，簡單的一問一答才會用 JSON 回而不是 SSE。
            config.json_response = true;
            config
        },
    );

    axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(
            Guard {
                token: token.to_string(),
            },
            guard,
        ))
}

/// 給整合測試用：組出同一份端點，但接一個假的 [`Desk`]。
///
/// 傳輸層要被真的 HTTP 打過才算數——推理得不到答案。
/// 見 `tests/mcp_endpoint.rs`。
pub fn router_for_test(token: &str, desk: Arc<dyn Desk>) -> axum::Router {
    router(token, desk)
}

/// 啟動端點。已經在跑就直接回現況。
pub fn start(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    {
        let running = server.lock().map_err(|_| "內部狀態毀損")?;
        if let Some(r) = &running.0 {
            return Ok(status(Some(r)));
        }
    }

    let token = new_token();
    let router = router(&token, Arc::new(AppDesk(app.clone())));

    // 綁 0 讓作業系統挑一個沒人用的埠。固定埠會在開兩個 App 時撞在一起，
    // 而且固定的東西比較好被別人猜到。
    let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .map_err(|e| format!("開不了本機端點：{e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("開不了本機端點：{e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("問不到埠號：{e}"))?
        .port();

    let (tx, rx) = tokio::sync::oneshot::channel();
    tauri::async_runtime::spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).expect("接手監聽失敗");
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });

    let mut running = server.lock().map_err(|_| "內部狀態毀損")?;
    running.0 = Some(Running {
        port,
        token,
        shutdown: tx,
    });
    Ok(status(running.0.as_ref()))
}

pub fn stop(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    let mut running = server.lock().map_err(|_| "內部狀態毀損")?;
    if let Some(r) = running.0.take() {
        let _ = r.shutdown.send(());
    }
    Ok(status(None))
}

pub fn status_of(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    let running = server.lock().map_err(|_| "內部狀態毀損")?;
    Ok(status(running.0.as_ref()))
}

fn status(running: Option<&Running>) -> McpStatus {
    match running {
        None => McpStatus {
            running: false,
            url: None,
        },
        Some(r) => McpStatus {
            running: true,
            url: Some(format!("http://127.0.0.1:{}/mcp", r.port)),
        },
    }
}

/// 每次啟用都換一個新的。
///
/// 不落地是刻意的：token 存進設定檔的話，它會活得比使用者的意圖久——
/// 他關掉 App 就該預期這個門關了。
fn new_token() -> String {
    // 借用 UUID 的隨機來源，不另外拉一個 rand。兩個 UUID 去掉連字號
    // 是 256 bit，對一個只存在於這次執行的本機 token 綽綽有餘。
    format!(
        "{}{}",
        loom_core::id::Id::generate(),
        loom_core::id::Id::generate()
    )
    .replace('-', "")
}

/// 給使用者複製到 Agent 設定檔裡的那一段 JSON。**含 token。**
pub fn config_snippet(app: &AppHandle) -> Result<Option<String>, String> {
    let server = app.state::<Mutex<Server>>();
    let running = server.lock().map_err(|_| "內部狀態毀損")?;
    Ok(running.0.as_ref().map(|r| {
        serde_json::to_string_pretty(&json!({
            "mcpServers": {
                "diagram-loom": {
                    "type": "http",
                    "url": format!("http://127.0.0.1:{}/mcp", r.port),
                    "headers": { "Authorization": format!("Bearer {}", r.token) }
                }
            }
        }))
        .unwrap_or_default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_long_enough_to_not_be_guessed() {
        let t = new_token();
        assert_eq!(t.len(), 64, "兩個 UUID 去掉連字號應該是 64 個字");
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(t, new_token(), "每次都要不一樣");
    }

    #[test]
    fn the_status_never_leaks_the_token() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let r = Running {
            port: 1234,
            token: "秘密".into(),
            shutdown: tx,
        };
        let s = status(Some(&r));
        assert_eq!(s.url.as_deref(), Some("http://127.0.0.1:1234/mcp"));
        assert!(!serde_json::to_string(&s).unwrap().contains("秘密"));
    }

    #[test]
    fn every_tool_survives_the_conversion_to_rmcp() {
        // loom-mcp 吐的是 JSON，rmcp 要的是 Tool。轉換掉了描述或 schema 的話，
        // Agent 就看不到說明書了——而那些描述是它唯一的說明書。
        let tools = loom_mcp::tools::list();
        for v in tools.as_array().unwrap() {
            let t = to_tool(v);
            assert!(!t.name.is_empty());
            assert!(
                t.description.as_ref().is_some_and(|d| d.len() > 40),
                "{} 的描述掉了",
                t.name
            );
            assert_eq!(
                t.input_schema.get("type").and_then(Value::as_str),
                Some("object"),
                "{} 的 schema 掉了",
                t.name
            );
        }
    }
}
