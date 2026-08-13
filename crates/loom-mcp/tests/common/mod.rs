//! 讓測試可以扮演一個 Agent。
//!
//! 這裡的 [`Desk`] 用的是**桌面 App 用的同一個 [`History`]**，不是一個假的
//! 資料袋。所以測試裡的「Agent 改了一筆」跟真的走過 MCP 端點是同一條路徑，
//! 復原、未儲存標記、lint 都一起被驗到。

// 每個整合測試各自是一個 crate，只用得到這裡的一部分。
#![allow(dead_code)]

use serde_json::{Value, json};

use loom_core::Project;
use loom_core::edit::Edit;
use loom_core::history::History;
use loom_core::id::Id;
use loom_core::logical::Logical;
use loom_mcp::{Workspace, tools};

/// 最小的 Workspace：一個 History，跟桌面 App 用的是同一個型別。
pub struct Desk(pub Option<History>);

impl Workspace for Desk {
    fn project(&self) -> Option<&Project> {
        self.0.as_ref().map(History::project)
    }
    fn edit(&mut self, edit: &Edit) -> Result<(), String> {
        self.0
            .as_mut()
            .ok_or("沒有開專案")?
            .edit(edit)
            .map_err(|e| e.to_string())
    }
}

pub fn empty() -> Desk {
    Desk(Some(History::opened(Project {
        id: Id::generate(),
        slug: "new".into(),
        name: "全新專案".into(),
        logical: Logical {
            people: vec![],
            systems: vec![],
            containers: vec![],
            relationships: vec![],
        },
        environments: vec![],
    })))
}

/// 叫一個工具，成功就回文字，失敗就 panic 並附上原因。
pub fn ok(ws: &mut Desk, name: &str, args: Value) -> String {
    tools::call(ws, name, &args).unwrap_or_else(|e| panic!("{name} 失敗了：{e}"))
}

/// 叫一個工具並預期失敗，回錯誤訊息。
pub fn err(ws: &mut Desk, name: &str, args: Value) -> String {
    match tools::call(ws, name, &args) {
        Err(e) => e,
        Ok(text) => panic!("{name} 竟然成功了：{text}"),
    }
}

/// 建一個元素。
pub fn create(ws: &mut Desk, kind: &str, fields: Value) -> String {
    ok(ws, "create", json!({ "kind": kind, "fields": fields }))
}

/// 從 describe 的某一張表裡撈出某個 slug 那一列的 id。
///
/// Agent 真的就是這樣拿 id 的——它看不到畫面，只有這幾張表。
///
/// **要指定是哪一張表。** 同一個名字會出現在不只一張表上：機器
/// `vm-redis-01` 那一列會列出它上面跑著的服務實體 `redis-01`，
/// 所以只比對「有沒有這個字」會撈到隔壁那張表的 id。
pub fn id_of(ws: &mut Desk, table: &str, slug: &str) -> String {
    let described = ok(ws, "describe", json!({}));
    let mut here = false;
    for line in described.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            here = title.starts_with(table);
            continue;
        }
        if !here {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.contains(&slug) {
            return cells
                .get(1)
                .unwrap_or_else(|| panic!("這一列沒有 id：{line}"))
                .to_string();
        }
    }
    panic!("describe 的「{table}」表裡找不到 {slug}：\n{described}");
}
