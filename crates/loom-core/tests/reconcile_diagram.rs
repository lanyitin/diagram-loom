//! 對帳的整合測試。
//!
//! 單元測試驗證三方比對的每一格；這裡驗證它跟真正的模型、跟 base 快照的
//! 存取接得起來——尤其是「解決 → 存 base → 再對帳一次應該乾淨」這個循環。

mod common;

use common::*;
use loom_core::id::Id;
use loom_core::reconcile::{
    DiagramElement, DifferenceKind, ElementKind, Resolution, SYNC_STATE_PATH, SyncState,
    model_elements, reconcile, settled_snapshot,
};
use loom_core::store::{FileStore, MemoryStore};

/// 把模型元素原封不動當成圖上的形狀——代表「圖畫得跟模型一模一樣」。
fn drawn_from_the_model(env: &loom_core::environment::Environment) -> Vec<DiagramElement> {
    model_elements(env)
        .into_iter()
        .map(|m| DiagramElement {
            id: m.id,
            label: m.label,
        })
        .collect()
}

#[test]
fn every_drawable_model_element_is_listed() {
    let project = healthy_project();
    let prod = &project.environments[0];
    let elements = model_elements(prod);

    let kinds: Vec<ElementKind> = {
        let mut k: Vec<_> = elements.iter().map(|e| e.kind).collect();
        k.sort();
        k.dedup();
        k
    };

    // prod 有：機器、服務落地、F5、外部系統落地、連線
    assert_eq!(
        kinds,
        vec![
            ElementKind::DeploymentNode,
            ElementKind::ContainerInstance,
            ElementKind::InfrastructureNode,
            ElementKind::SoftwareSystemInstance,
            ElementKind::Connection,
        ]
    );
}

#[test]
fn the_first_reconcile_sees_an_empty_diagram_so_everything_is_a_model_addition() {
    let project = healthy_project();
    let prod = &project.environments[0];

    let state = SyncState::default();
    let diffs = reconcile(
        &model_elements(prod),
        &[],
        &state.snapshot("prod/main.drawio"),
    );

    assert_eq!(diffs.len(), model_elements(prod).len());
    assert!(
        diffs
            .iter()
            .all(|d| matches!(d.kind, DifferenceKind::AddedToModel { .. })),
        "第一次對帳應該全都是「模型有、圖上沒有」"
    );
}

#[test]
fn a_diagram_matching_the_model_reconciles_clean() {
    let project = healthy_project();
    let prod = &project.environments[0];

    let model = model_elements(prod);
    let diagram = drawn_from_the_model(prod);
    let base = settled_snapshot(&model, &diagram);

    assert!(reconcile(&model, &diagram, &base).is_empty());
}

#[test]
fn a_node_deleted_on_the_diagram_is_reported_without_confusing_model_additions() {
    let project = healthy_project();
    let prod = &project.environments[0];

    let model = model_elements(prod);
    let mut diagram = drawn_from_the_model(prod);
    let base = settled_snapshot(&model, &diagram);

    // 使用者從圖上刪掉一個 Redis 落地
    let deleted = model
        .iter()
        .find(|m| m.label == "redis-01")
        .expect("假專案應該有 redis-01")
        .id
        .clone();
    diagram.retain(|d| d.id != deleted);

    let diffs = reconcile(&model, &diagram, &base);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].id, deleted);
    assert_eq!(diffs[0].kind, DifferenceKind::RemovedFromDiagram);

    // 這正是 base 存在的意義：沒有 base 的話，同樣的輸入會被判成「模型新增」。
    let no_base = reconcile(&model, &diagram, &Default::default());
    assert!(
        no_base
            .iter()
            .any(|d| d.id == deleted && matches!(d.kind, DifferenceKind::AddedToModel { .. })),
        "沒有 base 時會誤判成模型新增——這就是三方比對的理由"
    );
}

