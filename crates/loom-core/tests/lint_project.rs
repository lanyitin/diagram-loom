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
fn 健康的專案不該有任何發現() {
    let project = healthy_project();
    let found = summarize(&project);
    assert_eq!(
        found,
        Vec::<String>::new(),
        "健康的專案卻報出問題：{found:?}"
    );
}

#[test]
fn 少建一台機器會被萬用字元的_expect_抓到() {
    let mut project = healthy_project();

    // prod 原本 3 台 Redis，拔掉一台，但連線仍寫 expect: 3。
    let prod = &mut project.environments[0];
    prod.nodes.retain(|n| n.slug != "vm-redis-03");

    assert_eq!(summarize(&project), vec!["L004 env-prod conn-prod-2"]);
}

#[test]
fn 萬用字元沒填_expect_只是警告不是錯誤() {
    let mut project = healthy_project();

    let prod = &mut project.environments[0];
    if let Endpointing::Instance { instance, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern { expect, .. } = instance
    {
        *expect = None;
    }

    let findings = lint(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, Rule::L005);
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn f5後面忘了接會被可達性檢查抓到() {
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
fn 整個環境忘了實現邏輯連線() {
    let mut project = healthy_project();

    let prod = &mut project.environments[0];
    prod.connections.clear();

    let found = summarize(&project);
    // 邏輯連線沒實現（L001），且三台 Redis 與一台 API 都沒被碰到（L008）。
    assert_eq!(
        found,
        vec![
            "L001 env-prod r-api-快取",
            "L008 env-prod i-prod-api-01",
            "L008 env-prod i-prod-redis-01",
            "L008 env-prod i-prod-redis-02",
            "L008 env-prod i-prod-redis-03",
        ]
    );
}

#[test]
fn 服務在某個環境完全沒部署() {
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
            "L008 env-dev i-dev-api-01",
        ]
    );
}

#[test]
fn endpoint忘了填位址() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;

    assert_eq!(summarize(&project), vec!["L006 env-dev ep-dev-redis-01"]);
}

#[test]
fn 連線指向不存在的機器() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.connections[0].from = Endpointing::Instance {
        instance: InstanceRef::One(Id::new("i-dev-根本沒這台")),
        endpoint: None,
    };

    let found = summarize(&project);
    assert!(found.contains(&"L003 env-dev conn-dev-1".to_string()));
    // 順帶：原本的 API 現在沒被任何連線碰到了。
    assert!(found.contains(&"L008 env-dev i-dev-api-01".to_string()));
}

#[test]
fn 冷備機標記刻意獨立後就不再警告() {
    let mut project = healthy_project();

    {
        let dev = &mut project.environments[2];
        let mut 冷備 = dev.nodes[1].instances[0].clone();
        冷備.id = Id::new("i-dev-redis-備援");
        冷備.slug = "redis-standby".into();
        dev.nodes[0].instances.push(冷備);
    }

    // 沒標記 → 應該要叫
    assert_eq!(summarize(&project), vec!["L008 env-dev i-dev-redis-備援"]);

    // 標記「刻意獨立」→ 安靜
    project.environments[2].nodes[0].instances[1].standalone = true;
    assert_eq!(summarize(&project), Vec::<String>::new());
}

#[test]
fn 連線沒填用途只是警告() {
    let mut project = healthy_project();

    let dev = &mut project.environments[2];
    dev.connections[0].purpose = "   ".into();

    let findings = lint(&project);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, Rule::L007);
    assert_eq!(findings[0].severity(), Severity::Warning);
}

#[test]
fn 同一條邏輯連線在三個環境展開成不同數量都算健康() {
    // prod 兩段（走 F5）＋ 3 台叢集；test 一段 ＋ 2 台；dev 一段 ＋ 1 台。
    // 數量不同是正常的，不該報錯——這是設計上刻意的決定。
    let project = healthy_project();

    assert_eq!(project.environments[0].connections.len(), 2);
    assert_eq!(project.environments[1].connections.len(), 1);
    assert_eq!(project.environments[2].connections.len(), 1);
    assert_eq!(summarize(&project), Vec::<String>::new());
}
