//! Lint 的整合測試。
//!
//! 這裡只透過**公開 API** 操作，跟真正的使用者站在同一邊。
//!
//! # 為什麼比對「完整清單」而不是「有沒有包含某項」
//!
//! 只檢查「有沒有出現我預期的錯誤」，會漏掉「多冒出一個非預期的錯誤」。
//! 對一個以「找出缺漏」為賣點的工具來說，誤報跟漏報一樣糟——
//! 使用者一旦習慣忽略雜訊，真正的問題也會被忽略。
//!
//! 因此每個案例都比對整份結果。

mod common;

use common::*;
use loom_core::environment::{Endpointing, InstanceRef};
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity, lint};

/// 把發現整理成 `(規則, 環境, 對象)` 的字串，方便整份比對。
fn summarize(project: &loom_core::Project) -> Vec<String> {
    lint(project)
        .iter()
        .map(|f| {
            let env = f
                .environment
                .as_ref()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "-".into());
            format!("{} {} {}", f.rule.code(), env, f.subject)
        })
        .collect()
}

#[test]
fn a_healthy_project_has_no_findings() {
    let project = healthy_project();
    let found = summarize(&project);
    assert_eq!(
        found,
        Vec::<String>::new(),
        "健康的專案卻報出問題：{found:?}"
    );
}

#[test]
fn one_missing_node_is_caught_by_expect() {
    let mut project = healthy_project();

    // prod 原本 3 台 Redis，拔掉一台，但連線仍寫 expect: 3。
    let prod = &mut project.environments[0];
    prod.nodes.retain(|n| n.slug != "vm-redis-03");

    assert_eq!(summarize(&project), vec!["L004 env-prod conn-prod-2"]);
}

#[test]
fn a_wildcard_without_expect_is_a_warning_not_an_error() {
    let mut project = healthy_project();

    let prod = &mut project.environments[0];
    if let Endpointing::Instance { target, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None;
    }

    let findings = lint(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, Rule::L005);
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn a_pattern_starting_with_a_wildcard_still_counts_correctly() {
    // lint 內部為了效能，會先用「第一個 `*` 之前的字面前綴」二分搜出候選範圍，
    // 再做完整比對。`*-01` 這種沒有前綴的樣式是那個最佳化唯一會踩空的地方，
    // 所以特別釘住：它必須退回掃全部，而且算出來的數字要跟直覺一致。
    //
    // prod 有 api-01、redis-01、redis-02、redis-03，所以 `*-01` 應該是 2 個。
    let mut project = healthy_project();

    let prod = &mut project.environments[0];
    if let Endpointing::Instance { target, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern {
            slug_pattern,
            expect,
            ..
        } = target
    {
        *slug_pattern = "*-01".into();
        *expect = Some(2);
    }

    let findings = lint(&project);
    let count_mismatch: Vec<_> = findings.iter().filter(|f| f.rule == Rule::L004).collect();
    assert!(
        count_mismatch.is_empty(),
        "`*-01` 應該剛好符合 2 個，卻報了數量錯誤：{count_mismatch:?}"
    );
}

#[test]
fn a_dead_end_after_the_f5_is_caught_by_reachability() {
    let mut project = healthy_project();

    // 只留下「API → F5」那一段，砍掉「F5 → Redis」。
    // 連線本身都還在、都合法，單看每一條都沒問題——
    // 只有把整條路徑串起來看，才知道它到不了目的地。
    let prod = &mut project.environments[0];
    prod.connections.retain(|c| c.id == Id::new("conn-prod-1"));

    let found = summarize(&project);
    assert!(
        found.contains(&"L002 env-prod r-api-快取".to_string()),
        "應該要報 L002 走不通，實際得到：{found:?}"
    );
}

#[test]
fn an_environment_that_realises_no_contract() {
    let mut project = healthy_project();

    let prod = &mut project.environments[0];
    prod.connections.clear();

    let found = summarize(&project);
    // 兩條邏輯連線都沒實現（L001），且所有服務實體都沒被碰到（L008）。
    assert_eq!(
        found,
        vec![
            "L001 env-prod r-api-快取",
            "L001 env-prod r-api-金流",
            "L008 env-prod i-prod-api-01",
            "L008 env-prod i-prod-redis-01",
            "L008 env-prod i-prod-redis-02",
            "L008 env-prod i-prod-redis-03",
            "L008 env-prod sys-prod-payment",
        ]
    );
}

#[test]
fn a_container_deployed_in_no_environment() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.nodes.retain(|n| n.slug != "vm-redis-01");
    dev.connections.clear();

    let found = summarize(&project);
    assert_eq!(
        found,
        vec![
            "L001 env-dev c-redis",
            "L001 env-dev r-api-快取",
            "L001 env-dev r-api-金流",
            "L008 env-dev i-dev-api-01",
            "L008 env-dev sys-dev-payment",
        ]
    );
}

#[test]
fn endpoint_without_an_address() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;

    assert_eq!(summarize(&project), vec!["L006 env-dev ep-dev-redis-01"]);
}

