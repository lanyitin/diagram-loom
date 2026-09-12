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
//! 這是一個**可以改使用者檔案的本機端點**。兩道鎖是**不可關的**：
//!
//! | | 擋掉誰 |
//! | --- | --- |
//! | 只綁 `127.0.0.1` | 區網上的任何人 |
//! | 有 `Origin` 就拒絕 | 瀏覽器裡的網頁（含 DNS rebinding） |
//!
//! 第二條容易漏：本機端點對網頁來說也是可達的。MCP 客戶端是原生程式，
//! 不會送 `Origin`，所以這是一條乾淨的界線。
//!
//! token 是第三道，**可以關**。先講清楚它實際擋得住誰：
//!
//! - **同一個使用者的其他程序**——擋不住。它們本來就讀得到專案檔，
//!   也讀得到存 token 的設定檔。token 對這種情況等於零。
//! - **同一台機器上的其他使用者**——這裡是真的有用。他們讀不到你的
//!   家目錄，但連得到 `127.0.0.1`。
//!
//! 所以單人使用的機器上邊際價值很低，共用機器上才是真的。預設開著，
//! 使用者可以自己關，畫面上會直說關掉之後少了什麼。
//!
//! # 設定是 App 層級的，不是專案層級
//!
//! 一度想放進專案檔，錯了：**Agent 的設定裡只有一個 URL**。埠跟著專案走
//! 的話，切一次專案那份設定就失效——而「不要每次重貼」正是這些設定
//! 存在的唯一理由。token 同理。
//!
//! 所以埠、token、自動啟用都存在 App 自己的設定區。端點屬於 App，
//! 開哪個專案 Agent 就看哪個。

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
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorData,
    Implementation, InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
    Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::Opened;

/// 執行中的端點。
pub struct Running {
    port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

/// 端點的狀態。**開機自動啟用失敗的原因也留在這裡。**
///
/// 少了 `autostart_failed`，自動啟用撞到埠被佔用時是**完全安靜**的：
/// 使用者以為端點開著，Agent 連不進來，而畫面上看起來就跟他忘了打開一樣。
/// 那正是「自動啟用」要消滅的那個症狀，只是換了個原因。
#[derive(Default)]
pub struct Server {
    pub running: Option<Running>,
    pub autostart_failed: Option<String>,
}

/// 存在 App 設定區的偏好。**跟著這台機器，不跟著專案。**
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentConfig {
    /// 想用哪個埠。`None` 表示交給作業系統挑。
    ///
    /// 指定一個固定的埠，Agent 那邊的設定就不必每次重開都重貼——
    /// 那是這個欄位存在的唯一理由。
    pub port: Option<u16>,
    /// 要不要檢查 token。預設 `true`。
    pub require_token: bool,
    /// 一直都有，只是 `require_token` 為 `false` 時不檢查。
    ///
    /// 這樣使用者關掉再打開時不必重新產生一組，貼出去的設定也不會變。
    pub token: String,
    /// App 啟動時就把端點打開。
    ///
    /// 預設 `false`：這是可以改你檔案的端點，不該安靜地開著。
    /// 但使用者自己打開過之後，下次不必再打開一次——
    /// 一個每次都要重設的偏好等於沒有偏好。
    ///
    /// ⚠️ 這句話曾經只是一句話：畫面上另外放了一個「開啟 App 時自動啟用」
    /// 核取方塊，於是打開端點的人下次還是要再打開一次。現在**開關本身
    /// 就是這個偏好**，只有 [`remember`] 寫得動它。
    pub autostart: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            port: None,
            require_token: true,
            token: new_token(),
            autostart: false,
        }
    }
}

/// 前端要顯示的東西。**不含 token**——狀態會被畫面到處傳，
/// 而 token 只該在使用者主動要求時出現一次（見 [`config_snippet`]）。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub running: bool,
    pub url: Option<String>,
    /// 想用的埠。`None` 表示交給作業系統挑。
    pub preferred_port: Option<u16>,
    pub require_token: bool,
    pub autostart: bool,
    /// 開機自動啟用失敗了的話，原因。成功或沒開自動啟用時是 `None`。
    pub autostart_failed: Option<String>,
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

