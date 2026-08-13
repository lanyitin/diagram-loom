//! 匯入預覽的整合測試。
//!
//! 預覽這一步的存在理由是：匯入是唯一會大量改動資料的操作，貼錯一格
//! 就洗掉一個環境。所以這裡最在意的是**預覽有沒有說謊**——
//! 它列出來的東西，必須就是套用之後真正發生的事。

mod common;

use common::*;
use loom_core::importer::{Sheet, row_from_pairs, template_headers};
use loom_core::plan::{ChangeKind, Element, plan};

fn sheet(rows: Vec<Vec<String>>) -> Sheet {
    Sheet::new(template_headers(), rows)
}

fn row(env: &str, from: &str, to: &str, address: &str) -> Vec<String> {
    row_from_pairs(&[
        ("environment", env),
        ("purpose", "壓力測試"),
        ("serves", "api-連-redis"),
        ("from_node", from),
        ("from_service", "order-api"),
        ("to_node", to),
        ("to_service", "redis"),
        ("to_endpoint", "client-port"),
        ("to_address", address),
        ("protocol", "TCP"),
    ])
}

#[test]
fn importing_into_a_fresh_environment_is_all_additions() {
    let project = healthy_project();
    let (p, _) = plan(
        &project,
        &sheet(vec![row("stage", "vm-a", "vm-b", "10.9.0.1:6379")]),
    )
    .unwrap();

    assert!(p.changes.iter().all(|c| c.kind == ChangeKind::Added));
    assert_eq!(p.environments, vec!["stage"]);

    let kind: Vec<Element> = {
        let mut k: Vec<_> = p.changes.iter().map(|c| c.element).collect();
        k.sort();
        k.dedup();
        k
    };
    assert!(kind.contains(&Element::Environment));
    assert!(kind.contains(&Element::DeploymentNode));
    assert!(kind.contains(&Element::ContainerInstance));
    assert!(kind.contains(&Element::Connection));
}

#[test]
fn preview_leaves_the_original_project_alone() {
    // 預覽如果有副作用，使用者按「取消」之後資料就已經被改了。
    let project = healthy_project();
    let original = project.clone();

    let _ = plan(
        &project,
        &sheet(vec![row("stage", "vm-a", "vm-b", "10.9.0.1:6379")]),
    )
    .unwrap();

    assert_eq!(project, original);
}

#[test]
fn the_preview_is_exactly_what_apply_produces() {
    // 這是整份測試最重要的一條。預覽與實際若不一致，這個步驟就只是裝飾。
    let project = healthy_project();
    let sheet = sheet(vec![
        row("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        row("prod", "vm-api-01", "vm-new", "10.0.1.99:6379"),
    ]);

    let (p, applied) = plan(&project, &sheet).unwrap();

    // 拿套用後的結果再預覽一次同一張表——應該完全沒有變更可做。
    let (again, _) = plan(&applied, &sheet).unwrap();
    assert!(
        again.changes.is_empty(),
        "預覽宣稱會做這些事：{:?}\n但套用後再跑一次還有變更：{:?}",
        p.changes.len(),
        again.changes
    );
    assert!(again.unchanged > 0);
}

#[test]
fn reimporting_the_same_file_creates_no_duplicate_connections() {
    // 重新匯入更新過的 Excel 是最常見的用法。連線若不 upsert，
    // 每匯一次就整份複製一遍，而且 lint 不會抱怨——使用者根本不會發現。
    let project = healthy_project();
    let sheet = sheet(vec![row("stage", "vm-a", "vm-b", "10.9.0.1:6379")]);

    let (_, first) = plan(&project, &sheet).unwrap();
    let (_, second) = plan(&first, &sheet).unwrap();

    let connection_count = |p: &loom_core::Project| {
        p.environments
            .iter()
            .find(|e| e.slug == "stage")
            .unwrap()
            .connections
            .len()
    };
    assert_eq!(connection_count(&first), 1);
    assert_eq!(connection_count(&second), 1, "第二次匯入把連線複製了一份");
}

#[test]
fn a_changed_address_warns_on_reimport_instead_of_overwriting() {
    // ⚠️ 這條測的是現行規則（docs/excel-import.md）：位址第一次出現時建立，
    // 之後不一致就警告、不覆蓋。
    //
    // 那條規則當初是為了「同一份檔案裡五列重複寫同一台 Redis 的位址」。
    // 套用在「改了 IP 重新匯入」上的後果是：**試算表改不動已經存在的位址**。
    // 對一個預期用 Excel 維護數百條連線的人來說，這很可能不是他要的。
    //
    // 先照文件走，但把它釘成測試——哪天決定要改，這裡會提醒你那是刻意的取捨。
    let project = healthy_project();
    let first = sheet(vec![row("stage", "vm-a", "vm-b", "10.9.0.1:6379")]);
    let (_, before) = plan(&project, &first).unwrap();

    let changed = sheet(vec![row("stage", "vm-a", "vm-b", "10.9.0.99:6379")]);
    let (p, _) = plan(&before, &changed).unwrap();

    let address_changes: Vec<_> = p
        .changes
        .iter()
        .filter(|c| c.element == Element::Address && c.kind == ChangeKind::Updated)
        .collect();
    assert!(
        address_changes.is_empty(),
        "現行規則不覆蓋位址：{address_changes:?}"
    );

    assert_eq!(p.warnings.len(), 1, "但一定要講出來：{:?}", p.warnings);
    assert!(
        p.warnings[0].contains("10.9.0.1:6379"),
        "要說清楚保留了哪個"
    );
    assert!(p.warnings[0].contains("10.9.0.99:6379"), "以及忽略了哪個");
}

#[test]
fn an_address_conflict_becomes_a_warning_not_a_silent_drop() {
    // 同一個 Endpoint 在兩列填了不同位址。工具保留先出現的，
    // 但一定要講出來——不然使用者會以為兩個都寫進去了。
    let project = healthy_project();
    let sheet = sheet(vec![
        row("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        row("stage", "vm-a", "vm-b", "10.9.0.2:6379"),
    ]);

    let (p, _) = plan(&project, &sheet).unwrap();
    assert_eq!(p.warnings.len(), 1, "警告：{:?}", p.warnings);
    assert!(p.warnings[0].contains("10.9.0.2:6379"));
}

#[test]
fn a_bad_header_rejects_the_whole_sheet() {
    let project = healthy_project();
    let broken_sheet = Sheet::new(vec!["environment".into()], vec![vec!["prod".into()]]);
    assert!(plan(&project, &broken_sheet).is_err());
}

#[test]
fn a_sheet_touching_several_environments_lists_them_all() {
    let project = healthy_project();
    let sheet = sheet(vec![
        row("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        row("uat", "vm-c", "vm-d", "10.8.0.1:6379"),
        row("stage", "vm-a", "vm-e", "10.9.0.2:6379"),
    ]);

    let (p, _) = plan(&project, &sheet).unwrap();
    assert_eq!(p.environments, vec!["stage", "uat"], "重複的環境只列一次");
}

#[test]
fn changes_are_grouped_and_sorted_by_kind() {
    // 前端直接照順序畫，同一類的變更會排在一起。
    let project = healthy_project();
    let (p, _) = plan(
        &project,
        &sheet(vec![
            row("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
            row("stage", "vm-c", "vm-d", "10.9.0.2:6379"),
        ]),
    )
    .unwrap();

    let kind: Vec<Element> = p.changes.iter().map(|c| c.element).collect();
    let mut sorted = kind.clone();
    sorted.sort();
    assert_eq!(kind, sorted);
}
