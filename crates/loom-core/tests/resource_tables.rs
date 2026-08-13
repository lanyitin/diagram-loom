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

    let groups: Vec<TableGroup> = tables(&project, None).into_iter().map(|t| t.group).collect();

    assert!(
        !groups.contains(&TableGroup::Environment),
        "沒指定環境時不該有環境層的表",
    );
    assert!(groups.contains(&TableGroup::Logical));
}
