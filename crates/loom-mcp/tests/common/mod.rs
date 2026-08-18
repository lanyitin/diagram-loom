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
        memo: String::new(),
    })))
}

/// 這幾個測試都只有一個專案，所以「開著哪些」永遠是這一份。
/// 挑選的規則本身由 `pick` 的單元測試守著。
fn only_one() -> Vec<loom_mcp::pick::Open> {
    vec![loom_mcp::pick::Open {
        slug: "測試".into(),
        name: "測試專案".into(),
        root: "/tmp/測試.loom".into(),
        dirty: false,
        errors: 0,
        warnings: 0,
    }]
}

/// 叫一個工具，成功就回文字，失敗就 panic 並附上原因。
pub fn ok(ws: &mut Desk, name: &str, args: Value) -> String {
    tools::call(ws, &only_one(), name, &args).unwrap_or_else(|e| panic!("{name} 失敗了：{e}"))
}

/// 叫一個工具並預期失敗，回錯誤訊息。
pub fn err(ws: &mut Desk, name: &str, args: Value) -> String {
    match tools::call(ws, &only_one(), name, &args) {
        Err(e) => e,
        Ok(text) => panic!("{name} 竟然成功了：{text}"),
    }
}

/// 建一個元素。
pub fn create(ws: &mut Desk, kind: &str, fields: Value) -> String {
    ok(ws, "create", json!({ "kind": kind, "fields": fields }))
}

/// **兩個**環境都建好、lint 也乾淨的一份小專案。
///
/// # 為什麼素材一定要有兩個環境
///
/// 單一環境的素材驗不出這一層最貴的那個缺口：`describe` 曾經寫死
/// `environments.first()`，於是第二個環境裡的機器與服務實體 Agent
/// **一個都看不到**——而它建得出來、也改得動。
///
/// 那種缺口沒有任何錯誤訊息，只會讓 Agent 以為 uat 是空的，
/// 然後把已經存在的東西再建一次。素材只有一個環境的話，
/// 整組測試都會是綠的。
pub fn a_two_environment_project() -> Desk {
    let mut ws = empty();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單系統", "external": false}},
            {"kind": "container", "fields": {"slug": "apache", "name": "反向代理", "system": "shop"}},
            {"kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "shop"}},
            {"kind": "endpoint_def", "fields": {"owner": "apache", "slug": "http", "protocol": "tcp"}},
            {"kind": "endpoint_def", "fields": {"owner": "redis", "slug": "client-port", "protocol": "tcp"}},
            {"kind": "person", "fields": {"slug": "customer", "name": "使用者"}},
            {"kind": "relationship", "fields": {
                "slug": "user-to-apache", "purpose": "使用者連進系統",
                "from": "person:customer", "to": "container:apache", "to_endpoint": "http"}},
            {"kind": "relationship", "fields": {
                "slug": "apache-to-redis", "purpose": "讀寫快取",
                "from": "container:apache", "to": "container:redis", "to_endpoint": "client-port"}},
            {"kind": "environment", "fields": {"slug": "prod", "name": "正式"}},
            {"kind": "environment", "fields": {"slug": "uat", "name": "驗收"}}
        ]}),
    );

    for (env, redis_count, net) in [("prod", 3, 1), ("uat", 2, 2)] {
        ok(
            &mut ws,
            "create_nodes",
            json!({"scope": {"environment": env}, "items": [
                {"container": "apache", "count": 1,
                 "address_template": format!("10.{net}.0.{{ip}}:8080"), "ip_start": 11},
                {"container": "redis", "count": redis_count,
                 "address_template": format!("10.{net}.1.{{ip}}:6379"), "ip_start": 11}
            ]}),
        );
        ok(
            &mut ws,
            "add_connection",
            json!({"scope": {"environment": env}, "items": [
                {"relationship": "user-to-apache"},
                {"relationship": "apache-to-redis"}
            ]}),
        );
    }

    assert_eq!(
        ok(&mut ws, "lint", json!({})),
        "沒有發現任何缺漏。",
        "素材本身就不乾淨"
    );
    ws
}

/// 從 describe 的某一張表裡撈出某個 slug 那一列的 id。
///
/// **一定要開 `detail: "full"`**：預設的 brief 不印 id 了，因為 `update`
/// 與 `delete` 的 `target` 已經吃名字。還留著這個 helper 是為了驗證
/// 「拿 id 當 target 照樣可以」——那是舊 Agent 設定的相容路徑。
///
/// **要指定是哪一張表。** 同一個名字會出現在不只一張表上：機器
/// `vm-redis-01` 那一列會列出它上面跑著的服務實體 `redis-01`，
/// 所以只比對「有沒有這個字」會撈到隔壁那張表的 id。
pub fn id_of(ws: &mut Desk, table: &str, slug: &str) -> String {
    let described = ok(ws, "describe", json!({"detail": "full"}));
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
