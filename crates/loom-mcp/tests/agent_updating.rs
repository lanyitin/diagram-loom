//! 改既有的東西。
//!
//! # 這份測試在驗什麼
//!
//! `update` 以前是這一層唯一的單數工具，而且**只吃 id、碰不到連線**。
//! 三個後果，這裡各驗一組：
//!
//! 1. 補三十個位址要走三十趟，每趟回一份幾乎一樣的報告 → 現在收 `items`
//! 2. 要 id 就得先整份 describe 一次 → 現在 `target` 吃名字
//! 3. 連線只能刪掉重建，而重建**會換一個新的 id** → 現在改得動
//!
//! 第三點的代價最隱晦：圖上的形狀是靠 `loomId` 綁在連線上的，
//! 換了 id 等於那條線的標註全部斷掉，而畫面上不會有任何錯誤。

mod common;

use serde_json::json;

use common::*;
use loom_mcp::Workspace;

// ── 批次 ────────────────────────────────────────────────────────

#[test]
fn a_batch_of_updates_is_one_undo_step() {
    // 人看過覺得不對的時候，退回去該是一次，不是三十次。
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "redis-01", "fields": {"memo": "年底汰換"}},
            {"target": "redis-02", "fields": {"memo": "年底汰換"}},
            {"target": "redis-03", "fields": {"memo": "年底汰換"}}
        ]}),
    );
    assert!(out.contains("改好了 3 個"), "{out}");

    let h = ws.0.as_mut().unwrap();
    assert!(h.undo(), "退不回去");
    // 一次就要全部退掉。退了一次還剩兩個備註的話，人得再按兩次，
    // 而他不會知道還要按幾次。
    let left = h
        .project()
        .environments
        .iter()
        .flat_map(|e| e.instances())
        .filter(|i| i.memo == "年底汰換")
        .count();
    assert_eq!(left, 0, "按一次復原只退了一部分");
}

#[test]
fn one_bad_item_changes_nothing_and_says_which() {
    let mut ws = a_two_environment_project();
    let before = ws.project().unwrap().clone();

    let e = err(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "redis-01", "fields": {"memo": "改得動"}},
            {"target": "根本沒這個", "fields": {"memo": "改不動"}}
        ]}),
    );
    assert!(e.contains("第 2 項"), "沒說是第幾項：{e}");
    assert_eq!(ws.project().unwrap(), &before, "壞掉的那一批卻改了一半");
}

#[test]
fn later_items_can_name_what_an_earlier_item_renamed() {
    // 改了 slug 之後，同一批裡後面那一項指名的就是新名字。
    // 不在複本上邊改邊解的話，那一項會說找不到。
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "redis-01", "fields": {"slug": "cache-01"}},
            {"target": "cache-01", "fields": {"memo": "改名之後還找得到"}}
        ]}),
    );
    assert!(out.contains("改好了 2 個"), "{out}");
}

#[test]
fn a_dry_run_changes_nothing_but_still_says_what_would_break() {
    let mut ws = a_two_environment_project();
    let before = ws.project().unwrap().clone();

    let out = ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "dry_run": true, "items": [
            {"target": "redis-01", "fields": {"memo": "只是看看"}}
        ]}),
    );
    assert!(out.contains("預覽"), "{out}");
    assert_eq!(ws.project().unwrap(), &before, "dry_run 竟然真的改了");
}

// ── 用名字指名 ──────────────────────────────────────────────────

#[test]
fn a_name_that_exists_in_two_environments_is_never_guessed() {
    // `redis-01` 在 prod 與 uat 各有一台是**常態**。挑錯的後果是安靜地
    // 改到另一個環境——沒有錯誤、沒有紅字，使用者要切過去才會發現。
    let mut ws = a_two_environment_project();
    let e = err(
        &mut ws,
        "update",
        json!({"items": [{"target": "redis-01", "fields": {"memo": "哪一個？"}}]}),
    );

    assert!(e.contains("不會替你挑"), "{e}");
    assert!(e.contains("prod"), "沒列出候選：{e}");
    assert!(e.contains("uat"), "沒列出候選：{e}");
    assert!(e.contains("scope"), "沒告訴它下一步寫什麼：{e}");
}