/// App 的實作：Agent 操作的就是使用者眼前那幾份。
///
/// 拿的是同一個 `History`，所以修改會進同一條復原鏈，畫面重畫時
/// 看到的也是同一份資料。
///
/// # 使用者開著什麼，Agent 就看得到什麼
///
/// 清單直接來自視窗登記簿（`crate::Desk`），所以**沒有視窗在看的專案，
/// Agent 一定碰不到**。它的觸及範圍恰好等於使用者看得到的範圍——
/// 那正是把 MCP 掛在 App 裡而不是做成獨立程序的全部理由。
struct AppDesk(AppHandle);

/// `Opened` 借出來當 workspace 用。
struct Borrowed<'a>(&'a mut Opened);

impl Workspace for Borrowed<'_> {
    fn project(&self) -> Option<&Project> {
        Some(self.0.history.project())
    }

    fn edit(&mut self, edit: &Edit) -> Result<(), String> {
        self.0.history.edit(edit).map_err(|e| e.to_string())
    }
}

impl Desk for AppDesk {
    fn call(&self, name: &str, args: &Value) -> Result<String, String> {
        // 一、看使用者現在開著什麼。
        //
        // lint 每次都重跑：數百條連線是毫秒級，而 `projects` 與所有錯誤訊息
        // 都會印出這些數字——只在某些情況算的話，其他情況就會印 0，
        // 那比慢一點糟得多。
        let projects = self.0.state::<crate::Desk>().all();
        let mut open = Vec::with_capacity(projects.len());
        for p in &projects {
            let guard = p.state.lock().map_err(|_| "內部狀態毀損")?;
            let project = guard.history.project();
            let findings = loom_core::lint::lint(project);
            open.push(loom_mcp::pick::Open {
                slug: project.slug.clone(),
                name: project.name.clone(),
                root: p.root.display().to_string(),
                dirty: guard.history.is_dirty(),
                errors: findings
                    .iter()
                    .filter(|f| f.severity() == loom_core::lint::Severity::Error)
                    .count(),
                warnings: findings
                    .iter()
                    .filter(|f| f.severity() == loom_core::lint::Severity::Warning)
                    .count(),
            });
        }

        // 二、Agent 說要動哪一個。規則在 `loom_mcp::pick`，那裡不知道
        //     Tauri 存在，所以「多個時會不會亂猜」用 cargo test 就驗得到。
        //
        //     `projects` 不必挑——它問的正是「有哪些可以挑」。
        if !loom_mcp::tools::needs_project(name) {
            return Ok(loom_mcp::pick::listing(&open));
        }
        let picked = loom_mcp::pick::resolve(&open, loom_mcp::tools::wanted(args))?;

        // 三、只鎖那一個。其他專案的視窗完全不受影響——
        //     一批三百項跑得再久，隔壁那扇窗照樣能動。
        let target = &projects[picked];
        let outcome = {
            let mut opened = target.state.lock().map_err(|_| "內部狀態毀損")?;
            loom_mcp::tools::call(&mut Borrowed(&mut opened), &open, name, args)
        };

        // 四、只通知那一扇視窗重畫。Agent 在改東西的時候使用者要看得到——
        //     看不到的話，等他發現時已經是一整批改完了，那就沒辦法逐項判斷。
        let _ = self.0.emit_to(target.window.as_str(), "loom://changed", ());
        outcome
    }
}

#[derive(Clone)]
struct Handler {
    desk: Arc<dyn Desk>,
}

