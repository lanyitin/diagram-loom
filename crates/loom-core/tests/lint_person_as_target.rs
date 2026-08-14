//! L013：契約的目標端不能是人。
//!
//! # 這條規則補的是一個完全安靜的洞
//!
//! `RelationshipEnd::Person` 的註解一直寫著「**只該出現在來源端**」——
//! 「使用者連上系統」是 C4 Context 圖最常見的關係，但反過來沒有意義：
//! 人沒有接點、沒有位址、也不會被部署。
//!
//! 而那句話在這條規則之前**只是一句註解**。把人放到目標端，三道網全部漏掉：
//!
//! | 檢查 | 為什麼放過 |
//! | --- | --- |
//! | L012「契約端指向不存在的東西」 | 那個人確實存在 |
//! | L012「目標身上沒有那個接點」 | `target_has_endpoint` 對人直接回 `true` |
//! | L002 可達性 | BFS 把人當葉節點，走到就停，不算斷裂 |
//!
//! 於是模型裡留下一條**永遠不可能被實現**的契約，而工具一句話都不說。
//! 對一個賣點是「怕漏」的工具，「說沒問題但東西是錯的」是最糟的狀態——
//! 比整個失敗還糟，因為使用者不會再回頭看。

mod common;

use common::*;
use loom_core::environment::ConnectionEnd;
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity, lint};
use loom_core::logical::{Person, RelationshipEnd};

/// 一個有人的專案。`healthy_project` 的 `people` 是空的。
fn with_a_person() -> loom_core::Project {
    let mut project = healthy_project();
    project.logical.people.push(Person {
        id: Id::new("p-客戶"),
        slug: "customer".into(),
        name: "一般客戶".into(),
    });
    project
}

fn person_problems(project: &loom_core::Project) -> Vec<loom_core::lint::Finding> {
    lint(project)
        .into_iter()
        .filter(|f| f.rule == Rule::L013)
        .collect()
}

#[test]
fn a_person_on_the_source_end_is_fine() {
    // 這正是它存在的用途：「使用者連上系統」。不能因為加了規則就把它擋掉。
    let mut project = with_a_person();
    project.logical.relationships[0].from = RelationshipEnd::Person(Id::new("p-客戶"));

    assert_eq!(person_problems(&project), Vec::new());
}

#[test]
fn a_person_on_the_target_end_is_flagged() {
    let mut project = with_a_person();
    project.logical.relationships[0].to = RelationshipEnd::Person(Id::new("p-客戶"));

    let complaints = person_problems(&project);
    assert_eq!(complaints.len(), 1);
    assert_eq!(complaints[0].end, Some(ConnectionEnd::To));
    assert_eq!(complaints[0].subject, project.logical.relationships[0].id);
}

#[test]
fn it_is_an_error_not_a_warning() {
    // 這條契約永遠不可能被實現。警告會被當成「之後再說」，
    // 而它會在每個環境都變成一項不可能解決的 L001。
    let mut project = with_a_person();
    project.logical.relationships[0].to = RelationshipEnd::Person(Id::new("p-客戶"));

    assert_eq!(person_problems(&project)[0].severity(), Severity::Error);
}

#[test]
fn the_message_names_the_person_and_both_ways_out() {
    // 沒有「照著修」的按鈕，因為**有兩個都合理的答案**：把兩端對調，
    // 或改成連到對方的服務。工具挑一個等於替使用者決定他的架構。
    // 所以訊息必須把兩條路都講出來，否則他只知道「壞了」。
    let mut project = with_a_person();
    project.logical.relationships[0].to = RelationshipEnd::Person(Id::new("p-客戶"));

    let detail = &person_problems(&project)[0].detail;
    assert!(detail.contains("customer"), "要指名是哪一個人：{detail}");
    assert!(detail.contains("對調"), "{detail}");
    assert!(detail.contains("服務"), "{detail}");
}

#[test]
fn a_healthy_project_says_nothing() {
    assert_eq!(person_problems(&healthy_project()), Vec::new());
}

