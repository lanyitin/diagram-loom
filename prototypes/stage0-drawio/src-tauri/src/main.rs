//! 階段 0 的可丟棄原型。
//!
//! 這支程式只有一個任務：回答「Tauri 能不能內嵌 draw.io」。
//! 它開一個視窗、讓前端的探針跑完一輪嵌入協定、把結果印到 stdout、然後結束。
//!
//! 刻意不引用 `loom-core`——這裡驗的是外殼，不是領域模型。
//! 它也有自己的 `[workspace]`，所以 `tauri` 這個相依不會混進 `loom-core` 的 workspace。
//!
//! ## 為什麼 draw.io 不放進 frontendDist
//!
//! 解壓後的 draw.io 有 152 MB。`frontendDist` 的內容會在編譯期被嵌進執行檔，
//! 152 MB 進去會讓編譯慢到不能用。所以改成註冊一個 `drawio://` 自訂協定，
//! 從磁碟讀檔供應——相對路徑照常運作，而且未來正式版可以直接改成讀
//! app bundle 裡的 resources 目錄。

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use tauri::http::{Response, StatusCode};

/// 探針卡住時的保險絲。沒有它，失敗的表現會是「視窗開著、終端機不動」。
/// 規模測試會跑很久，所以讓它可以調。
fn watchdog() -> Duration {
    let secs = std::env::var("LOOM_STAGE0_WATCHDOG")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(180);
    Duration::from_secs(secs)
}

static REPORTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
struct Check {
    name: String,
    ok: bool,
    detail: String,
}

/// 這輪要打誰：真的 draw.io，還是當對照組的假 draw.io。
fn target() -> String {
    std::env::var("LOOM_STAGE0_TARGET").unwrap_or_else(|_| "drawio".into())
}

/// `drawio://` 協定的根目錄。對照組與正式組走同一條管路，只有根目錄不同，
/// 所以兩者的差異一定來自內容而不是外殼。
fn document_root() -> PathBuf {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    match target().as_str() {
        "stub" => here.join("../web/stub"),
        _ => here.join("../../../vendor/drawio"),
    }
}

#[tauri::command]
fn target_command() -> String {
    target()
}

/// `flat`（第一輪的 2 節點素材）或 `nested`（三層巢狀 + 跨容器連線）。
#[tauri::command]
fn scenario() -> String {
    std::env::var("LOOM_STAGE0_SCENARIO").unwrap_or_else(|_| "flat".into())
}

/// 把探針手上的 XML 倒到磁碟。紅燈時「到底長什麼樣」比任何猜測都有用。
#[tauri::command]
fn dump(name: String, contents: String) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../out");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    let path = dir.join(safe);
    match std::fs::write(&path, contents) {
        Ok(()) => println!("倒出 {}", path.display()),
        Err(e) => eprintln!("倒出 {} 失敗：{e}", path.display()),
    }
}

#[tauri::command]
fn report(target: String, checks: Vec<Check>) {
    REPORTED.store(true, Ordering::SeqCst);

    let passed = checks.iter().filter(|c| c.ok).count();
    let total = checks.len();

    println!("\n╭─ 階段 0 結果（目標：{target}）");
    for c in &checks {
        println!("│ {} {}", if c.ok { "OK" } else { "xx" }, c.name);
        if !c.detail.is_empty() {
            println!("│      {}", c.detail);
        }
    }
    println!("╰─ {passed}/{total} 通過\n");

    // 視窗會在行程結束時一起收掉。用離開碼把結論帶給呼叫者。
    exit(if passed == total && total > 0 { 0 } else { 1 });
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") | Some("webmanifest") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("wasm") => "application/wasm",
        Some("xml") => "text/xml; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

/// 把請求路徑限制在根目錄底下。`..` 會被丟掉。
///
/// 這支程式跑完就丟，但供應本機檔案的程式碼一旦被複製走，路徑穿越就跟著走，
/// 所以還是在這裡擋。
fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path);
    let mut out = root.to_path_buf();
    for part in Path::new(decoded.trim_start_matches('/')).components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn serve(root: &Path, url_path: &str) -> Response<Cow<'static, [u8]>> {
    let plain = |code: StatusCode, msg: &str| {
        Response::builder()
            .status(code)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Cow::Owned(msg.as_bytes().to_vec()))
            .unwrap()
    };

    let Some(path) = resolve(root, url_path) else {
        return plain(StatusCode::FORBIDDEN, "路徑穿越");
    };

    match std::fs::read(&path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", mime_for(&path))
            // 讓 iframe 裡的 draw.io 能被父視窗以 postMessage 溝通，
            // 同時不允許它被塞進別的地方。
            .header("Access-Control-Allow-Origin", "*")
            .body(Cow::Owned(bytes))
            .unwrap(),
        Err(e) => {
            eprintln!("drawio:// 找不到 {}：{e}", path.display());
            plain(StatusCode::NOT_FOUND, &format!("找不到 {url_path}"))
        }
    }
}

fn main() {
    let root = document_root();
    println!("目標：{}", target());
    println!("drawio:// 根目錄：{}", root.display());
    if !root.exists() {
        eprintln!("\nxx 根目錄不存在。draw.io 還沒解壓？\n");
        exit(2);
    }

    let 保險絲 = watchdog();
    thread::spawn(move || {
        thread::sleep(保險絲);
        if !REPORTED.load(Ordering::SeqCst) {
            eprintln!("\nxx 探針在 {保險絲:?} 內沒有回報——視窗可能整個沒起來。\n");
            exit(2);
        }
    });

    tauri::Builder::default()
        .register_uri_scheme_protocol("drawio", move |_ctx, request| {
            serve(&root, request.uri().path())
        })
        .invoke_handler(tauri::generate_handler![target_command, scenario, dump, report])
        .run(tauri::generate_context!())
        .expect("Tauri 啟動失敗");
}
