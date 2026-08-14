//! L014：同一個環境裡兩個不同種類的東西叫同一個名字。
//!
//! # 為什麼會發生
//!
//! 新增時的唯一性檢查是**分開的**：服務實體只跟服務實體比、設備只跟設備比、
//! 外部系統實體只跟外部系統實體比。所以一台叫 `pay-01` 的服務實體與一台
//! 叫 `pay-01` 的設備可以同時存在——兩邊的檢查都會過。
//!
//! # 為什麼會咬人
//!
//! Agent 是**用名字**指涉東西的（`loom-mcp` 的 `refs`），而它依序找
//! 服務實體 → 設備 → 外部系統實體，先找到的贏。於是「連到 pay-01」
//! 會安靜地接到服務實體上，而使用者要的是那台設備——**連線看起來完全正常**。
//!
//! `refs` 的模組註解一直寫著「名字撞在一起本來就是該被 lint 抓的問題，
//! 這裡不替它遮掩」。在這條規則之前，那個 lint 並不存在。

mod common;

use common::*;
use loom_core::environment::InfrastructureNode;
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity, lint};

fn collisions(project: &loom_core::Project) -> Vec<loom_core::lint::Finding> {
    lint(project)
        .into_iter()
        .filter(|f| f.rule == Rule::L014)
        .collect()
}

/// 加一台設備，名字自己指定。
fn with_infra_named(slug: &str) -> loom_core::Project {
    let mut project = healthy_project();
    project.environments[0].infra.push(InfrastructureNode {
        id: Id::new(format!("infra-{slug}")),
        slug: slug.into(),
        endpoints: vec![],
    });
    project
}

#[test]
fn a_healthy_project_says_nothing() {
    assert_eq!(collisions(&healthy_project()), Vec::new());
}

#[test]
fn a_device_named_after_a_service_instance_is_flagged() {
    let project = healthy_project();
    let taken = project.environments[0].instances()[0].slug.clone();

    let project = with_infra_named(&taken);
    let complaints = collisions(&project);

    assert_eq!(complaints.len(), 1, "{complaints:?}");
    assert!(complaints[0].detail.contains(&taken), "{:?}", complaints[0]);
}

#[test]
fn it_is_reported_on_the_one_that_loses() {
    // 贏的那個用名字還指得到，輸的那個指不到——需要改名的是它。
    // 報在贏的那個身上的話，使用者會去改一個其實還能用的東西。
    let project = healthy_project();
    let winner = project.environments[0].instances()[0].clone();

    let project = with_infra_named(&winner.slug);
    let complaints = collisions(&project);

    assert_ne!(complaints[0].subject, winner.id, "報錯對象了");
    assert_eq!(
        complaints[0].subject,
        Id::new(format!("infra-{}", winner.slug))
    );
}

#[test]
fn the_message_says_which_one_wins() {
    // 只說「撞名了」的話，使用者不知道現在到底指到誰。
    let project = healthy_project();
    let taken = project.environments[0].instances()[0].slug.clone();
    let project = with_infra_named(&taken);

    let detail = &collisions(&project)[0].detail;
    assert!(detail.contains("服務實體"), "{detail}");
    assert!(detail.contains("設備"), "{detail}");
    assert!(detail.contains("改一個名字"), "{detail}");
}

#[test]
fn it_is_a_warning_not_an_error() {
    // 資料本身沒有壞，壞的是「用名字指涉」這一條路。而現實中 VIP 跟它
    // 服務的那個東西同名是有可能的，用錯誤會變成一個拿不掉的紅字。
    let project = healthy_project();
    let taken = project.environments[0].instances()[0].slug.clone();
    let project = with_infra_named(&taken);

    assert_eq!(collisions(&project)[0].severity(), Severity::Warning);
}

#[test]
fn the_same_name_in_a_different_environment_is_fine() {
    // 環境之間本來就會重複——prod 與 test 都有 redis-01 是正常的。
    // 所以要挑一個**只在 prod 有**的名字，加到 test 去。
    let mut project = healthy_project();
    let in_test: Vec<String> = project.environments[1]
        .instances()
        .iter()
        .map(|i| i.slug.clone())
        .collect();
    let only_in_prod = project.environments[0]
        .instances()
        .iter()
        .map(|i| i.slug.clone())
        .find(|s| !in_test.contains(s))
        .expect("素材裡 prod 該有一個 test 沒有的服務實體");

    project.environments[1].infra.push(InfrastructureNode {
        id: Id::new("infra-別的環境"),
        slug: only_in_prod,
        endpoints: vec![],
    });

    assert_eq!(collisions(&project), Vec::new());
}
