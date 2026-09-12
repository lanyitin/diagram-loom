//! 資源清單的**順序與分組**。
//!
//! # 為什麼這值得一份測試
//!
//! 分頁列的順序不是排版偏好，是**相依順序**：沒有系統就掛不了服務、
//! 沒有服務就寫不出契約、沒有機器就放不了服務實體。所以從頭讀到尾
//! 就是一份「從零開始怎麼建」的說明書。
//!
//! 這種東西壞掉的時候不會有人發現——畫面照樣畫得出來，只是順序變得
//! 沒有道理，而「沒有道理」不會讓任何測試變紅。所以要明著寫下來。

mod common;

use loom_core::inventory::{TableGroup, tables};
use loom_core::resource::Kind;

/// 只有標題，方便一眼比對。
fn titles(project: &loom_core::Project, env: Option<&loom_core::id::Id>) -> Vec<String> {
    tables(project, env).into_iter().map(|t| t.title).collect()
}

#[test]
fn the_tab_order_is_the_order_you_build_things_in() {
    let project = common::healthy_project();
    let env = project.environments.first().map(|e| &e.id);

    assert_eq!(
        titles(&project, env),
        vec![
            // 專案層級。不進分頁列，但在清單裡——MCP 靠這份查 id。
            "環境",
            // 邏輯層（母版）。人不依賴任何東西，所以它最前面。
            "人",
            "系統",
            "服務",
            "接點定義",
            "契約",
            // 環境層（分身）。機器與設備是先有的地，實體才放得上去。
            "機器",
            "設備",
            "設備接點",
            "服務實體",
            "外部系統實體",
        ],
    );
}

#[test]
fn every_table_says_which_layer_it_belongs_to() {
    let project = common::healthy_project();
    let env = project.environments.first().map(|e| &e.id);

    let groups: Vec<(String, TableGroup)> = tables(&project, env)
        .into_iter()
        .map(|t| (t.title, t.group))
        .collect();

    for (title, group) in &groups {
        let expected = match title.as_str() {
            "環境" => TableGroup::Project,
            "人" | "系統" | "服務" | "接點定義" | "契約" => TableGroup::Logical,
            _ => TableGroup::Environment,
        };
        assert_eq!(group, &expected, "「{title}」歸錯層了");
    }
}

#[test]
fn the_environments_table_is_not_scoped_to_an_environment() {
    // 它列的是**全部**環境，所以不隸屬於任何一個。畫面就是靠這一點
    // 把它從分頁列拿掉、改放進環境選單的。
    let project = common::healthy_project();
    let env = project.environments.first().map(|e| &e.id);

    let table = tables(&project, env)
        .into_iter()
        .find(|t| t.group == TableGroup::Project)
        .expect("找不到專案層級的表");

    assert_eq!(table.title, "環境");
    assert_eq!(table.environment, None);
    assert_eq!(table.rows.len(), project.environments.len());
}

#[test]
fn without_an_environment_only_the_master_copy_shows_up() {
    // 剛開一個空專案就是這樣：還沒有環境，所以環境層一張表都沒有。
    let project = common::healthy_project();

    let groups: Vec<TableGroup> = tables(&project, None)
        .into_iter()
        .map(|t| t.group)
        .collect();

    assert!(
        !groups.contains(&TableGroup::Environment),
        "沒指定環境時不該有環境層的表",
    );
    assert!(groups.contains(&TableGroup::Logical));
}

#[test]
fn every_table_has_a_memo_column_and_it_is_last() {
    // 備註是**每一種**資源都有的欄位。九張表各補一次的話，
    // 遲早會漏掉一張——而漏掉的那一種，備註就是「存得進去、看不到」。
    let project = common::healthy_project();
    let env = project.environments.first().map(|e| &e.id);

    for table in tables(&project, env) {
        assert_eq!(
            table.columns.last().map(String::as_str),
            Some("備註"),
            "{} 這張表的最後一欄不是備註",
            table.title,
        );
        for row in &table.rows {
            assert_eq!(
                row.cells.len(),
                table.columns.len(),
                "{} 的格子數跟欄位數對不上",
                table.title,
            );
        }
    }
}

#[test]
fn a_memo_shows_up_in_the_table() {
    let mut project = common::healthy_project();
    project.logical.containers[0].memo = "等年底汰換".into();
    let env = project.environments.first().map(|e| &e.id);

    let containers = tables(&project, env)
        .into_iter()
        .find(|t| t.title == "服務")
        .expect("找不到服務的表");

    assert!(
        containers
            .rows
            .iter()
            .any(|r| r.cells.last().map(String::as_str) == Some("等年底汰換")),
        "備註沒有出現在表格裡",
    );
}

/// `Kind` 有幾種，分頁列就該有幾張表。
///
/// # 為什麼這條比「順序對不對」更要緊
///
/// 少一張表**不會讓任何東西壞掉**：Rust 存得下，`resource::write_into`
/// 收得了，MCP 也寫得進去——只有人在畫面上建不了。沒有錯誤訊息，
/// 沒有規則會叫，跟備註那一欄當初缺席的方式一模一樣。
///
/// 「設備接點」就這樣缺了很久，而它的後果特別遠：
/// [`loom_core::connect::choices`] 列的是設備身上的 **VIP**，所以一台
/// 沒有 VIP 的 F5 在「補一條連線」的目標選單裡是**完全看不見的**。
/// 使用者看到的是「選單裡沒有 F5」，而真正的原因隔了三層。
#[test]
fn every_kind_of_resource_has_a_table() {
    let project = common::healthy_project();
    let env = project.environments.first().map(|e| &e.id);

    // 用 `Debug` 的字串比，只是為了不必替領域型別加一個 `Ord`——
    // 排序在這裡是測試的方便，不是模型的性質。
    let mut has: Vec<String> = tables(&project, env)
        .into_iter()
        .map(|t| format!("{:?}", t.kind))
        .collect();
    has.sort();

    let mut all: Vec<String> = all_kinds().iter().map(|k| format!("{k:?}")).collect();
    all.sort();

    assert_eq!(has, all, "有種類沒有自己的表，畫面上就建不出來");
}

/// 全部的 `Kind`。
///
/// 底下那個 `match` 沒有 `_` 分支，只為了一件事：**新增一種資源時這裡
/// 編不過**，逼著人回來補。否則這份清單自己會過期，而過期的清單
/// 會讓上面那條測試安靜地變成「我跟我自己一樣」。
fn all_kinds() -> Vec<Kind> {
    let all = vec![
        Kind::Person,
        Kind::System,
        Kind::Container,
        Kind::EndpointDef,
        Kind::Relationship,
        Kind::Environment,
        Kind::Node,
        Kind::Infra,
        Kind::InfraEndpoint,
        Kind::Instance,
        Kind::SystemInstance,
    ];
    for kind in &all {
        match kind {
            Kind::Person
            | Kind::System
            | Kind::Container
            | Kind::EndpointDef
            | Kind::Relationship
            | Kind::Environment
            | Kind::Node
            | Kind::Infra
            | Kind::InfraEndpoint
            | Kind::Instance
            | Kind::SystemInstance => {}
        }
    }
    all
}
