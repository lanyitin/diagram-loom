//! 權限檔的守門測試。
//!
//! # 為什麼這值得一個測試
//!
//! 權限少給的後果**完全看不出來**：前端呼叫被拒絕，Promise 靜靜地 reject，
//! 畫面上什麼都沒有。這裡真的踩過一次——
//!
//! `CloseGuard` 註冊了 `onCloseRequested`。Tauri 一看到有 JS 監聽這個事件，
//! 就會自動 `prevent_close()`，把「關不關」的主導權交給前端；
//! 前端的 wrapper 接著呼叫 `destroy()` 真的關掉。
//!
//! 但 `core:default` **不包含** `core:window:allow-destroy`。於是：
//!
//! 1. 按下關閉 → Tauri 擋下來，等前端決定
//! 2. 前端呼叫 `destroy()` → 權限不足，被拒
//! 3. 視窗永遠關不掉，程序也永遠不會結束
//!
//! 沒有任何錯誤訊息。加一個監聽器就把「關閉視窗」整個弄壞了。

use std::collections::HashSet;

fn permissions() -> HashSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("讀不到 {}：{e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("權限檔不是合法的 JSON");

    json["permissions"]
        .as_array()
        .expect("權限檔少了 permissions 陣列")
        .iter()
        .map(|v| v.as_str().expect("權限必須是字串").to_string())
        .collect()
}

#[test]
fn the_frontend_can_close_the_window() {
    // 只要 CloseGuard 還在監聽 onCloseRequested，這條就不能拿掉，
    // 否則視窗會關不起來、程序不會結束，而且完全沒有錯誤訊息。
    assert!(
        permissions().contains("core:window:allow-destroy"),
        "少了 core:window:allow-destroy。CloseGuard 監聽了 onCloseRequested，\
         Tauri 因此會自動 prevent_close()，關不關由前端決定；\
         前端的 destroy() 被權限擋下來的話，視窗就永遠關不掉了。"
    );
}

#[test]
fn only_the_permissions_we_need() {
    // 這個檔是安全邊界，不是設定樣板。多開一條就要在這裡多一行，
    // 而多的那一行會逼人想一下「這個真的需要嗎」。
    let expected: HashSet<String> = [
        // 事件、路徑、視窗查詢等等的基本盤。
        "core:default",
        // 見上面那個測試。
        "core:window:allow-destroy",
        // 開啟專案資料夾用的檔案選擇器。**只有 open，沒有 save**——
        // 存檔的路徑是專案資料夾，不該由對話框決定。
        "dialog:allow-open",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    assert_eq!(permissions(), expected);
}