/// 工具清單可以被客戶端當成新鮮的多久。
///
/// 這份清單是編譯期常數（`loom_mcp::tools::list()`），一個行程活著的期間
/// 不會變，所以留一點快取；真的改了，也就是重新編譯與重啟之後的事。
const TOOL_LIST_TTL_MS: u64 = 5 * 60 * 1000;

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
        // `ttlMs` 與 `cacheScope` 在協定 2026-07-28 是**必填**（SEP-2549）。
        // rmcp 會把它們留成 `None`（因為舊版沒這兩個欄位），但只要客戶端
        // 談成了 2026-07-28，缺了就會被擋在 schema 驗證外面——Claude Code
        // 的症狀是「連上了，但抓不到工具」。舊版客戶端會忽略多出來的欄位，
        // 所以無條件帶上是安全的。
        Ok(ListToolsResult::with_all_items(tools)
            .with_ttl_ms(TOOL_LIST_TTL_MS)
            .with_cache_scope(CacheScope::Private))
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
    /// `None` 表示使用者關掉了 token 檢查。
    ///
    /// 只有這一道可以關。綁 `127.0.0.1` 與擋 `Origin` 是無條件的——
    /// 那兩道擋的是「不在這台機器上的人」與「瀏覽器裡的網頁」，
    /// 而使用者沒有理由要放那兩種進來。
    token: Option<String>,
}

/// 誰進得來。rmcp 負責協定，這一層只負責這件事。
async fn guard(State(guard): State<Guard>, request: Request, next: Next) -> Response {
    let headers = request.headers();

    // 瀏覽器一定會送 Origin，原生的 MCP 客戶端不會。這條界線擋掉
    // 「使用者開了一個惡意網頁，那個網頁去打本機端點」。**不可關。**
    if headers.contains_key("origin") {
        return (StatusCode::FORBIDDEN, "不接受來自瀏覽器的請求").into_response();
    }

    if let Some(want) = &guard.token {
        let ok = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| t == want);
        if !ok {
            return (StatusCode::UNAUTHORIZED, "token 不對").into_response();
        }
    }

    next.run(request).await
}

/// 組出整個端點：rmcp 的協定服務，外面包一層 token 與 `Origin` 的檢查。
///
/// 抽成函式是為了讓測試起得起同一份東西——傳輸層要被真的 HTTP 打過才算數。
fn router(token: Option<String>, desk: Arc<dyn Desk>) -> axum::Router {
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
        .layer(middleware::from_fn_with_state(Guard { token }, guard))
}

/// 設定檔的位置。App 的設定區，不是專案資料夾——
/// 專案資料夾會進使用者的 git，而 token 是憑證。
fn config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("找不到設定資料夾：{e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("建不了設定資料夾：{e}"))?;
    Ok(dir.join("agent.json"))
}

/// 讀設定。**讀不到或壞掉都退回預設值，不報錯**——
/// 一個偏好設定不該讓使用者連 App 都用不了。
pub fn config(app: &AppHandle) -> AgentConfig {
    config_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_config(app: &AppHandle, config: &AgentConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("寫不進 {}：{e}", path.display()))
}

/// 改設定。端點正在跑的話會**重開**，因為埠與 token 都是啟動時決定的。
///
/// **不收 `autostart`**：那個偏好只有開關本身寫得動（見 [`remember`]）。
/// 兩個地方寫得到同一個值，遲早會有一個把另一個蓋掉——這裡尤其危險，
/// 因為這個函式結尾會重開端點。
pub fn set_config(
    app: &AppHandle,
    port: Option<u16>,
    require_token: bool,
) -> Result<McpStatus, String> {
    let mut config = config(app);
    config.port = port;
    config.require_token = require_token;
    save_config(app, &config)?;
    restart_if_running(app)
}

/// 記住「使用者要不要這個端點開著」。
///
/// # 為什麼開關本身就是偏好
///
/// 這裡原本有兩個控制項：一個開關，一個「開啟 App 時自動啟用」核取方塊。
/// 於是打開端點的人下次還要再打開一次——除非他發現了第二個方塊。
///
/// 而 `AgentConfig::autostart` 的註解一直寫著「使用者自己打開過之後，
/// 下次不必再打開一次」。那句話描述的是這裡，不是那個核取方塊：
/// **一個每次都要重設的偏好等於沒有偏好。**
///
/// 預設仍然是關的——可以改你檔案的端點不該在誰都沒說過的情況下開著。
/// 但「他自己打開過」就是說過了。
///
/// ⚠️ 只有 [`crate::start_mcp`] / [`crate::stop_mcp`] 這兩個 command 呼叫得到。
/// 放進 [`start`] 的話 [`restart_if_running`] 會順手把它打開，
/// 於是「端點開著時關掉自動啟用」變成做不到。
pub fn remember(app: &AppHandle, autostart: bool) -> Result<(), String> {
    let mut config = config(app);
    if config.autostart == autostart {
        // 沒變就不要重寫檔案。每次開機都改寫一次設定檔沒有意義。
        return Ok(());
    }
    config.autostart = autostart;
    save_config(app, &config)
}

