//! Agent 怎麼處理外部系統實體。
//!
//! # 為什麼單獨一個檔
//!
//! 這種元素跟其他的不一樣：**它存在的唯一理由就是位址**。「金流在 prod
//! 打哪個位址」——名字與對應的系統只是拿來指認它的。
//!
//! 而位址在這裡本來填不到。`create kind=system_instance` 只收
//! environment / slug / system，`update` 也一樣。於是 Agent 建出一個空殼，
//! lint 立刻叫 L001「沒有指定位址」，而它**沒有任何工具解得掉**——
//! 那正是「工具指著問題卻沒給辦法」的狀態。

mod common;

use common::{Desk, empty, err, ok};
use loom_core::lint::{Rule, lint};
use loom_mcp::Workspace;
use serde_json::json;

/// 一個外部系統（單一接點）＋ 一個環境。
fn ready() -> Desk {
    let mut ws = empty();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "sso", "name": "單一登入", "external": true}},
            {"kind": "endpoint_def", "fields": {"owner": "sso", "slug": "https", "protocol": "tcp"}},
            {"kind": "environment", "fields": {"slug": "prod", "name": "正式"}}
        ]}),
    );
    ws
}

fn addresses(ws: &Desk) -> Vec<String> {
    ws.project()
        .unwrap()
        .environments
        .iter()
        .flat_map(|e| &e.systems)
        .flat_map(|s| &s.endpoints)
        .filter_map(|e| e.address.clone())
        .collect()
}

#[test]
fn creating_one_with_an_address_leaves_nothing_for_lint_to_complain_about() {
    let mut ws = ready();

    ok(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "sso-prod", "system": "sso",
            "address": "sso.corp.local:443"
        }}]}),
    );

    assert_eq!(addresses(&ws), ["sso.corp.local:443"]);
    let complaints: Vec<_> = lint(ws.project().unwrap())
        .into_iter()
        .filter(|f| f.rule == Rule::L001 || f.rule == Rule::L006)
        .collect();
    assert!(complaints.is_empty(), "{complaints:?}");
}

#[test]
fn without_an_address_lint_says_so_and_update_can_still_fix_it() {
    // 先建一個空殼是合法的（有時候位址還沒問到），但 lint 要叫，
    // 而且**要補得回來**——這正是原本補不回來的地方。
    let mut ws = ready();
    ok(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "sso-prod", "system": "sso"
        }}]}),
    );
    assert!(addresses(&ws).is_empty());

    let id = ws.project().unwrap().environments[0].systems[0]
        .id
        .to_string();
    ok(
        &mut ws,
        "update",
        json!({"id": id, "fields": {"address": "sso.corp.local:443"}}),
    );

    assert_eq!(addresses(&ws), ["sso.corp.local:443"]);
}

#[test]
fn filling_the_address_twice_does_not_grow_a_second_endpoint() {
    // 重跑一次 update 是 Agent 很常做的事（它會重試）。每次都長一個新接點
    // 的話，位址會有好幾份而且只有一份是對的。
    let mut ws = ready();
    ok(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "sso-prod", "system": "sso",
            "address": "舊的:443"
        }}]}),
    );
    let id = ws.project().unwrap().environments[0].systems[0]
        .id
        .to_string();
    ok(
        &mut ws,
        "update",
        json!({"id": id, "fields": {"address": "新的:443"}}),
    );

    assert_eq!(addresses(&ws), ["新的:443"], "應該是覆蓋，不是多一個");
    assert_eq!(
        ws.project().unwrap().environments[0].systems[0]
            .endpoints
            .len(),
        1
    );
}

#[test]
fn several_endpoint_defs_means_you_have_to_say_which() {
    // 猜錯的話位址會落到錯的接點上，而症狀是「明明填了位址卻還是說缺位址」。
    let mut ws = ready();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "endpoint_def", "fields": {"owner": "sso", "slug": "soap", "protocol": "tcp"}}
        ]}),
    );

    let complaint = err(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "sso-prod", "system": "sso", "address": "x:443"
        }}]}),
    );
    assert!(complaint.contains("請用 `endpoint` 指定"), "{complaint}");
    // 列出候選，Agent 才修得掉。
    assert!(
        complaint.contains("https") && complaint.contains("soap"),
        "{complaint}"
    );

    ok(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "sso-prod", "system": "sso",
            "address": "x:443", "endpoint": "soap"
        }}]}),
    );
    assert_eq!(addresses(&ws), ["x:443"]);
}