#[test]
fn a_deletion_and_a_rename_at_once_are_classified_separately() {
    let project = healthy_project();
    let prod = &project.environments[0];

    let model = model_elements(prod);
    let mut diagram = drawn_from_the_model(prod);
    let base = settled_snapshot(&model, &diagram);

    let redis01 = model
        .iter()
        .find(|m| m.label == "redis-01")
        .unwrap()
        .id
        .clone();
    let redis02 = model
        .iter()
        .find(|m| m.label == "redis-02")
        .unwrap()
        .id
        .clone();

    diagram.retain(|d| d.id != redis01);
    diagram.iter_mut().find(|d| d.id == redis02).unwrap().label = "快取節點二".into();

    let diffs = reconcile(&model, &diagram, &base);
    assert_eq!(diffs.len(), 2);

    let deleted = diffs.iter().find(|d| d.id == redis01).unwrap();
    assert_eq!(deleted.kind, DifferenceKind::RemovedFromDiagram);

    let renamed = diffs.iter().find(|d| d.id == redis02).unwrap();
    assert_eq!(
        renamed.kind,
        DifferenceKind::RenamedInDiagram {
            from: "redis-02".into(),
            to: "快取節點二".into(),
        }
    );
}

#[test]
fn saving_the_base_after_resolving_makes_the_next_reconcile_clean() {
    let project = healthy_project();
    let prod = &project.environments[0];
    let model = model_elements(prod);

    let mut store = MemoryStore::new();

    // 第一輪：圖是空的，全部都是「要加到圖上」
    let mut state = SyncState::load(&store).unwrap();
    let first_round = reconcile(&model, &[], &state.snapshot("prod/main.drawio"));
    assert!(!first_round.is_empty());
    assert!(
        first_round
            .iter()
            .all(|d| d.resolutions().contains(&Resolution::AddToDiagram))
    );

    // 使用者全選「加到圖上」，JS 照做，圖變成跟模型一樣
    let diagram = drawn_from_the_model(prod);

    // 對帳完成 → 更新 base 並存檔
    state.diagrams.insert(
        "prod/main.drawio".into(),
        settled_snapshot(&model, &diagram),
    );
    state.save(&mut store).unwrap();

    // 第二輪：重新讀檔，應該完全乾淨
    let reloaded = SyncState::load(&store).unwrap();
    let second_round = reconcile(&model, &diagram, &reloaded.snapshot("prod/main.drawio"));
    assert!(
        second_round.is_empty(),
        "解決後再對帳仍有差異：{second_round:?}"
    );
}

#[test]
fn base_snapshot_lives_at_the_agreed_path() {
    let mut store = MemoryStore::new();
    SyncState::default().save(&mut store).unwrap();
    assert_eq!(store.paths(), vec![SYNC_STATE_PATH]);
}

#[test]
fn reading_before_the_first_reconcile_is_not_an_error() {
    // 第一次使用專案時本來就沒有這個檔。
    let store = MemoryStore::new();
    let state = SyncState::load(&store).unwrap();
    assert!(state.diagrams.is_empty());
}

#[test]
fn each_diagram_has_its_own_base() {
    let project = healthy_project();
    let prod = &project.environments[0];
    let model = model_elements(prod);
    let diagram = drawn_from_the_model(prod);

    let mut state = SyncState::default();
    state.diagrams.insert(
        "prod/網路架構.drawio".into(),
        settled_snapshot(&model, &diagram),
    );

    // 另一張圖還沒對帳過，不該沿用第一張的 base
    assert!(state.snapshot("prod/資料流.drawio").elements.is_empty());
    assert!(!state.snapshot("prod/網路架構.drawio").elements.is_empty());
}

#[test]
fn a_corrupt_base_file_is_reported_not_treated_as_empty() {
    // 空的 base 代表「還沒對帳過」，會讓所有東西看起來像新增的。
    // 檔案壞掉卻被當成空的，使用者會以為圖被清空了。
    let mut store = MemoryStore::new();
    store.write(SYNC_STATE_PATH, "這不是 YAML: [[[").unwrap();

    assert!(SyncState::load(&store).is_err());
}

#[test]
fn an_unknown_shape_offers_both_create_and_remove() {
    let project = healthy_project();
    let prod = &project.environments[0];
    let model = model_elements(prod);

    let mut diagram = drawn_from_the_model(prod);
    let base = settled_snapshot(&model, &diagram);
    diagram.push(DiagramElement {
        id: Id::new("來路不明"),
        label: "誰畫的".into(),
    });

    let diffs = reconcile(&model, &diagram, &base);
    assert_eq!(diffs.len(), 1);
    assert_eq!(diffs[0].kind, DifferenceKind::AddedToDiagram);
    assert_eq!(
        diffs[0].resolutions(),
        vec![Resolution::CreateInModel, Resolution::RemoveFromDiagram]
    );
}
