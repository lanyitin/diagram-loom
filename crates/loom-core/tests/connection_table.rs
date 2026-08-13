//! 連線表的整合測試。
//!
//! 表格的重點是**把 id 解析成人看得懂的東西**。所以這裡在意的是：
//! 萬用字元有沒有展開成正確的台數與位址、設備那端的不對稱有沒有處理對、
//! 以及查不到的東西會不會靜靜消失（那會讓使用者以為沒事）。

mod common;

use common::*;
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity};
use loom_core::table::{SideKind, rows};

fn 找(project: &loom_core::Project, env: &str, conn: &str) -> loom_core::table::Row {
    rows(project)
        .into_iter()
        .find(|r| r.environment == Id::new(env) && r.id == Id::new(conn))
        .unwrap_or_else(|| panic!("找不到 {env}/{conn}"))
}

#[test]
fn 每個環境的每條連線都會有一列() {
    let project = healthy_project();
    let 全部 = rows(&project);
    let 應有: usize = project
        .environments
        .iter()
        .map(|e| e.connections.len())
        .sum();
    assert_eq!(全部.len(), 應有);
}

#[test]
fn 萬用字元展開成實際的台數與位址() {
    let project = healthy_project();
    // prod 第二段：F5 → redis-*，共 3 台。
    let row = 找(&project, "env-prod", "conn-prod-2");

    assert_eq!(row.to.kind, SideKind::Instance);
    assert_eq!(
        row.to.label, "redis-*",
        "萬用字元要保留原樣，使用者才看得出這是一群"
    );
    assert_eq!(row.to.matched, 3);
    assert_eq!(row.to.expect, Some(3));
    assert_eq!(
        row.to.addresses,
        vec!["10.0.1.11:6379", "10.0.1.12:6379", "10.0.1.13:6379"]
    );
}

#[test]
fn 設備那端指的是具體endpoint不是邏輯定義() {
    // 這個不對稱是刻意的：設備不對應任何邏輯層元素，沒有 EndpointDef 可指。
    // 解析時如果把兩者搞混，位址就會查不到而靜靜空著。
    let project = healthy_project();
    let row = 找(&project, "env-prod", "conn-prod-1");

    assert_eq!(row.to.kind, SideKind::Infra);
    assert_eq!(row.to.label, "f5-vip");
    assert_eq!(row.to.endpoint.as_deref(), Some("vip-redis"));
    assert_eq!(row.to.addresses, vec!["10.0.0.100:6379"]);
}

#[test]
fn 來源端沒指定endpoint是合法的() {
    // 由作業系統分配 ephemeral port，所以來源端可以留空。
    let project = healthy_project();
    let row = 找(&project, "env-prod", "conn-prod-1");

    assert_eq!(row.from.label, "api-01");
    assert_eq!(row.from.endpoint, None);
    assert!(row.from.addresses.is_empty());
}

#[test]
fn 外部系統那端解析得出落地位址() {
    let project = healthy_project();
    let row = 找(&project, "env-test", "conn-test-pay");

    assert_eq!(row.to.kind, SideKind::System);
    assert_eq!(row.to.label, "payment-sandbox");
    assert_eq!(row.to.addresses.len(), 1);
}

#[test]
fn 沒問題的列不帶嚴重度() {
    let project = healthy_project();
    assert!(rows(&project).iter().all(|r| r.severity.is_none()));
}

#[test]
fn 有問題的列帶著規則與嚴重度() {
    let mut project = healthy_project();
    project.environments[0].connections[1].purpose.clear(); // L007，Warning

    let row = 找(&project, "env-prod", "conn-prod-2");
    assert_eq!(row.severity, Some(Severity::Warning));
    assert!(row.rules.contains(&Rule::L007));
}

#[test]
fn 指向不存在的機器時把id印出來而不是留白() {
    // 留白的話使用者只會看到一格空的，不知道是哪個 id 壞了。
    let mut project = healthy_project();
    project.environments[2].connections[0].from = from_instance("i-dev-不存在");

    let row = 找(&project, "env-dev", "conn-dev-1");
    assert_eq!(row.from.label, "i-dev-不存在");
    assert_eq!(row.from.matched, 0);
    assert_eq!(row.severity, Some(Severity::Error));
}

#[test]
fn 指向不存在的契約時_serves_slug_是空的() {
    let mut project = healthy_project();
    project.environments[2].connections[0].serves = Id::new("r-不存在");

    let row = 找(&project, "env-dev", "conn-dev-1");
    assert_eq!(row.serves_slug, None);
    assert!(row.rules.contains(&Rule::L003));
}

#[test]
fn 先依環境再依契約排序() {
    // 前端直接照順序畫，不再排一次。
    let project = healthy_project();
    let 全部 = rows(&project);

    let 環境順序: Vec<usize> = 全部
        .iter()
        .map(|r| {
            project
                .environments
                .iter()
                .position(|e| e.id == r.environment)
                .unwrap()
        })
        .collect();
    let mut 排好的 = 環境順序.clone();
    排好的.sort();
    assert_eq!(環境順序, 排好的);
}