/// 開機自動啟用。失敗的原因留著給畫面說，**不要吞掉**。
pub fn autostart(app: &AppHandle) {
    if !config(app).autostart {
        return;
    }
    if let Err(why) = start(app)
        && let Ok(mut server) = app.state::<Mutex<Server>>().lock()
    {
        server.autostart_failed = Some(why);
    }
}

/// 換一組新的 token。舊的立刻失效。
pub fn regenerate_token(app: &AppHandle) -> Result<McpStatus, String> {
    let mut config = config(app);
    config.token = new_token();
    save_config(app, &config)?;
    restart_if_running(app)
}

fn restart_if_running(app: &AppHandle) -> Result<McpStatus, String> {
    let was_running = {
        let server = app.state::<Mutex<Server>>();
        let running = server.lock().map_err(|_| "內部狀態毀損")?;
        running.running.is_some()
    };
    if !was_running {
        return status_of(app);
    }
    stop(app)?;
    start(app)
}

/// 給整合測試用：組出同一份端點，但接一個假的 [`Desk`]。
///
/// 傳輸層要被真的 HTTP 打過才算數——推理得不到答案。
/// 見 `tests/mcp_endpoint.rs`。
pub fn router_for_test(token: Option<String>, desk: Arc<dyn Desk>) -> axum::Router {
    router(token, desk)
}

/// 啟動端點。已經在跑就直接回現況。
pub fn start(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    {
        let running = server.lock().map_err(|_| "內部狀態毀損")?;
        if let Some(r) = &running.running {
            return Ok(status(
                Some(r),
                running.autostart_failed.as_deref(),
                &config(app),
            ));
        }
    }

    let settings = config(app);
    let router = router(
        settings.require_token.then(|| settings.token.clone()),
        Arc::new(AppDesk(app.clone())),
    );

    // 使用者指定的埠，沒指定就綁 0 讓作業系統挑。
    //
    // 指定的埠被佔用時**報錯，不要偷偷換一個**——偷偷換的話他 Agent 那邊
    // 的設定會安靜地連到空氣，而那正是指定固定埠想避免的事。
    let wanted = settings.port.unwrap_or(0);
    let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], wanted)))
        .map_err(|e| match settings.port {
            Some(p) => format!("埠 {p} 開不起來（可能被別的程式佔用了）：{e}"),
            None => format!("開不了本機端點：{e}"),
        })?;
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
    running.running = Some(Running { port, shutdown: tx });
    // 開起來了，上次自動啟用失敗的那句話就不再成立。
    running.autostart_failed = None;
    Ok(status(
        running.running.as_ref(),
        running.autostart_failed.as_deref(),
        &config(app),
    ))
}

pub fn stop(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    let mut running = server.lock().map_err(|_| "內部狀態毀損")?;
    if let Some(r) = running.running.take() {
        let _ = r.shutdown.send(());
    }
    Ok(status(
        None,
        running.autostart_failed.as_deref(),
        &config(app),
    ))
}

pub fn status_of(app: &AppHandle) -> Result<McpStatus, String> {
    let server = app.state::<Mutex<Server>>();
    let running = server.lock().map_err(|_| "內部狀態毀損")?;
    Ok(status(
        running.running.as_ref(),
        running.autostart_failed.as_deref(),
        &config(app),
    ))
}

fn status(running: Option<&Running>, failed: Option<&str>, config: &AgentConfig) -> McpStatus {
    McpStatus {
        running: running.is_some(),
        url: running.map(|r| format!("http://127.0.0.1:{}/mcp", r.port)),
        preferred_port: config.port,
        require_token: config.require_token,
        autostart: config.autostart,
        autostart_failed: failed.map(str::to_string),
    }
}

