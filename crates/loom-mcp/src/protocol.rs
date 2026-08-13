//! MCP 的 JSON-RPC 外殼。
//!
//! 只實作用得到的三個方法：`initialize`、`tools/list`、`tools/call`。
//! MCP 還有 resources 與 prompts，這個工具都不需要——**沒用到的東西不實作**，
//! 因為每一個實作了但沒人走的分支都是將來會壞掉而沒人發現的地方。
//!
//! 這一層不知道 HTTP 存在，只做「一個 JSON 進來，一個 JSON 出去」。
//! 所以整個協定可以用 `cargo test` 驗證，不必真的開一個 server。

use serde_json::{Value, json};

use crate::{Workspace, tools};

/// 我們實作的版本。跟著客戶端送來的走，只在它送了我們不認得的版本時才用這個。
const PROTOCOL_VERSION: &str = "2025-06-18";

/// 處理一個 JSON-RPC 請求。
///
/// 回傳 `None` 表示這是一個通知（notification，沒有 `id`），依規範不能回應。
pub fn handle(ws: &mut dyn Workspace, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

    // 沒有 id 就是通知。`notifications/initialized` 會走到這裡。
    id.as_ref()?;

    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "tools/list" => Ok(json!({ "tools": tools::list() })),
        "tools/call" => return Some(reply(id, tools_call(ws, &params))),
        "ping" => Ok(json!({})),
        other => Err(format!("沒有這個方法：{other}")),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": message }
        }),
    })
}

fn initialize(params: &Value) -> Value {
    // 對方送什麼版本就回什麼版本，除非我們真的不認得。
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);

    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "diagram-loom", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "\
這是使用者當下開著的那份 diagram-loom 專案。

動手之前先叫 describe——不然你會重複建立已經有的東西。
每建完一批就叫 lint，照它說的補。

**不會自動存檔。** 你改的東西會出現在使用者畫面上並標記為未儲存，
由他看過再決定要不要留（他也可以按 ⌘Z 退回去）。所以做完之後
請用人話說明你建了什麼、還剩下哪些 lint 沒解決。"
    })
}

/// `tools/call` 的錯誤**不是 JSON-RPC 錯誤**。
///
/// 依 MCP 規範，工具執行失敗要回 `result` 並帶 `isError: true`，
/// 這樣模型看得到錯誤內容並且可以修正；回成協定層的 error 的話，
/// 多數客戶端會直接中斷，而使用者只會看到「工具壞了」。
fn tools_call(ws: &mut dyn Workspace, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match tools::call(ws, name, &args) {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(message) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    }
}

fn reply(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
