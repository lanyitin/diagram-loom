//! 真的起一個 server，用真的 TCP 打進去。
//!
//! # 為什麼這份測試非有不可
//!
//! `loom-mcp` 那 14 個測試驗的是**工具的行為**，完全不碰 HTTP。
//! 而傳輸層正是最沒把握的地方：token 檢查、`Origin` 檢查、rmcp 的接線、
//! `Accept` 標頭對不對。這些「用推理」得不到答案——第一版手寫協定就是
//! 因為沒有真的客戶端打過，才一直不知道對不對。
//!
//! 所以這裡不 mock HTTP：起一個真的 listener，開一個真的 socket，
//! 自己寫 HTTP 請求進去。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};

use diagram_loom_lib::mcp::{Desk, router_for_test};
use serde_json::{Value, json};

/// 真的 token 是十六進位（見 `mcp::new_token`）。這裡跟著用 ASCII——
/// HTTP 標頭值本來就只能是 ASCII，用中文當 token 會在讀標頭時就失敗，
/// 那樣測到的是「標頭解不開」而不是「token 不對」。
const TOKEN: &str = "0123456789abcdef";

/// 假的 Desk：只記下被呼叫了什麼，並回一段固定的字。
///
/// 工具的實際行為由 `loom-mcp` 顧著，這裡要驗的是「請求有沒有送到」。
struct Spy(Mutex<Vec<String>>);

impl Desk for Spy {
    fn call(&self, name: &str, args: &Value) -> Result<String, String> {
        self.0.lock().unwrap().push(name.to_string());
        match name {
            "describe" => Ok(format!("看到了 {args}")),
            _ => Err(format!("沒有這個工具：{name}")),
        }
    }
}

struct Server {
    addr: SocketAddr,
    spy: Arc<Spy>,
    _runtime: tokio::runtime::Runtime,
}

fn start(token: Option<&str>) -> Server {
    let spy = Arc::new(Spy(Mutex::new(Vec::new())));
    let app = router_for_test(token.map(str::to_string), spy.clone());

    let listener = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();

    // 用 multi_thread：它自己有工作執行緒會推 spawn 出去的任務。
    // current_thread 需要有人一直 block_on 才會動，在測試裡很容易寫成死鎖。
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        let _ = axum::serve(listener, app).await;
    });

    Server {
        addr,
        spy,
        _runtime: runtime,
    }
}

/// 自己寫一個 HTTP 請求。不拉 client 相依——要驗的就是「線上長什麼樣」。
fn post(server: &Server, headers: &[(&str, &str)], body: &Value) -> (u16, String) {
    let body = body.to_string();
    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        request.push_str(&format!("{k}: {v}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);

    let mut stream = TcpStream::connect(server.addr).expect("連不上");
    // 有逾時才不會在協定對不上的時候整組測試卡死——那是最難查的失敗方式。
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw);

    let status = raw
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, payload(&raw))
}

/// 從回應裡挖出那個 JSON-RPC 物件。
///
/// 回應可能是純 JSON、可能是 chunked、也可能是 SSE（`data: {...}`），
/// 取決於協定版本與設定。要驗的是**內容**，所以三種都吃——
/// 綁死其中一種的話，rmcp 換了預設值這裡就會紅，但其實沒壞。
fn payload(raw: &str) -> String {
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    body.lines()
        .map(|l| l.trim().trim_start_matches("data:").trim())
        .find(|l| l.starts_with('{'))
        .unwrap_or("")
        .to_string()
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// 走完 initialize，回傳給後續請求用的標頭（含 session id，如果有的話）。
fn handshake(server: &Server, token: &str) -> Vec<(String, String)> {
    let auth = bearer(token);
    let (status, body) = post(
        server,
        &[("Authorization", &auth)],
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0"}
            }
        }),
    );
    assert_eq!(status, 200, "initialize 失敗：{body}");
    assert!(body.contains("diagram-loom"), "{body}");

    let mut headers = vec![("Authorization".to_string(), auth)];
    // rmcp 在 legacy 模式下會發 Mcp-Session-Id，之後的請求要帶回去。
    if let Some(id) = session_id(server, token) {
        headers.push(("Mcp-Session-Id".to_string(), id));
    }
    headers
}

