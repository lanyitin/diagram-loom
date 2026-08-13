//! 把 draw.io 的靜態檔供應給 iframe。
//!
//! # 為什麼要自訂協定，不放 `frontendDist`
//!
//! 解壓後 **152 MB**。`frontendDist` 的內容會在編譯期嵌進執行檔，
//! 塞進去編譯會慢到不能用（見 `docs/stage0-findings.md` 第一節）。
//!
//! 所以註冊 `drawio://`，從磁碟供應。副作用是 draw.io 落在**自己的 origin**
//! 上，跟主視窗跨來源——那反而是好事：嵌入協定本來就是為跨來源設計的，
//! 而且主視窗的 CSP 不會意外綁住 draw.io。
//!
//! # ⚠️ 路徑穿越
//!
//! 這是一個「把使用者給的路徑轉成檔案」的函式，也就是最典型會被
//! `../../../../etc/passwd` 打穿的地方。iframe 的內容雖然是我們自己包的，
//! 但那份 draw.io 是第三方的一大包 JavaScript——**不該假設它只會要它該要的東西**。
//!
//! 擋法不是掃字串裡有沒有 `..`（那會被 `%2e%2e` 之類的繞過），
//! 而是**正規化之後確認還在根目錄底下**。

use std::path::{Component, Path, PathBuf};

/// 把請求的路徑轉成磁碟上的檔案。
///
/// 回傳 `None` 表示這個路徑不該供應——可能是穿越、也可能是根本沒這個檔。
pub fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    // 去掉查詢字串與錨點。draw.io 的網址一定帶一長串參數。
    let path = url_path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches('/');

    let path = percent_decode(path);
    let path = if path.is_empty() { "index.html" } else { &path };

    // 逐段檢查。只接受一般的檔名——`..`、絕對路徑、Windows 的磁碟代號
    // 全部擋掉，而不是去掃字串。
    let mut out = root.to_path_buf();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(name) => out.push(name),
            // `./` 無害，跳過就好。
            Component::CurDir => {}
            _ => return None,
        }
    }

    // 前面那圈已經擋掉所有穿越，這裡再確認一次實際落點還在根目錄底下——
    // 符號連結可以繞過純字串的檢查，而 draw.io 那包裡有沒有連結我們不知道。
    let real_root = root.canonicalize().ok()?;
    let real = out.canonicalize().ok()?;
    if !real.starts_with(&real_root) {
        return None;
    }
    real.is_file().then_some(real)
}

/// 副檔名決定 MIME。**猜不到就回 `application/octet-stream`**——
/// 猜錯型別會讓瀏覽器拒絕執行，而那個錯誤訊息完全看不出是這裡的問題。
pub fn mime_of(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml; charset=utf-8",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// 只解 `%XX`。draw.io 的路徑裡會有空白與括號。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some((hi, lo)) = hex(bytes[i + 1]).zip(hex(bytes[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一個小小的假 draw.io，外面再放一個不該被拿到的檔案。
    fn fixture() -> (tempdir::Dir, PathBuf) {
        let dir = tempdir::Dir::new("drawio-serve");
        let root = dir.path().join("vendor");
        std::fs::create_dir_all(root.join("js")).unwrap();
        std::fs::write(root.join("index.html"), "<html>").unwrap();
        std::fs::write(root.join("js").join("app.min.js"), "//").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "不該被拿到").unwrap();
        (dir, root)
    }

    #[test]
    fn serves_a_file_under_the_root() {
        let (_dir, root) = fixture();
        assert!(resolve(&root, "/js/app.min.js").is_some());
    }

    #[test]
    fn an_empty_path_means_index_html() {
        let (_dir, root) = fixture();
        assert!(resolve(&root, "/").is_some());
        assert!(resolve(&root, "").is_some());
    }

    #[test]
    fn the_query_string_is_not_part_of_the_path() {
        // draw.io 的網址一定帶一長串參數。連著查就會變成「找不到檔案」，
        // 而那個症狀是「整個編輯器一片空白」，看不出是這裡的問題。
        let (_dir, root) = fixture();
        assert!(resolve(&root, "/index.html?embed=1&proto=json").is_some());
        assert!(resolve(&root, "/index.html#anchor").is_some());
    }

    mod path_traversal {
        use super::*;

        #[test]
        fn dot_dot_is_rejected() {
            let (_dir, root) = fixture();
            assert_eq!(resolve(&root, "/../secret.txt"), None);
            assert_eq!(resolve(&root, "/js/../../secret.txt"), None);
        }

        #[test]
        fn percent_encoded_dot_dot_is_rejected() {
            // 掃字串的作法會被這個繞過去，所以擋法是「正規化之後確認還在根底下」。
            let (_dir, root) = fixture();
            assert_eq!(resolve(&root, "/%2e%2e/secret.txt"), None);
            assert_eq!(resolve(&root, "/%2E%2E%2Fsecret.txt"), None);
        }

        #[test]
        fn an_absolute_path_is_rejected() {
            let (_dir, root) = fixture();
            assert_eq!(resolve(&root, "//etc/passwd"), None);
        }

        #[test]
        fn a_symlink_pointing_outside_is_rejected() {
            // 純字串檢查擋不掉這個。那包 draw.io 是第三方的，
            // 不該假設它裡面沒有連結。
            let (dir, root) = fixture();
            #[cfg(unix)]
            std::os::unix::fs::symlink(dir.path().join("secret.txt"), root.join("escape")).unwrap();
            #[cfg(unix)]
            assert_eq!(resolve(&root, "/escape"), None);
            let _ = dir;
        }
    }

    #[test]
    fn a_missing_file_is_not_served() {
        let (_dir, root) = fixture();
        assert_eq!(resolve(&root, "/nope.js"), None);
    }

    #[test]
    fn a_directory_is_not_served() {
        let (_dir, root) = fixture();
        assert_eq!(resolve(&root, "/js"), None);
    }

    #[test]
    fn the_mime_types_cover_what_drawio_actually_ships() {
        // 猜錯型別會讓瀏覽器拒絕執行，而那個錯誤看不出是這裡的問題。
        for (name, want) in [
            ("index.html", "text/html; charset=utf-8"),
            ("app.min.js", "text/javascript; charset=utf-8"),
            ("app.css", "text/css; charset=utf-8"),
            ("shapes.svg", "image/svg+xml"),
            ("logo.png", "image/png"),
            ("font.woff2", "font/woff2"),
            ("Graph.wasm", "application/wasm"),
        ] {
            assert_eq!(mime_of(Path::new(name)), want, "{name}");
        }
        assert_eq!(
            mime_of(Path::new("weird.qqq")),
            "application/octet-stream",
            "猜不到就該回 octet-stream，不要瞎猜一個"
        );
    }

    /// 最小的暫存資料夾。不為了一個測試拉一個相依進來。
    mod tempdir {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU32, Ordering};

        static N: AtomicU32 = AtomicU32::new(0);

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new(tag: &str) -> Self {
                let n = N.fetch_add(1, Ordering::Relaxed);
                let path =
                    std::env::temp_dir().join(format!("loom-{tag}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).unwrap();
                Dir(path)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