#[test]
fn scoping_to_one_environment_resolves_the_name() {
    let mut ws = a_two_environment_project();
    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "uat"}, "items": [
            {"target": "redis-01", "fields": {"memo": "這是 uat 那一台"}}
        ]}),
    );

    let uat = ws
        .project()
        .unwrap()
        .environments
        .iter()
        .find(|e| e.slug == "uat")
        .unwrap();
    assert!(
        uat.instances().iter().any(|i| i.memo == "這是 uat 那一台"),
        "改到別的環境去了"
    );
}

#[test]
fn an_id_still_works_as_a_target() {
    // `describe` 預設不印 id 了，但舊 Agent 手上還有 create 回傳的那些。
    let mut ws = a_two_environment_project();
    let id = id_of(&mut ws, "服務", "redis");

    let out = ok(
        &mut ws,
        "update",
        json!({"items": [{"target": id, "fields": {"name": "快取叢集"}}]}),
    );
    assert!(out.contains("改好了"), "{out}");
}

#[test]
fn the_old_singular_shape_still_works() {
    let mut ws = a_two_environment_project();
    let id = id_of(&mut ws, "服務", "redis");
    let out = ok(
        &mut ws,
        "update",
        json!({"id": id, "fields": {"name": "快取"}}),
    );
    assert!(out.contains("改好了"), "{out}");
}

#[test]
fn a_name_that_does_not_exist_lists_candidates_and_a_way_out() {
    let mut ws = a_two_environment_project();
    let e = err(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"},
               "items": [{"target": "redsi-01", "fields": {"memo": "打錯字"}}]}),
    );
    assert!(e.contains("redis-01"), "沒列出候選：{e}");
    assert!(e.contains("describe"), "清單被截斷時就沒別條路了：{e}");
}

// ── 連線 ────────────────────────────────────────────────────────

#[test]
fn a_connection_can_be_named_by_its_two_ends() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "* -> redis-*", "fields": {"purpose": "讀寫工作階段快取"}}
        ]}),
    );
    assert!(out.contains("改好了"), "{out}");
    assert!(out.contains("連線"), "回報沒講清楚改的是連線：{out}");
}

#[test]
fn a_connection_can_be_named_by_the_contract_it_serves() {
    let mut ws = a_two_environment_project();
    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"purpose": "讀寫快取（改過）"}}
        ]}),
    );

    let prod = ws
        .project()
        .unwrap()
        .environments
        .iter()
        .find(|e| e.slug == "prod")
        .unwrap();
    assert!(
        prod.connections.iter().any(|c| c.purpose.contains("改過")),
        "沒改到"
    );
}

#[test]
fn the_same_contract_in_two_environments_is_never_guessed() {
    let mut ws = a_two_environment_project();
    let e = err(
        &mut ws,
        "update",
        json!({"items": [{"target": "conn:apache-to-redis", "fields": {"purpose": "哪一個？"}}]}),
    );
    assert!(e.contains("prod"), "{e}");
    assert!(e.contains("uat"), "{e}");
}

#[test]
fn changing_a_connection_keeps_its_id() {
    // **這是整組裡最重要的一條。** 以前唯一的做法是刪掉重建，而重建會
    // 換一個新的 id——圖上綁著它的標註因此全部斷掉，畫面上卻不會報錯。
    let mut ws = a_two_environment_project();
    let before: Vec<String> = connection_ids(&ws, "prod");

    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"purpose": "改過了"}}
        ]}),
    );

    assert_eq!(before, connection_ids(&ws, "prod"), "連線的 id 竟然換了");
}

#[test]
fn a_connection_can_change_how_many_it_expects() {
    // L004／L005 的修法。以前 lint 說「用 update 改 expect」，
    // 而 update 根本碰不到連線——它教了一條不存在的路。
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"to_expect": 6}}
        ]}),
    );

    // 只有三台卻期望六台，所以這一改應該當場冒出 L004。
    assert!(out.contains("L004"), "改了 expect 卻沒人叫：{out}");
}

