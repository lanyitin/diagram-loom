//! 產生 `fixtures/sample.loom`：`mise run fixture`。
//!
//! 這不是測試，是產生器——所以標了 `#[ignore]`，平常不會跑。
//! 用測試而不是 bin，是因為素材的建構器住在 `tests/common`，
//! 而它刻意只用公開 API 組成（見那份檔案的說明）。
//!
//! 範例專案**刻意留了破洞**。全綠的畫面看不出設計對不對，
//! 而且真實的專案本來就長這樣。

mod common;

use common::*;
use loom_core::environment::{Endpointing, InstanceRef};
use loom_core::id::Id;
use loom_core::repository;

#[test]
#[ignore = "產生器，不是測試。用 mise run fixture 執行"]
fn dump_sample_fixture() {
    let mut project = healthy_project();
    project.name = "網路商店（範例）".into();

    // ① prod 的 expect 寫 4，實際只有 3 台 → L004，格子變紅。
    //    這是萬用字元最危險的情境：少一台看起來完全正常。
    if let Endpointing::Instance { target, .. } = &mut project.environments[0].connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = Some(4);
    }

    // ② dev 完全沒接金流 → L001，格子空著。這就是使用者最怕的那格。
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_PAY));
    project.environments[2].systems.clear();

    // ③ test 的某條連線沒填用途 → L007，只是警告。
    project.environments[1].connections[0].purpose.clear();

    let destination =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample.loom");
    if destination.exists() {
        std::fs::remove_dir_all(&destination).expect("清掉舊的範例專案");
    }
    repository::save_to_dir(&project, &destination).expect("寫出範例專案");

    let findings = loom_core::lint::lint(&project);
    println!("\n已寫出 {}", destination.display());
    println!("刻意留下的問題共 {} 項：", findings.len());
    for f in &findings {
        println!("  {} {}", f.rule.code(), f.detail);
    }
}