#[test]
fn a_system_with_no_endpoint_def_says_where_to_go_next() {
    // 空的錯誤訊息會讓 Agent 卡住。要講出下一步。
    let mut ws = empty();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "crm", "name": "客戶", "external": true}},
            {"kind": "environment", "fields": {"slug": "prod", "name": "正式"}}
        ]}),
    );

    let complaint = err(
        &mut ws,
        "create",
        json!({"items": [{"kind": "system_instance", "fields": {
            "environment": "prod", "slug": "crm-prod", "system": "crm", "address": "x:443"
        }}]}),
    );
    assert!(complaint.contains("還沒有定義任何接點"), "{complaint}");
    assert!(complaint.contains("endpoint_def"), "{complaint}");
}

#[test]
fn the_lint_report_tells_each_kind_of_l001_apart() {
    // 同一條規則報在三種東西上，三種修法完全不同。原本三種給同一句
    // 「用 create_nodes 建機器，或用 add_connection 補連線」——
    // 對外部系統來說**兩句都是錯的**。
    let mut ws = ready();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "container", "fields": {"slug": "web", "name": "前台", "system": "sso"}}
        ]}),
    );

    let report = ok(&mut ws, "lint", json!({}));

    assert!(
        report.contains("這個外部系統在這個環境還沒有實體"),
        "{report}"
    );
    assert!(report.contains("system_instance"), "{report}");
    assert!(
        report.contains("這個服務在這個環境一台都還沒建"),
        "{report}"
    );
    assert!(report.contains("create_nodes"), "{report}");
}

/// 打錯欄位名不可以安靜地過去。
///
/// # 為什麼這對 Agent 特別重要
///
/// 對人來說「填了沒進去」只是要重來一次；對 Agent 來說它會**照著往下走**，
/// 而錯誤要到很後面才以「lint 說缺位址」的形式冒出來——那時它已經完全
/// 不知道是哪一步的問題了。
mod unknown_fields {
    use super::*;

    #[test]
    fn a_typo_in_a_field_name_is_rejected_with_the_real_ones() {
        let mut ws = ready();

        let complaint = err(
            &mut ws,
            "create",
            json!({"items": [{"kind": "system_instance", "fields": {
                "environment": "prod", "slug": "sso-prod", "system": "sso",
                "adress": "sso.corp.local:443"
            }}]}),
        );

        assert!(
            complaint.contains("adress"),
            "要指出是哪一個字打錯：{complaint}"
        );
        assert!(
            complaint.contains("address"),
            "要列出真正收的欄位：{complaint}"
        );
    }

    #[test]
    fn a_field_that_belongs_to_another_kind_is_rejected_too() {
        // 機器沒有 name，只有 slug。送 name 過去原本是靜靜忽略。
        let mut ws = ready();
        let complaint = err(
            &mut ws,
            "create",
            json!({"items": [{"kind": "node", "fields": {
                "environment": "prod", "slug": "vm-1", "kind": "virtual-machine",
                "name": "第一台"
            }}]}),
        );

        assert!(complaint.contains("name"), "{complaint}");
    }

    #[test]
    fn the_kind_key_itself_is_not_a_field() {
        // `kind` 對 node 來說既是 create 用來挑資源種類的鍵、也是它自己的
        // 欄位。擋錯的話所有機器都建不起來。
        let mut ws = ready();
        ok(
            &mut ws,
            "create",
            json!({"items": [{"kind": "node", "fields": {
                "environment": "prod", "slug": "vm-1", "kind": "virtual-machine"
            }}]}),
        );
    }

    #[test]
    fn every_kind_that_create_accepts_can_actually_be_created() {
        // accepted_fields 漏掉一個真正會用到的欄位，症狀是「本來會動的
        // 東西突然建不起來」。這條把 create 說明裡列的每一種都走一遍。
        let mut ws = ready();
        ok(
            &mut ws,
            "create",
            json!({"items": [
                {"kind": "person", "fields": {"slug": "客戶", "name": "一般客戶"}},
                {"kind": "container", "fields": {"slug": "web", "name": "前台", "system": "sso"}},
                {"kind": "endpoint_def", "fields": {"owner": "web", "slug": "http", "protocol": "tcp"}},
                {"kind": "relationship", "fields": {
                    "slug": "web-連-sso", "purpose": "登入",
                    "from": "container:web", "to": "system:sso", "to_endpoint": "https"}},
                {"kind": "node", "fields": {
                    "environment": "prod", "slug": "dc", "kind": "site"}},
                {"kind": "node", "fields": {
                    "environment": "prod", "slug": "vm-9", "kind": "virtual-machine", "within": "dc"}},
                {"kind": "infra", "fields": {"environment": "prod", "slug": "f5-01"}},
                {"kind": "infra_endpoint", "fields": {
                    "environment": "prod", "owner": "f5-01", "slug": "vip", "address": "10.0.0.1:443"}},
                {"kind": "system_instance", "fields": {
                    "environment": "prod", "slug": "sso-prod", "system": "sso",
                    "address": "sso.corp.local:443"}}
            ]}),
        );
    }
}
