//! 覆蓋矩陣的整合測試。
//!
//! 矩陣是主畫面，它會決定使用者第一眼看到什麼。所以這裡最在意的不是
//! 「數字算得對不對」，而是**它有沒有跟 lint 面板講一樣的話**——
//! 同一個環境，矩陣說沒事、面板說有錯，使用者就不會再相信任何一邊。

mod common;

use common::*;
use loom_core::coverage::{Status, coverage};
use loom_core::environment::{Endpointing, InstanceRef};
use loom_core::id::Id;
use loom_core::lint::{Severity, lint};

#[test]
fn a_healthy_project_has_an_all_green_matrix() {
    let project = healthy_project();
    let m = coverage(&project);

    assert_eq!(m.relationships.len(), 2);
    assert_eq!(m.environments.len(), 3);
    assert_eq!(m.cells.len(), 6, "矩陣應該是 2 條契約 × 3 個環境");

    let not_green: Vec<_> = m
        .cells
        .iter()
        .filter(|c| c.status != Status::Realized)
        .collect();
    assert!(
        not_green.is_empty(),
        "健康的專案卻有格子不是綠的：{not_green:?}"
    );
}

#[test]
fn one_contract_has_different_hops_and_counts_per_environment() {
    let project = healthy_project();
    let m = coverage(&project);

    // prod 走 F5 是兩段、3 台 Redis；dev 直連是一段、1 台。
    // 「幾段 · 幾台」正是格子裡要顯示的東西——只顯示打勾就看不出這個差別。
    let prod = m.cell(&Id::new(REL_CACHE), &Id::new("env-prod")).unwrap();
    assert_eq!((prod.segments, prod.targets), (2, 3));

    let dev = m.cell(&Id::new(REL_CACHE), &Id::new("env-dev")).unwrap();
    assert_eq!((dev.segments, dev.targets), (1, 1));
}

#[test]
fn through_an_f5_the_count_comes_from_the_last_hop() {
    // 第一段的目標是 F5 設備，數量沒有意義；真正抵達的是最後一段。
    let project = healthy_project();
    let m = coverage(&project);

    let prod = m.cell(&Id::new(REL_CACHE), &Id::new("env-prod")).unwrap();
    assert_eq!(prod.targets, 3, "應該顯示 3 台 Redis，而不是 1 台 F5");
    assert_eq!(prod.expect, Some(3));
}

#[test]
fn an_unrealised_contract_is_missing_not_broken() {
    // 這是使用者最怕的那一格，必須跟「有畫但畫錯」分得開。
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let m = coverage(&project);
    let dev = m.cell(&Id::new(REL_CACHE), &Id::new("env-dev")).unwrap();

    assert_eq!(dev.status, Status::Missing);
    assert_eq!(dev.segments, 0);
    assert_eq!(dev.targets, 0);

    let missing: Vec<_> = m.missing().collect();
    assert_eq!(missing.len(), 1, "只有這一格該是缺的：{missing:?}");
}

#[test]
fn one_missing_node_turns_the_cell_broken() {
    let mut project = healthy_project();
    project.environments[0]
        .nodes
        .retain(|n| n.slug != "vm-redis-03");

    let m = coverage(&project);
    let prod = m.cell(&Id::new(REL_CACHE), &Id::new("env-prod")).unwrap();

    assert_eq!(prod.status, Status::Broken);
    // 期望與實際並排——萬用字元最危險的地方就是「少一台看起來完全正常」。
    assert_eq!((prod.targets, prod.expect), (2, Some(3)));
}

#[test]
fn a_warning_is_not_treated_as_an_error() {
    let mut project = healthy_project();
    let prod = &mut project.environments[0];
    if let Endpointing::Instance { target, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None; // L005，Warning
    }

    let m = coverage(&project);
    let cell = m.cell(&Id::new(REL_CACHE), &Id::new("env-prod")).unwrap();
    assert_eq!(cell.status, Status::Warning);
}