/// 重跑一次 initialize 只為了讀回應標頭裡的 session id（如果有）。
fn session_id(server: &Server, token: &str) -> Option<String> {
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "0"}}
    })
    .to_string();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(server.addr).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok()?;
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw);
    raw.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("mcp-session-id:"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}

fn with_headers(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

#[test]
fn a_real_client_can_initialise_and_list_tools() {
    let server = start(Some(TOKEN));
    let headers = handshake(&server, TOKEN);

    let (status, body) = post(
        &server,
        &with_headers(&headers),
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    );
    assert_eq!(status, 200, "{body}");
    // 十個工具的名字至少要看得到幾個關鍵的。
    for name in ["describe", "lint", "create", "add_connection"] {
        assert!(body.contains(name), "工具清單少了 {name}：{body}");
    }
}

#[test]
fn a_tool_call_reaches_the_workspace() {
    let server = start(Some(TOKEN));
    let headers = handshake(&server, TOKEN);

    let (status, body) = post(
        &server,
        &with_headers(&headers),
        &json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "describe", "arguments": {}}
        }),
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("看到了"), "工具沒被呼叫到：{body}");
    assert!(
        server
            .spy
            .0
            .lock()
            .unwrap()
            .contains(&"describe".to_string()),
        "Desk 沒收到呼叫"
    );
}

#[test]
fn a_tool_failure_comes_back_as_a_result_not_a_protocol_error() {
    // 回成協定層錯誤的話多數客戶端會直接中斷，模型連修正的機會都沒有。
    // 而這裡的錯誤訊息全都是寫來讓它自己修正的。
    let server = start(Some(TOKEN));
    let headers = handshake(&server, TOKEN);

    let (status, body) = post(
        &server,
        &with_headers(&headers),
        &json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "刪掉全部", "arguments": {}}
        }),
    );
    assert_eq!(status, 200, "{body}");
    let json: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
    assert!(json.get("error").is_none(), "不該是協定層錯誤：{body}");
    assert_eq!(json["result"]["isError"], true, "{body}");
    assert!(body.contains("沒有這個工具"), "{body}");
}

mod the_door_is_locked {
    use super::*;

    #[test]
    fn without_a_token_anyone_local_gets_in() {
        // 使用者可以關掉 token 檢查。關掉之後就真的不檢查——
        // 這是刻意的取捨，不是漏洞（見 mcp.rs 的說明）。
        let server = start(None);
        let (status, body) = post(
            &server,
            &[],
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                    "params":{"protocolVersion":"2025-06-18","capabilities":{},
                              "clientInfo":{"name":"t","version":"0"}}}),
        );
        assert_eq!(status, 200, "{body}");
    }

    #[test]
    fn a_browser_is_still_blocked_even_with_the_token_check_off() {
        // 這一道**不可關**。使用者關掉的是 token，不是「讓網頁進來」。
        let server = start(None);
        let (status, _) = post(
            &server,
            &[("Origin", "https://evil.example")],
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        );
        assert_eq!(status, 403);
        assert!(server.spy.0.lock().unwrap().is_empty());
    }

    #[test]
    fn no_token_is_rejected() {
        let server = start(Some(TOKEN));
        let (status, _) = post(
            &server,
            &[],
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
        );
        assert_eq!(status, 401);
        assert!(
            server.spy.0.lock().unwrap().is_empty(),
            "沒帶 token 卻進得來"
        );
    }

    #[test]
    fn a_wrong_token_is_rejected() {
        let server = start(Some(TOKEN));
        let (status, _) = post(
            &server,
            &[("Authorization", "Bearer wrong-token")],
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
        );
        assert_eq!(status, 401);
    }

    #[test]
    fn a_request_from_a_browser_is_rejected_even_with_the_right_token() {
        // 本機端點對網頁來說也是可達的（DNS rebinding）。原生的 MCP 客戶端
        // 不會送 Origin，所以「有 Origin 就拒絕」是一條乾淨的界線。
        let server = start(Some(TOKEN));
        let (status, _) = post(
            &server,
            &[
                ("Authorization", "Bearer 0123456789abcdef"),
                ("Origin", "https://evil.example"),
            ],
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
        );
        assert_eq!(status, 403);
        assert!(server.spy.0.lock().unwrap().is_empty());
    }
}