#[test]
fn a_person_that_does_not_exist_on_the_target_end_gets_both_complaints() {
    // 兩個問題是獨立的：他不存在（L012），而且他不該在那一端（L013）。
    // 只報一個的話，使用者修好其中一個之後才發現還有另一個。
    let mut project = with_a_person();
    project.logical.relationships[0].to = RelationshipEnd::Person(Id::new("p-沒這個人"));

    let rules: Vec<Rule> = lint(&project)
        .into_iter()
        .filter(|f| {
            f.subject == project.logical.relationships[0].id && f.end == Some(ConnectionEnd::To)
        })
        .map(|f| f.rule)
        .collect();

    assert!(rules.contains(&Rule::L012), "{rules:?}");
    assert!(rules.contains(&Rule::L013), "{rules:?}");
}

/// 建立時就擋下來，不只事後叫。
///
/// # 為什麼兩層都要
///
/// `edit.rs` 的模組註解自己講過理由：lint 是**事後**的——東西已經建進去了
/// 才叫你回頭修，而它建出來之後長得像一條正常的契約。環境層的連線早就在
/// `validate_connection` 擋了同一件事，邏輯層的契約卻沒有。
///
/// L013 仍然留著，因為它蓋得到這裡蓋不到的：既有的舊資料，以及手改的
/// YAML（純文字格式本來就是要給人改的）。
mod rejected_at_creation {
    use super::*;
    use loom_core::edit::{self, Edit, EditError};
    use loom_core::logical::Relationship;
    use loom_core::resource::Resource;

    fn contract_to(project: &loom_core::Project, to: RelationshipEnd) -> Resource {
        let base = &project.logical.relationships[0];
        Resource::Relationship(Relationship {
            id: Id::new("r-新的"),
            slug: "新契約".into(),
            purpose: "測試".into(),
            from: base.from.clone(),
            to,
            to_endpoint: base.to_endpoint.clone(),
        })
    }

    #[test]
    fn adding_a_contract_that_targets_a_person_is_refused() {
        let project = with_a_person();
        let bad = contract_to(&project, RelationshipEnd::Person(Id::new("p-客戶")));

        let mut copy = project.clone();
        assert_eq!(
            edit::apply(&mut copy, &Edit::AddResource(bad)),
            Err(EditError::PersonAsTarget),
        );
        // 擋下來就是真的沒建進去。
        assert_eq!(
            copy.logical.relationships.len(),
            project.logical.relationships.len()
        );
    }

    #[test]
    fn a_person_on_the_source_end_is_still_allowed() {
        // 這正是它存在的用途：「使用者連上系統」。
        let mut project = with_a_person();
        let ok = contract_to(&project, project.logical.relationships[0].to.clone());
        let Resource::Relationship(mut rel) = ok else {
            unreachable!()
        };
        rel.from = RelationshipEnd::Person(Id::new("p-客戶"));

        edit::apply(
            &mut project,
            &Edit::AddResource(Resource::Relationship(rel)),
        )
        .expect("人當來源應該要過");
    }

    #[test]
    fn editing_an_existing_contract_into_that_shape_is_refused_too() {
        // 新增擋了、修改沒擋的話，繞一下就進去了。
        let mut project = with_a_person();
        let mut rel = project.logical.relationships[0].clone();
        rel.to = RelationshipEnd::Person(Id::new("p-客戶"));

        assert_eq!(
            edit::apply(
                &mut project,
                &Edit::UpdateResource(Resource::Relationship(rel))
            ),
            Err(EditError::PersonAsTarget),
        );
    }

    #[test]
    fn lint_still_catches_data_that_got_in_some_other_way() {
        // 手改 YAML 進來的、或這條檢查之前就存在的。兩層各守各的。
        let mut project = with_a_person();
        project.logical.relationships[0].to = RelationshipEnd::Person(Id::new("p-客戶"));

        assert_eq!(person_problems(&project).len(), 1);
    }
}