#[test]
fn a_connection_can_become_a_fallback_path() {
    let mut ws = a_two_environment_project();
    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"kind": "fallback"}}
        ]}),
    );

    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "prod"}, "select": {"kinds": ["connection"]}}),
    );
    assert!(out.contains("備援"), "{out}");
}

#[test]
fn an_unknown_connection_kind_lists_the_two_that_work() {
    let mut ws = a_two_environment_project();
    let e = err(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"kind": "backup"}}
        ]}),
    );
    assert!(e.contains("primary"), "{e}");
    assert!(e.contains("fallback"), "{e}");
}

#[test]
fn a_connection_takes_a_memo_and_hands_it_back() {
    // 備註是這個模型裡唯一**沒有規則會回報**的欄位：寫錯了 lint 不會叫。
    // 所以「讀得回來」跟「寫得進去」一樣重要——讀不回來的話 Agent 會以為
    // 沒寫成功，然後再寫一次。
    let mut ws = a_two_environment_project();
    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"memo": "等年底汰換"}}
        ]}),
    );

    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "prod"}, "select": {"kinds": ["connection"]}}),
    );
    assert!(out.contains("等年底汰換"), "備註讀不回來：{out}");
}

#[test]
fn a_connection_memo_does_not_leak_into_the_purpose() {
    // 兩個欄位是刻意分開的：L007 在看 `purpose`。寫進同一格的話，
    // 「等年底汰換」會讓 L007 從此不再叫——那是最不會被發現的一種失效。
    let mut ws = a_two_environment_project();
    ok(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {"memo": "等年底汰換"}}
        ]}),
    );

    let prod = ws
        .project()
        .unwrap()
        .environments
        .iter()
        .find(|e| e.slug == "prod")
        .unwrap();
    let conn = prod
        .connections
        .iter()
        .find(|c| c.memo == "等年底汰換")
        .expect("備註沒寫進去");
    assert!(!conn.purpose.contains("汰換"), "備註跑到用途裡去了");
}

#[test]
fn a_connection_memo_can_be_cleared() {
    // 備註沒有規則在看，所以寫錯了沒有第二條路可以救。
    let mut ws = a_two_environment_project();
    for memo in ["寫錯了", ""] {
        ok(
            &mut ws,
            "update",
            json!({"scope": {"environment": "prod"}, "items": [
                {"target": "conn:apache-to-redis", "fields": {"memo": memo}}
            ]}),
        );
    }

    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "prod"}, "select": {"kinds": ["connection"]}}),
    );
    assert!(!out.contains("寫錯了"), "清不掉：{out}");
}

#[test]
fn changing_nothing_at_all_is_refused() {
    let mut ws = a_two_environment_project();
    let e = err(
        &mut ws,
        "update",
        json!({"scope": {"environment": "prod"}, "items": [
            {"target": "conn:apache-to-redis", "fields": {}}
        ]}),
    );
    assert!(e.contains("purpose"), "沒告訴它收什麼：{e}");
}

// ── 刪除也吃名字 ────────────────────────────────────────────────

#[test]
fn delete_takes_names_too() {
    // 兩支工具的 `target` 是同一套寫法，所以 Agent 學一次用兩處。
    let mut ws = a_two_environment_project();
    let preview = ok(
        &mut ws,
        "delete",
        json!({"scope": {"environment": "prod"}, "targets": ["redis-03"]}),
    );
    assert!(preview.contains("預覽"), "{preview}");

    let done = ok(
        &mut ws,
        "delete",
        json!({"scope": {"environment": "prod"}, "targets": ["redis-03"], "confirm": true}),
    );
    assert!(done.contains("刪掉了 1 個"), "{done}");
}

#[test]
fn delete_can_name_a_connection_by_its_ends() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "delete",
        json!({"scope": {"environment": "uat"},
               "targets": ["* -> redis-*"], "confirm": true}),
    );
    assert!(out.contains("刪掉了 1 個"), "{out}");
}

/// 某個環境現在有哪些連線 id。
fn connection_ids(ws: &Desk, env: &str) -> Vec<String> {
    let mut ids: Vec<String> = ws
        .project()
        .unwrap()
        .environments
        .iter()
        .find(|e| e.slug == env)
        .unwrap()
        .connections
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    ids.sort();
    ids
}
