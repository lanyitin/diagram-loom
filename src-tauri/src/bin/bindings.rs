//! 產生 TypeScript 型別：`mise run bindings`。
//!
//! 跟 App 共用同一個 `builder()`，所以不可能出現「command 加了但型別沒更新」。
//! `mise run check` 會跑這支並確認 git 上的檔案沒有變化——忘了重新產生就會紅燈。

use specta_typescript::Typescript;

fn main() {
    // 相對於這個 crate 而不是相對於 cwd——否則從哪個目錄跑會決定檔案掉在哪。
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/lib/bindings.ts")
            .display()
            .to_string()
    });

    diagram_loom_lib::builder()
        .export(
            // specta 拒絕匯出 usize / u64（怕超過 JS 的安全整數範圍），
            // 而且 0.0.12 沒有全域開關。所以會跨邊界的計數一律用 u32——
            // 「期望有幾台機器」本來也不需要 64 位元。
            Typescript::default().header("// 由 `mise run bindings` 產生，不要手改。\n"),
            &out,
        )
        .expect("產生 TypeScript 型別失敗");

    println!("已寫出 {out}");
}