/// 每次啟用都換一個新的。
///
/// 不服務實體是刻意的：token 存進設定檔的話，它會活得比使用者的意圖久——
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
    let settings = config(app);
    Ok(running.running.as_ref().map(|r| {
        let mut entry = json!({
            "type": "http",
            "url": format!("http://127.0.0.1:{}/mcp", r.port),
        });
        // 關掉檢查時連 headers 都不要出現——留一個空的下去，
        // 有些客戶端會照送，那只是讓人以為還有一道鎖。
        if settings.require_token {
            entry["headers"] = json!({
                "Authorization": format!("Bearer {}", settings.token)
            });
        }
        serde_json::to_string_pretty(&json!({ "mcpServers": { "diagram-loom": entry } }))
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
        // 狀態會被畫面到處傳。token 只能從 config_snippet 出去一次。
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let r = Running {
            port: 1234,
            shutdown: tx,
        };
        let config = AgentConfig {
            token: "秘密".into(),
            ..AgentConfig::default()
        };
        let s = status(Some(&r), None, &config);
        assert_eq!(s.url.as_deref(), Some("http://127.0.0.1:1234/mcp"));
        assert!(!serde_json::to_string(&s).unwrap().contains("秘密"));
    }

    #[test]
    fn the_defaults_are_the_careful_ones() {
        // 預設要 token、預設不自動啟用、預設讓 OS 挑埠。
        // 使用者可以一項一項放寬，但不該是他什麼都沒做就已經放寬了。
        let d = AgentConfig::default();
        assert!(d.require_token);
        assert!(!d.autostart);
        assert_eq!(d.port, None);
        assert_eq!(d.token.len(), 64);
    }

    #[test]
    fn the_config_survives_a_round_trip() {
        // 設定活不過重啟的話，「不用每次重貼」這個功能就沒意義了。
        let before = AgentConfig {
            port: Some(53809),
            require_token: false,
            token: "abc".into(),
            autostart: true,
        };
        let text = serde_json::to_string(&before).unwrap();
        let after: AgentConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(after.port, Some(53809));
        assert!(!after.require_token);
        assert_eq!(after.token, "abc");
        assert!(after.autostart);
    }

    #[test]
    fn only_the_switch_writes_the_autostart_preference() {
        // 兩個地方寫得動同一個偏好，遲早有一個把另一個蓋掉。這裡尤其危險：
        // `set_config` 結尾會 `restart_if_running`，而重開會走 `start`。
        // 若 `start` 也順手記一次，「關掉自動啟用」就會被自己的重開撤銷。
        //
        // 現在是型別擋住的（`set_config` 收不到 autostart），這條測試守的是
        // 另外那半邊：`start` 不准碰它。
        let source = include_str!("mcp.rs");
        let start = source
            .split("pub fn start(")
            .nth(1)
            .expect("找不到 start()");
        let body = start.split("\npub fn ").next().unwrap();
        assert!(
            !body.contains("remember("),
            "start() 碰了 autostart 偏好——重開端點會把使用者關掉的偏好打開回來"
        );
    }

    #[test]
    fn the_startup_failure_reaches_the_screen() {
        // 自動啟用失敗如果只是被吞掉，畫面跟「忘了打開」長得一模一樣，
        // 而使用者會以為自己上次忘了勾——那正是這個偏好要消滅的東西。
        let config = AgentConfig {
            autostart: true,
            ..AgentConfig::default()
        };
        let s = status(None, Some("埠 53809 開不起來"), &config);
        assert!(!s.running);
        assert!(s.autostart);
        assert_eq!(s.autostart_failed.as_deref(), Some("埠 53809 開不起來"));
    }

    #[test]
    fn an_old_config_file_still_loads() {
        // `serde(default)` 讓舊檔讀得進來。少了它，加一個欄位就會讓
        // 所有既有使用者在啟動時掉回預設值——而那包含「token 換了一組」。
        let c: AgentConfig = serde_json::from_str(r#"{"port":1234}"#).unwrap();
        assert_eq!(c.port, Some(1234));
        assert!(c.require_token, "舊檔沒有這個欄位時要退回安全的那一邊");
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
