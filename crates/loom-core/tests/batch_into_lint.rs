//! 批次建立 → 放進環境 → 跑 lint 的整合測試。
//!
//! 這條路徑是實際使用時最常走的：使用者填一次樣板生出 12 台機器，
//! 然後 lint 應該安靜。單元測試各自驗證兩邊都對，但「生出來的東西
//! 能不能通過 lint」只有把它們接起來才知道。

mod common;

use common::*;
use loom_core::batch::{BatchSpec, EndpointPlan, expand};
use loom_core::environment::{Endpointing, InstanceRef, NodeKind};
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};
use loom_core::logical::Protocol;

/// 生出 `count` 台 Redis 的規格。名稱與位址的起始值刻意錯開，
/// 對應現實中 `redis-01`～`redis-12` 配 `10.0.1.11`～`10.0.1.22` 的情形。
fn redis_batch(count: usize) -> BatchSpec {
    BatchSpec {
        count,
        name_template: "redis-{n}".into(),
        node_template: "vm-redis-{n}".into(),
        start: 1,
        pad: 2,
        address_template: "10.0.1.{ip}:6379".into(),
        ip_start: 11,
        node_kind: NodeKind::VirtualMachine,
        container: Id::new(REDIS),
        endpoint: EndpointPlan {
            def: Id::new(REDIS_CLIENT),
            slug: "client-port".into(),
            protocol: Protocol::Tcp,
        },
    }
}

/// 用批次建立換掉 prod 原本手寫的 Redis，並把連線的 expect 設成 `expect`。
fn prod_with_batch(count: usize, expect: usize) -> loom_core::Project {
    let mut project = healthy_project();
    let prod = &mut project.environments[0];

    prod.nodes.retain(|n| !n.slug.starts_with("vm-redis-"));
    prod.nodes
        .extend(expand(&redis_batch(count), |hint| Id::new(format!("prod-{hint}"))).unwrap());

    prod.connections[1].to = Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: "redis-*".into(),
            expect: Some(expect),
        },
        endpoint: Some(Id::new(REDIS_CLIENT)),
    };

    project
}

#[test]
fn 批次生出的十二台機器可以直接通過_lint() {
    let project = prod_with_batch(12, 12);
    let findings = lint(&project);
    assert!(
        findings.is_empty(),
        "批次建立的結果卻沒通過 lint：{findings:?}"
    );
}

#[test]
fn 批次數量與expect對不上時會被抓到() {
    // 使用者把數量打成 11，但連線仍寫 expect: 12。
    let project = prod_with_batch(11, 12);

    let findings = lint(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, Rule::L004);
    assert!(
        findings[0].detail.contains("期望 12 個，實際符合 11 個"),
        "錯誤訊息不夠具體：{}",
        findings[0].detail
    );
}

#[test]
fn 批次生出的實例都有位址不會觸發_l006() {
    let project = prod_with_batch(12, 12);

    let addresses: Vec<String> = project.environments[0]
        .instances()
        .iter()
        .filter(|i| i.slug.starts_with("redis-"))
        .filter_map(|i| i.endpoints[0].address.clone())
        .collect();

    assert_eq!(addresses.len(), 12);
    assert_eq!(addresses[0], "10.0.1.11:6379");
    assert_eq!(addresses[11], "10.0.1.22:6379");
}

#[test]
fn 批次生出的名稱能被萬用字元選中() {
    let project = prod_with_batch(12, 12);
    let matched = project.environments[0].instances_matching("redis-*");
    assert_eq!(matched.len(), 12);
}

#[test]
fn 批次建立的識別碼不重複() {
    let project = prod_with_batch(12, 12);

    let mut ids: Vec<String> = project.environments[0]
        .instances()
        .iter()
        .map(|i| i.id.to_string())
        .collect();
    let 總數 = ids.len();
    ids.sort();
    ids.dedup();

    assert_eq!(ids.len(), 總數, "有重複的 Instance 識別碼");
}
