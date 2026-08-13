//! [`Edit::Batch`]：一整批修改算一步。
//!
//! # 這裡真正在守的東西
//!
//! 批次存在的理由是「Agent 一次建幾百個」，而那個情境有兩個致命的失敗方式：
//!
//! 1. **做到一半失敗。** 模型變成殘的，而殘在哪裡沒有人知道——
//!    這比整批失敗糟得多，因為它看起來像成功了。
//! 2. **復原要按幾百次。** 而「Agent 建了一批、人看過覺得不對」
//!    正是最需要一鍵退回的時候。
//!
//! 所以這份測試幾乎都在驗這兩件事，而不是驗「批次會不會生效」。

mod common;

use common::*;
use loom_core::edit::{self, Edit, EditError};
use loom_core::history::History;
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};

const PROD: &str = "env-prod";

fn set_purpose(connection: &str, purpose: &str) -> Edit {
    Edit::SetPurpose {
        environment: Some(Id::new(PROD)),
        subject: Id::new(connection),
        purpose: purpose.into(),
    }
}

/// prod 環境裡那些連線的 id。
fn prod_connections(project: &loom_core::Project) -> Vec<String> {
    project.environments[0]
        .connections
        .iter()
        .map(|c| c.id.to_string())
        .collect()
}

#[test]
fn a_whole_batch_undoes_in_one_press() {
    // 這是整個 Batch 存在的理由。少了它，Agent 建三百個東西，
    // 使用者想全部退掉要按三百次——他不會按，他會關掉不存檔，
    // 而那樣連對的那部分也一起沒了。
    let project = healthy_project();
    let ids = prod_connections(&project);
    let mut history = History::opened(project);

    let batch = Edit::Batch(ids.iter().map(|id| set_purpose(id, "改過了")).collect());
    history.edit(&batch).unwrap();

    assert!(
        history.project().environments[0]
            .connections
            .iter()
            .all(|c| c.purpose == "改過了"),
        "批次沒有全部生效"
    );

    assert!(history.undo());
    assert!(
        history.project().environments[0]
            .connections
            .iter()
            .all(|c| c.purpose != "改過了"),
        "按一次復原只退了一部分"
    );
    assert!(!history.undo(), "一批應該只留一步，卻留了不只一步");
}

#[test]
fn a_batch_that_fails_partway_changes_nothing_at_all() {
    // 「建到第 37 個才失敗」是最糟的結果：看起來成功了，其實模型是殘的。
    let mut project = healthy_project();
    let before = project.clone();
    let good = prod_connections(&project)[0].clone();

    let err = edit::apply(
        &mut project,
        &Edit::Batch(vec![
            set_purpose(&good, "這一項是好的"),
            set_purpose("根本沒這條連線", "這一項會炸"),
        ]),
    )
    .unwrap_err();

    assert_eq!(project, before, "失敗的批次卻留下了第一項的修改");
    assert!(
        matches!(
            err,
            EditError::InBatch {
                at: 1,
                total: 2,
                ..
            }
        ),
        "沒說是第幾項失敗的：{err}"
    );
}

#[test]
fn the_error_says_which_one_broke_because_the_agent_has_to_find_it() {
    // Agent 一次送幾百項進來。只說「找不到」它無從下手，
    // 說「第 2 項找不到」它才知道要改哪一個再送一次。
    let mut project = healthy_project();
    let err = edit::apply(
        &mut project,
        &Edit::Batch(vec![set_purpose("壞的", "x"), set_purpose("也是壞的", "y")]),
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("第 1 項"), "{err}");
    assert!(err.contains("整批都沒有套用"), "{err}");
}

#[test]
fn a_failed_batch_leaves_no_undo_step() {
    // 失敗卻留下一步的話，使用者按復原會退掉他上一次真的做過的事。
    let mut history = History::opened(healthy_project());
    assert!(
        history
            .edit(&Edit::Batch(vec![set_purpose("壞的", "x")]))
            .is_err()
    );
    assert_eq!(history.undo_label(), None);
    assert!(!history.is_dirty());
}

#[test]
fn an_empty_batch_is_an_error_not_a_silent_no_op() {
    // 放過去的話會留下一步「什麼都沒做」的復原紀錄，使用者按了沒反應。
    let mut project = healthy_project();
    assert_eq!(
        edit::apply(&mut project, &Edit::Batch(vec![])),
        Err(EditError::EmptyBatch)
    );
}

#[test]
fn the_undo_label_names_the_action_when_the_whole_batch_does_one_thing() {
    // 「復原：修改用途」比「復原：批次修改」有用得多，
    // 而一批通常真的就是同一件事重複幾百次。
    let project = healthy_project();
    let ids = prod_connections(&project);
    let mut history = History::opened(project);

    history
        .edit(&Edit::Batch(
            ids.iter().map(|id| set_purpose(id, "一樣的事")).collect(),
        ))
        .unwrap();
    assert_eq!(history.undo_label(), Some("修改用途"));
}

#[test]
fn a_mixed_batch_falls_back_to_a_generic_label() {
    let project = healthy_project();
    let first = prod_connections(&project)[0].clone();
    let mut history = History::opened(project);

    history
        .edit(&Edit::Batch(vec![
            set_purpose(&first, "改用途"),
            Edit::DeleteConnection {
                environment: Id::new(PROD),
                connection: Id::new(&first),
            },
        ]))
        .unwrap();
    assert_eq!(history.undo_label(), Some("批次修改"));
}

#[test]
fn a_batch_is_judged_by_lint_only_at_the_end() {
    // 中間狀態不合法沒關係——這正是批次的價值。單獨送「刪掉這條連線」
    // 會留下一個 L002 的缺口，但同一批裡緊接著補一條回去，
    // 收工時 lint 應該跟開工時一樣乾淨。
    let mut project = healthy_project();
    assert!(lint(&project).is_empty(), "素材本身就不乾淨");

    let doomed = project.environments[0].connections[0].clone();
    edit::apply(
        &mut project,
        &Edit::Batch(vec![
            Edit::DeleteConnection {
                environment: Id::new(PROD),
                connection: doomed.id.clone(),
            },
            Edit::AddConnection {
                environment: Id::new(PROD),
                id: Id::new("conn-重新接上"),
                serves: doomed.serves.clone(),
                purpose: doomed.purpose.clone(),
                kind: doomed.kind,
                from: doomed.from.clone(),
                to: doomed.to.clone(),
            },
        ]),
    )
    .unwrap();

    assert_eq!(
        lint(&project).iter().map(|f| f.rule).collect::<Vec<_>>(),
        Vec::<Rule>::new()
    );
}
