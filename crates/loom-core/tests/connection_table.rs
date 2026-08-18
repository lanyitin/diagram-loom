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

fn search(project: &loom_core::Project, env: &str, conn: &str) -> loom_core::table::Row {
    rows(project)
        .into_iter()
        .find(|r| r.environment == Id::new(env) && r.id == Id::new(conn))
        .unwrap_or_else(|| panic!("找不到 {env}/{conn}"))
}

#[test]
fn every_connection_in_every_environment_gets_a_row() {
    let project = healthy_project();
    let all = rows(&project);
    let expected: usize = project
        .environments
        .iter()
        .map(|e| e.connections.len())
        .sum();
    assert_eq!(all.len(), expected);
}

#[test]
fn a_wildcard_expands_to_real_counts_and_addresses() {
    let project = healthy_project();
    // prod 第二段：F5 → redis-*，共 3 台。
    let row = search(&project, "env-prod", "conn-prod-2");

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
fn the_infra_side_points_at_a_concrete_endpoint_not_a_definition() {
    // 這個不對稱是刻意的：設備不對應任何邏輯層元素，沒有 EndpointDef 可指。
    // 解析時如果把兩者搞混，位址就會查不到而靜靜空著。
    let project = healthy_project();
    let row = search(&project, "env-prod", "conn-prod-1");

    assert_eq!(row.to.kind, SideKind::Infra);
    assert_eq!(row.to.label, "f5-vip");
    assert_eq!(row.to.endpoint.as_deref(), Some("vip-redis"));
    assert_eq!(row.to.addresses, vec!["10.0.0.100:6379"]);
}

#[test]
fn a_source_side_without_an_endpoint_is_legal() {
    // 由作業系統分配 ephemeral port，所以來源端可以留空。
    let project = healthy_project();
    let row = search(&project, "env-prod", "conn-prod-1");

    assert_eq!(row.from.label, "api-01");
    assert_eq!(row.from.endpoint, None);
    assert!(row.from.addresses.is_empty());
}

#[test]
fn the_external_system_side_resolves_to_its_address() {
    let project = healthy_project();
    let row = search(&project, "env-test", "conn-test-pay");

    assert_eq!(row.to.kind, SideKind::System);
    assert_eq!(row.to.label, "payment-sandbox");
    assert_eq!(row.to.addresses.len(), 1);
}

#[test]
fn a_clean_row_carries_no_severity() {
    let project = healthy_project();
    assert!(rows(&project).iter().all(|r| r.severity.is_none()));
}

#[test]
fn a_row_with_problems_carries_its_rules_and_severity() {
    let mut project = healthy_project();
    project.environments[0].connections[1].purpose.clear(); // L007，Warning

    let row = search(&project, "env-prod", "conn-prod-2");
    assert_eq!(row.severity, Some(Severity::Warning));
    assert!(row.rules.contains(&Rule::L007));
}

#[test]
fn a_missing_instance_shows_its_id_rather_than_a_blank() {
    // 留白的話使用者只會看到一格空的，不知道是哪個 id 壞了。
    let mut project = healthy_project();
    project.environments[2].connections[0].from = from_instance("i-dev-不存在");

    let row = search(&project, "env-dev", "conn-dev-1");
    assert_eq!(row.from.label, "i-dev-不存在");
    assert_eq!(row.from.matched, 0);
    assert_eq!(row.severity, Some(Severity::Error));
}

#[test]
fn a_missing_contract_leaves_serves_slug_empty() {
    let mut project = healthy_project();
    project.environments[2].connections[0].serves = Id::new("r-不存在");

    let row = search(&project, "env-dev", "conn-dev-1");
    assert_eq!(row.serves_slug, None);
    assert!(row.rules.contains(&Rule::L003));
}

#[test]
fn sorted_by_environment_then_by_contract() {
    // 前端直接照順序畫，不再排一次。
    let project = healthy_project();
    let all = rows(&project);

    let environment_order: Vec<usize> = all
        .iter()
        .map(|r| {
            project
                .environments
                .iter()
                .position(|e| e.id == r.environment)
                .unwrap()
        })
        .collect();
    let mut sorted = environment_order.clone();
    sorted.sort();
    assert_eq!(environment_order, sorted);
}

/// Lint 面板上點一項發現，要能跳到**那一列**。
///
/// 之前只跳到「那個環境的有問題的列」，所以同一個環境裡連點兩項，
/// 畫面完全沒變——看起來就像壞掉了。
///
/// `Row::subjects` 是那個跳轉的依據，所以它必須真的接得住每一種 subject：
/// L006 給的是 Endpoint id、L008 給的是 Instance id、L001／L002 給的是契約 id。
mod clicking_a_finding_lands_on_its_row {
    use super::*;
    use loom_core::environment::{Endpointing, InstanceRef};
    use loom_core::id::Id;
    use loom_core::lint::lint;

    /// 這個 id 在這個環境裡對應到幾列。
    fn rows_matching(project: &loom_core::Project, env: &Id, subject: &Id) -> usize {
        rows(project)
            .iter()
            .filter(|r| &r.environment == env && r.subjects.contains(subject))
            .count()
    }

    #[test]
    fn a_connection_and_its_contract_are_both_findable() {
        let project = healthy_project();
        let prod = Id::new("env-prod");

        assert_eq!(rows_matching(&project, &prod, &Id::new("conn-prod-1")), 1);
        // 快取那條契約在 prod 走兩段，所以會對到兩列——這正確，
        // 使用者要看的就是整條路徑。
        assert_eq!(rows_matching(&project, &prod, &Id::new(REL_CACHE)), 2);
    }

    #[test]
    fn l006_endpoint_maps_to_the_rows_that_use_it() {
        let mut project = healthy_project();
        project.environments[2].nodes[1].instances[0].endpoints[0].address = None;

        let f = lint(&project).into_iter().next().unwrap();
        assert_eq!(f.rule, Rule::L006);
        assert!(
            rows_matching(&project, f.environment.as_ref().unwrap(), &f.subject) > 0,
            "L006 指的是 Endpoint，卻對不到任何一列"
        );
    }

    #[test]
    fn l008_instance_maps_to_the_rows_that_touch_it() {
        // 反過來測：孤兒的定義就是沒人碰，所以它**不該**對到任何列，
        // 而同一個環境裡有人碰的那台一定對得到。
        let mut project = healthy_project();
        let dev = &mut project.environments[2];
        let touched = dev.nodes[1].instances[0].id.clone();
        let mut orphan = dev.nodes[1].instances[0].clone();
        orphan.id = Id::new("i-dev-孤兒");
        orphan.slug = "redis-standby".into();
        dev.nodes[0].instances.push(orphan);

        let dev_id = Id::new("env-dev");
        assert!(rows_matching(&project, &dev_id, &touched) > 0);
        assert_eq!(rows_matching(&project, &dev_id, &Id::new("i-dev-孤兒")), 0);
    }

    #[test]
    fn every_instance_behind_a_wildcard_is_reachable_from_the_finding() {
        // prod 的 redis-* 是三台。點其中任何一台的問題，都要跳得到那一列。
        let project = healthy_project();
        let prod = Id::new("env-prod");

        for slug in ["i-prod-redis-01", "i-prod-redis-02", "i-prod-redis-03"] {
            assert!(
                rows_matching(&project, &prod, &Id::new(slug)) > 0,
                "{slug} 對不到任何一列"
            );
        }
    }

    #[test]
    fn the_broken_id_is_collected_too() {
        // L003 的整個重點就是「這個 id 是壞的」。若不收，
        // 使用者點那項發現會跳到一片空白，比不能點更糟。
        let mut project = healthy_project();
        project.environments[2].connections[0].from = Endpointing::Instance {
            target: InstanceRef::One(Id::new("i-dev-根本沒這台")),
            endpoint: None,
        };

        assert_eq!(
            rows_matching(&project, &Id::new("env-dev"), &Id::new("i-dev-根本沒這台")),
            1
        );
    }

    #[test]
    fn every_environment_finding_lands_on_at_least_one_row() {
        // 這是整組的重點。邏輯層的發現（environment 是 None）本來就沒有列，
        // 那要由畫面另外處理；除此之外，會叫的每一項都必須跳得到地方。
        let mut project = healthy_project();
        {
            let dev = &mut project.environments[2];
            dev.nodes[1].instances[0].endpoints[0].address = None;
            dev.connections[0].purpose = String::new();
        }
        if let Endpointing::Instance { target, .. } = &mut project.environments[0].connections[1].to
            && let InstanceRef::Pattern { expect, .. } = target
        {
            *expect = Some(99);
        }

        let findings = lint(&project);
        assert!(findings.len() >= 3);

        for f in &findings {
            let Some(env) = &f.environment else { continue };
            assert!(
                rows_matching(&project, env, &f.subject) > 0,
                "{f:?} 點下去會跳到一片空白"
            );
        }
    }
}

#[test]
fn the_memo_comes_through_on_the_row() {
    // 備註是這個模型裡唯一沒有規則會回報的欄位——寫錯了 lint 不會叫。
    // 所以它的回報路徑只有這張表。讀不回來的話，寫的人會以為沒寫成功，
    // 然後再寫一次。
    let mut project = healthy_project();
    project.environments[0].connections[0].memo = "等年底汰換".into();

    let row = search(
        &project,
        "env-prod",
        &project.environments[0].connections[0].id.to_string(),
    );
    assert_eq!(row.memo, "等年底汰換");
}