#[test]
fn a_connection_pointing_at_a_missing_instance() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.connections[0].from = Endpointing::Instance {
        target: InstanceRef::One(Id::new("i-dev-根本沒這台")),
        endpoint: None,
    };

    let found = summarize(&project);
    assert!(found.contains(&"L003 env-dev conn-dev-1".to_string()));
    // API 仍被金流那條連線碰到，所以不該報 L008——L008 看的是「有沒有人碰」，
    // 不是「該有的連線在不在」，那是 L001／L002 的職責。
    assert!(!found.contains(&"L008 env-dev i-dev-api-01".to_string()));
}

#[test]
fn a_standby_marked_standalone_stops_warning() {
    let mut project = healthy_project();

    {
        let dev = &mut project.environments[2];
        let mut standby = dev.nodes[1].instances[0].clone();
        standby.id = Id::new("i-dev-redis-備援");
        standby.slug = "redis-standby".into();
        dev.nodes[0].instances.push(standby);
    }

    // 沒標記 → 應該要叫
    assert_eq!(summarize(&project), vec!["L008 env-dev i-dev-redis-備援"]);

    // 標記「刻意獨立」→ 安靜
    project.environments[2].nodes[0].instances[1].standalone = true;
    assert_eq!(summarize(&project), Vec::<String>::new());
}

#[test]
fn a_connection_without_a_purpose_is_only_a_warning() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.connections[0].purpose = "   ".into();

    let findings = lint(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, Rule::L007);
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn one_contract_expanding_to_different_counts_is_still_healthy() {
    // 快取那條：prod 兩段（走 F5）＋ 3 台叢集；test 一段 ＋ 2 台；dev 一段 ＋ 1 台。
    // 數量不同是正常的，不該報錯——這是設計上刻意的決定。
    let project = healthy_project();

    assert_eq!(project.environments[0].connections.len(), 3);
    assert_eq!(project.environments[1].connections.len(), 2);
    assert_eq!(project.environments[2].connections.len(), 2);
    assert_eq!(summarize(&project), Vec::<String>::new());
}

#[test]
fn an_external_system_may_land_on_a_different_address_per_environment() {
    // prod 打正式閘道、test 打 sandbox、dev 打本機 mock。
    // 位址不同是正常的，重點是「每個環境都有指定」。
    let project = healthy_project();

    let addresses: Vec<&str> = project
        .environments
        .iter()
        .flat_map(|e| &e.systems)
        .flat_map(|s| &s.endpoints)
        .filter_map(|e| e.address.as_deref())
        .collect();

    assert_eq!(
        addresses,
        vec![
            "https://pay.example.com",
            "https://sandbox.pay.example.com",
            "http://localhost:9000",
        ]
    );
    assert_eq!(summarize(&project), Vec::<String>::new());
}

#[test]
fn an_external_system_with_no_landing_in_one_environment() {
    let mut project = healthy_project();

    // test 環境忘了填金流 sandbox 的位址。
    project.environments[1].systems.clear();

    let found = summarize(&project);
    assert_eq!(
        found,
        vec![
            // 外部系統本身沒有實體
            "L001 env-test s-payment",
            // 連帶那條邏輯連線也沒東西可指
            "L003 env-test conn-test-pay",
        ]
    );
}

#[test]
fn an_external_system_endpoint_without_an_address() {
    let mut project = healthy_project();

    project.environments[1].systems[0].endpoints[0].address = None;

    assert_eq!(summarize(&project), vec!["L006 env-test ep-test-payment"]);
}