#[test]
fn the_matrix_never_contradicts_the_lint_panel() {
    // 這是整份測試最重要的一條。
    //
    // 矩陣與 lint 面板是同一份資料的兩種排法。如果它們對「這個環境有沒有問題」
    // 給出不同答案，使用者就沒有任何一邊可以信。
    //
    // 刻意弄壞好幾個不同的地方，讓兩邊都有話要說。
    let mut project = healthy_project();
    project.environments[0]
        .nodes
        .retain(|n| n.slug != "vm-redis-03"); // prod：L004
    project.environments[1].connections.clear(); // test：整個環境都空了
    project.environments[2].connections[0].purpose.clear(); // dev：L007

    let m = coverage(&project);
    let findings = lint(&project);

    for cell in &m.cells {
        // lint 認為這條契約在這個環境有沒有錯？
        let cell_connections: Vec<&Id> = project
            .environments
            .iter()
            .find(|e| e.id == cell.environment)
            .unwrap()
            .connections
            .iter()
            .filter(|c| c.serves == cell.relationship)
            .map(|c| &c.id)
            .collect();

        let lint_has_something_to_say = findings.iter().any(|f| {
            f.environment.as_ref() == Some(&cell.environment)
                && (f.subject == cell.relationship || cell_connections.contains(&&f.subject))
        });
        let lint_errors = findings.iter().any(|f| {
            f.environment.as_ref() == Some(&cell.environment)
                && f.severity() == Severity::Error
                && (f.subject == cell.relationship || cell_connections.contains(&&f.subject))
        });

        match cell.status {
            Status::Realized => assert!(
                !lint_has_something_to_say,
                "矩陣說 {:?} / {:?} 沒事，lint 卻有話說",
                cell.relationship, cell.environment
            ),
            Status::Warning => assert!(
                lint_has_something_to_say && !lint_errors,
                "矩陣說 {:?} / {:?} 只是警告，但 lint 說的不是這樣",
                cell.relationship,
                cell.environment
            ),
            Status::Broken | Status::Missing => assert!(
                lint_errors,
                "矩陣說 {:?} / {:?} 有錯，lint 卻沒報錯",
                cell.relationship, cell.environment
            ),
        }
    }
}

#[test]
fn findings_outside_any_contract_stay_out_of_the_matrix() {
    // 「某台機器忘了填位址」是 L006，它屬於那台機器，不屬於任何一條契約。
    // 這種東西只該出現在 lint 面板；混進矩陣會讓格子紅得莫名其妙。
    let mut project = healthy_project();
    let cold = instance("dev", "redis-cold", REDIS, REDIS_CLIENT, "10.2.0.9:6379");
    let mut cold = cold;
    cold.endpoints[0].address = None; // L006
    cold.standalone = true; // 避免順便觸發 L008
    project.environments[2].nodes.extend(vms("dev", vec![cold]));

    let findings = lint(&project);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == loom_core::lint::Rule::L006),
        "前提沒成立：應該要有 L006"
    );

    let m = coverage(&project);
    let dev_cell: Vec<_> = m
        .cells
        .iter()
        .filter(|c| c.environment == Id::new("env-dev"))
        .collect();
    assert!(
        dev_cell.iter().all(|c| c.status == Status::Realized),
        "機器層級的問題不該讓契約的格子變色：{dev_cell:?}"
    );
}

#[test]
fn cells_are_ordered_row_first_then_column() {
    // 前端直接照順序畫，不必再排一次。
    let project = healthy_project();
    let m = coverage(&project);

    let order: Vec<(usize, usize)> = m
        .cells
        .iter()
        .map(|c| {
            (
                m.relationships
                    .iter()
                    .position(|r| r == &c.relationship)
                    .unwrap(),
                m.environments
                    .iter()
                    .position(|e| e == &c.environment)
                    .unwrap(),
            )
        })
        .collect();

    let mut expected_order = order.clone();
    expected_order.sort();
    assert_eq!(order, expected_order);
}
