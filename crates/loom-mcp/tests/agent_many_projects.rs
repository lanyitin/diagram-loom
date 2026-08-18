//! Agent 同時面對好幾個專案。
//!
//! # 這裡守的是什麼
//!
//! 使用者可以開好幾扇視窗、各看一個專案，而 Agent 看得到全部。
//! 於是每一次呼叫都多了一個問題：**這一趟要動哪一個？**
//!
//! 答錯的後果是**安靜地改到別的專案**——沒有錯誤、沒有紅字，使用者要等到
//! 切去另一扇視窗才會發現。所以這裡不只驗「挑得對」，更要驗**挑不出來的
//! 時候會停下來**。
//!
//! # 為什麼在這一層測
//!
//! `pick` 的單元測試驗的是規則本身；這裡把 `wanted` → `resolve` → `call`
//! 串起來跑一遍，形狀跟 `src-tauri/src/mcp.rs` 的 `AppDesk::call` 一樣。
//! 差別只在那邊多了鎖與視窗，而那兩樣不需要在這裡驗。

mod common;

use common::Desk;
use loom_mcp::pick::{self, Open};
use loom_mcp::tools;
use serde_json::{Value, json};

/// 使用者開著的兩個專案。順序就是他開啟的順序。
fn two_desks() -> (Vec<Desk>, Vec<Open>) {
    let desks = vec![common::empty(), common::empty()];
    let open = vec![
        open("payments", "/w/payments.loom"),
        open("billing", "/w/billing.loom"),
    ];
    (desks, open)
}

fn open(slug: &str, root: &str) -> Open {
    Open {
        slug: slug.into(),
        name: slug.into(),
        root: root.into(),
        dirty: false,
        errors: 0,
        warnings: 0,
    }
}

/// 走一遍 `AppDesk::call` 的形狀：先挑，再只動挑中的那一個。
fn call(desks: &mut [Desk], open: &[Open], name: &str, args: Value) -> Result<String, String> {
    if !tools::needs_project(name) {
        return Ok(pick::listing(open));
    }
    let picked = pick::resolve(open, tools::wanted(&args))?;
    tools::call(&mut desks[picked], open, name, &args)
}

/// 這個專案裡看不看得到某個東西。用來確認到底改到了哪一份。
fn has(desk: &mut Desk, open: &[Open], slug: &str) -> bool {
    tools::call(desk, open, "describe", &json!({}))
        .unwrap()
        .contains(slug)
}

/// 一個最小的可建項目。
fn a_system(slug: &str) -> Value {
    json!({"kind": "system", "fields": {"slug": slug, "name": slug, "external": false}})
}

#[test]
fn naming_the_project_puts_the_change_in_that_one_only() {
    let (mut desks, open) = two_desks();

    call(
        &mut desks,
        &open,
        "create",
        json!({"project": "billing", "items": [a_system("收款閘道")]}),
    )
    .unwrap();

    assert!(
        !has(&mut desks[0], &open, "收款閘道"),
        "payments 不該被動到"
    );
    assert!(has(&mut desks[1], &open, "收款閘道"));
}

#[test]
fn leaving_it_out_stops_instead_of_guessing() {
    // 這是整個模組存在的理由。挑一個來做的話，使用者會在完全不知情的
    // 情況下得到一個被改壞的專案。
    let (mut desks, open) = two_desks();

    let err = call(
        &mut desks,
        &open,
        "create",
        json!({"items": [a_system("隨便")]}),
    )
    .unwrap_err();

    assert!(err.contains("你沒說要動哪一個"), "{err}");
    assert!(err.contains("payments") && err.contains("billing"), "{err}");
    assert!(!has(&mut desks[0], &open, "隨便"), "兩邊都不該被動到");
    assert!(!has(&mut desks[1], &open, "隨便"));
}

#[test]
fn a_single_project_still_needs_no_naming() {
    // 只開一個的時候用法要跟多專案之前完全一樣，否則每個既有的
    // Agent 對話都得改。
    let mut desks = vec![common::empty()];
    let open = vec![open("payments", "/w/payments.loom")];

    call(
        &mut desks,
        &open,
        "create",
        json!({"items": [a_system("只有一個")]}),
    )
    .unwrap();

    assert!(has(&mut desks[0], &open, "只有一個"));
}

#[test]
fn projects_answers_even_when_several_are_open() {
    // `projects` 是 Agent 唯一能問路的工具。它要是也被「你沒說要動哪一個」
    // 擋下來，那就變成唯一問不到的那一個——而它正是用來回答那個問題的。
    let (mut desks, open) = two_desks();

    let text = call(&mut desks, &open, "projects", json!({})).unwrap();

    assert!(text.contains("payments"), "{text}");
    assert!(text.contains("billing"), "{text}");
    assert!(text.contains("/w/billing.loom"), "撞名時只有路徑分得開");
}

#[test]
fn a_wrong_name_lists_what_is_actually_open() {
    let (mut desks, open) = two_desks();

    let err = call(&mut desks, &open, "describe", json!({"project": "crm"})).unwrap_err();

    assert!(err.contains("沒有開著叫做「crm」的專案"), "{err}");
    // 只說「找不到」的話 Agent 只能猜。列出來它才修得掉。
    assert!(err.contains("payments"), "{err}");
}

#[test]
fn every_tool_accepts_a_project_argument() {
    // 少一個工具沒補上 `scope.project`，症狀是「那個工具沒辦法指定專案」——
    // Agent 只會覺得莫名其妙。schema 由 `tool()` 統一補，這裡守住它。
    let list = tools::list();
    for t in list.as_array().expect("工具清單是陣列") {
        let name = t["name"].as_str().unwrap();
        if !tools::needs_project(name) {
            continue;
        }
        let scope = &t["inputSchema"]["properties"]["scope"]["properties"];
        assert!(
            scope.get("project").is_some(),
            "工具 {name} 的 schema 少了 scope.project",
        );
    }
}

#[test]
fn the_old_top_level_project_argument_still_picks_the_right_one() {
    // 形狀換成 `scope.project` 之後，舊 Agent 手上那份設定寫的還是舊名字。
    // 收它的理由跟收單數 items 一樣：那一趟看起來完全正常，
    // 擋下來的話它只會拿到「你沒說要動哪一個」，然後開始亂猜。
    let (mut desks, open) = two_desks();

    let new_shape = call(
        &mut desks,
        &open,
        "describe",
        json!({"scope": {"project": "payments"}}),
    )
    .unwrap();
    let old_shape = call(
        &mut desks,
        &open,
        "describe",
        json!({"project": "payments"}),
    )
    .unwrap();

    assert_eq!(new_shape, old_shape);
}
