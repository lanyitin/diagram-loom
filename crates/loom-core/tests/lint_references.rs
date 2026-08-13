//! L012 參照完整性：模型元素指向一個不存在的模型元素。
//!
//! # 這條是「可以刪除」的前置條件
//!
//! 在有這條之前，把一個服務從邏輯層刪掉，指著它的那批落地會**安靜地**
//! 留在專案裡指著空氣——lint 一句話都不說。對一個賣點是「怕漏」的工具，
//! 那是最不能出的錯：資料壞了，而唯一該發現的機制沒發現。
//!
//! 使用者選的策略是「允許懸空 ＋ lint 報錯」（見 `docs/decisions.md`），
//! 所以刪除不會被擋，但**一定要叫**。這裡守的就是「一定要叫」。

mod common;

use common::*;
use loom_core::environment::ConnectionEnd;
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity, lint};
use loom_core::logical::RelationshipEnd;

/// 只挑 L012，其他規則會不會跟著叫是另一回事。
fn 參照問題(project: &loom_core::Project) -> Vec<String> {
    lint(project)
        .iter()
        .filter(|f| f.rule == Rule::L012)
        .map(|f| f.detail.clone())
        .collect()
}

#[test]
fn 健康的專案不會報參照問題() {
    assert_eq!(參照問題(&healthy_project()), Vec::<String>::new());
}

#[test]
fn 刪掉服務之後_指著它的落地會被叫出來() {
    // 這就是當初挖出這條規則的那個實驗：在有 L012 之前，lint 回傳空陣列。
    let mut project = healthy_project();
    project
        .logical
        .containers
        .retain(|c| c.id != Id::new(REDIS));

    let 叫的 = 參照問題(&project);
    assert!(
        叫的.iter().any(|d| d.contains("指向不存在的服務")),
        "刪掉服務之後 lint 竟然沒話說：{叫的:?}"
    );
    // prod 三台、test 兩台、dev 一台，每一台都要點名——
    // 只說「有東西壞了」而不說是哪幾台，使用者還是得自己找。
    assert_eq!(叫的.iter().filter(|d| d.starts_with("落地")).count(), 6);
}

#[test]
fn 是錯誤不是警告() {
    // 資料真的壞了。警告會被當成「之後再說」。
    let mut project = healthy_project();
    project
        .logical
        .containers
        .retain(|c| c.id != Id::new(REDIS));

    let f = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L012)
        .unwrap();
    assert_eq!(f.severity(), Severity::Error);
}

#[test]
fn 刪掉服務之後_契約指著它也會被叫出來() {
    let mut project = healthy_project();
    project
        .logical
        .containers
        .retain(|c| c.id != Id::new(REDIS));

    let 叫的 = 參照問題(&project);
    assert!(
        叫的.iter().any(|d| d.contains("目標端指向不存在的服務")),
        "契約還指著被刪掉的服務，卻沒被叫出來：{叫的:?}"
    );
}

#[test]
fn 契約指著不存在的人() {
    let mut project = healthy_project();
    project.logical.relationships[0].from = RelationshipEnd::Person(Id::new("p-沒這個人"));

    let 叫的 = 參照問題(&project);
    assert_eq!(叫的.len(), 1);
    assert!(叫的[0].contains("來源端指向不存在的人"));
}

#[test]
fn 說得出是契約的哪一端() {
    // 兩端都可能壞掉，不指名的話兩項發現長得一模一樣。
    let mut project = healthy_project();
    project.logical.relationships[0].from = RelationshipEnd::Container(Id::new("c-沒這個"));
    project.logical.relationships[0].to = RelationshipEnd::Container(Id::new("c-也沒這個"));

    let 兩端: Vec<_> = lint(&project)
        .into_iter()
        .filter(|f| f.rule == Rule::L012 && f.subject == project.logical.relationships[0].id)
        .filter_map(|f| f.end)
        .collect();
    assert!(兩端.contains(&ConnectionEnd::From));
    assert!(兩端.contains(&ConnectionEnd::To));
}

#[test]
fn 服務屬於一個不存在的系統() {
    let mut project = healthy_project();
    project.logical.containers[0].system = Id::new("s-被刪掉的系統");

    let 叫的 = 參照問題(&project);
    assert_eq!(叫的.len(), 1);
    assert!(叫的[0].contains("屬於一個不存在的系統"));
}

#[test]
fn 刪掉接點定義之後_用到它的落地會被叫出來() {
    // 這是最容易漏的一種：接點定義只是邏輯層的一個小條目，
    // 刪掉之後每個環境的實際 endpoint 都還在，看起來一切正常。
    let mut project = healthy_project();
    for c in &mut project.logical.containers {
        c.endpoints.retain(|e| e.id != Id::new(REDIS_CLIENT));
    }

    let 叫的 = 參照問題(&project);
    assert!(
        叫的.iter().any(|d| d.contains("指向不存在的接點定義")),
        "落地上的 endpoint 還指著被刪掉的定義：{叫的:?}"
    );
    assert!(
        叫的.iter().any(|d| d.contains("目標身上沒有接點定義")),
        "契約還指著被刪掉的接點定義：{叫的:?}"
    );
}

#[test]
fn 契約的目標接點掛在別人身上也算壞掉() {
    // 指到「存在但不屬於目標」的接點定義。連線展開時會找不到實際 endpoint，
    // 而那個錯誤會出現在很遠的地方，很難追回來。
    let mut project = healthy_project();
    project.logical.relationships[0].to_endpoint = Id::new(PAY_HTTPS);

    let 叫的 = 參照問題(&project);
    assert!(
        叫的.iter().any(|d| d.contains("目標身上沒有接點定義")),
        "接點掛在別人身上卻沒被叫出來：{叫的:?}"
    );
}

#[test]
fn 外部系統落地指著不存在的系統() {
    let mut project = healthy_project();
    project.logical.systems.retain(|s| s.id != Id::new(PAYMENT));

    let 叫的 = 參照問題(&project);
    assert!(
        叫的.iter().any(|d| d.contains("指向不存在的外部系統")),
        "外部系統被刪掉，落地卻沒被叫出來：{叫的:?}"
    );
}

#[test]
fn 擁有者不存在時不會重複叫接點的問題() {
    // 服務都不見了，它底下每個 endpoint 再各叫一次「定義不存在」只是噪音。
    // 誤報跟漏報一樣糟——使用者一旦習慣忽略雜訊，真正的問題也會被忽略。
    let mut project = healthy_project();
    project
        .logical
        .containers
        .retain(|c| c.id != Id::new(REDIS));

    let 叫的 = 參照問題(&project);
    assert!(
        !叫的.iter().any(|d| d.contains("指向不存在的接點定義")),
        "服務不存在時還多叫了接點的問題：{叫的:?}"
    );
}

#[test]
fn 設備的接點沒有邏輯層對應_不該被誤報() {
    // F5 這類設備不對應任何邏輯層元素，它的 endpoint 沒有 `def`。
    // 這是刻意的設計，不是缺漏。
    let project = healthy_project();
    assert!(
        project.environments[0].infra[0].endpoints[0].def.is_none(),
        "素材本身要有一個沒有 def 的設備接點，這個測試才有意義"
    );
    assert_eq!(參照問題(&project), Vec::<String>::new());
}
