//! 產生 `fixtures/通路系統.loom`：`mise run fixture:real`。
//!
//! 內容本身在 [`common::real`]——那份素材同時也被
//! `repository_roundtrip.rs` 拿來現建，所以不能只住在這個 `#[ignore]` 的檔裡。

mod common;

use common::real::project;

#[test]
#[ignore = "產生器，不是測試。用 mise run fixture:real 執行"]
fn dump_real_fixture() {
    let project = project();

    let destination =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/通路系統.loom");
    if destination.exists() {
        std::fs::remove_dir_all(&destination).expect("清掉舊的");
    }
    loom_core::repository::save_to_dir(&project, &destination).expect("寫出樣本");

    let instance_count: usize = project
        .environments
        .iter()
        .map(|e| {
            e.nodes
                .iter()
                .map(|n| n.instances_recursive().len())
                .sum::<usize>()
        })
        .sum();
    let connection_count: usize = project
        .environments
        .iter()
        .map(|e| e.connections.len())
        .sum();

    println!("\n已寫出 {}", destination.display());
    println!(
        "{} 個服務／{} 條契約／{} 個落地／{} 條連線",
        project.logical.containers.len(),
        project.logical.relationships.len(),
        instance_count,
        connection_count
    );

    println!("\nlint：");
    let findings = loom_core::lint::lint(&project);
    if findings.is_empty() {
        println!("  （乾淨）");
    }
    for f in &findings {
        println!("  {} {}", f.rule.code(), f.detail);
    }
}
