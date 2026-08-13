//! 一次一批，不是一次一個。
//!
//! # 這份測試在驗什麼
//!
//! 真實情境：使用者請一個 Agent 分析一個很大的系統。分析很快就跑完，
//! 然後它花了非常久在**一趟一趟地呼叫工具建資源**；想大改的時候，
//! 因為刪除也是一次一個，它甚至建議使用者「不如重開一個新專案」。
//!
//! 所以這裡驗的不是「批次會不會生效」，而是那三件讓它慢下來、
//! 讓它想放棄的事有沒有真的解決：
//!
//! 1. 同一批裡後面的**指名得到**前面的（不然還是得拆成很多趟）
//! 2. 失敗時**整批不建**，而且說得出是第幾項
//! 3. 刪除掃得掉連帶壞掉的東西，而且**動手前先給看**

mod common;

use serde_json::json;

use common::*;
use loom_core::lint::lint;
use loom_mcp::Workspace;

/// 一份小專案的邏輯層，全部塞在一次呼叫裡。
///
/// 這正是重點：系統 → 服務 → 接點定義 → 契約，四層相依關係，一趟送完。
fn logical_layer_in_one_call(ws: &mut Desk) -> String {
    ok(
        ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單系統", "external": false}},
            {"kind": "container", "fields": {"slug": "apache", "name": "反向代理", "system": "shop"}},
            {"kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "shop"}},
            {"kind": "endpoint_def", "fields": {"owner": "apache", "slug": "http", "protocol": "tcp"}},
            {"kind": "endpoint_def", "fields": {"owner": "redis", "slug": "client-port", "protocol": "tcp"}},
            {"kind": "person", "fields": {"slug": "customer", "name": "使用者"}},
            {"kind": "relationship", "fields": {
                "slug": "user-to-apache", "purpose": "使用者連進系統",
                "from": "person:customer", "to": "container:apache", "to_endpoint": "http"}},
            {"kind": "relationship", "fields": {
                "slug": "apache-to-redis", "purpose": "讀寫快取",
                "from": "container:apache", "to": "container:redis", "to_endpoint": "client-port"}},
            {"kind": "environment", "fields": {"slug": "prod", "name": "正式"}}
        ]}),
    )
}

#[test]
fn later_items_can_name_things_created_earlier_in_the_same_batch() {
    // 少了這一條，批次就是假的：服務要指名它的系統，契約要指名兩端的服務，
    // 如果整批都對著「還沒動過的專案」解名字，第二項就會說找不到——
    // 而那樣 Agent 只能退回一趟一個。
    let mut ws = empty();
    let out = logical_layer_in_one_call(&mut ws);

    assert!(out.contains("建好了 9 個"), "{out}");
    let p = ws.project().unwrap();
    assert_eq!(p.logical.systems.len(), 1);
    assert_eq!(p.logical.containers.len(), 2);
    assert_eq!(p.logical.relationships.len(), 2);
    assert_eq!(p.environments.len(), 1);
}

#[test]
fn a_small_batch_hands_back_every_id() {
    // 少數幾個的時候直接給 id，省 Agent 一趟 describe。
    let mut ws = empty();
    let out = ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單", "external": false}},
            {"kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "shop"}}
        ]}),
    );
    assert_eq!(out.matches("（id: ").count(), 2, "{out}");
}

#[test]
fn a_huge_batch_reports_counts_instead_of_a_wall_of_ids() {
    // 三百個 id 是三百行雜訊，而 Agent 當下用不到——它是照 slug 在思考的。
    let mut ws = empty();
    let mut items = vec![
        json!({"kind": "system", "fields": {"slug": "shop", "name": "訂單", "external": false}}),
    ];
    for i in 0..40 {
        items.push(json!({"kind": "container", "fields": {
            "slug": format!("svc-{i:02}"), "name": format!("服務 {i}"), "system": "shop"}}));
    }
    let out = ok(&mut ws, "create", json!({ "items": items }));

    assert!(out.contains("建好了 41 個"), "{out}");
    assert!(out.contains("system 1"), "{out}");
    assert!(out.contains("container 40"), "{out}");
    assert!(
        !out.contains("（id: "),
        "數量這麼多不該把 id 全列出來：{out}"
    );
    assert!(out.contains("describe"), "沒告訴它去哪裡拿 id：{out}");
}

#[test]
fn one_bad_item_builds_nothing_at_all() {
    // 「建到第 3 個才失敗」是最糟的結果：看起來成功了，其實模型是殘的，
    // 而殘在哪裡沒有人知道。
    let mut ws = empty();
    let e = err(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單", "external": false}},
            {"kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "shop"}},
            {"kind": "container", "fields": {"slug": "apache", "name": "RP", "system": "打錯了"}}
        ]}),
    );

    assert!(e.contains("第 3 項"), "沒說是第幾項壞掉：{e}");
    assert!(e.contains("整批都沒有建"), "{e}");
    assert!(e.contains("找不到系統 打錯了"), "沒說原因：{e}");
    assert!(e.contains("shop"), "沒列出有哪些可以選：{e}");

    let p = ws.project().unwrap();
    assert!(p.logical.systems.is_empty(), "失敗的批次卻建了前兩項");
    assert!(p.logical.containers.is_empty());
}

#[test]
fn a_duplicate_inside_one_batch_is_caught_with_its_position() {
    // Agent 讀同一段文字讀出兩次是常有的事。撞名在同一批裡也要擋，
    // 而且要說是第幾項——不然它得自己把幾百項比對一遍。
    let mut ws = empty();
    let e = err(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單", "external": false}},
            {"kind": "system", "fields": {"slug": "shop", "name": "又一次", "external": false}}
        ]}),
    );
    assert!(e.contains("第 2 項"), "{e}");
    assert!(e.contains("已經有一個叫 shop"), "{e}");
}

#[test]
fn a_single_item_error_stays_short() {
    // 只送一項的時候講「第 1 項（共 1 項）」是噪音。
    let mut ws = empty();
    let e = err(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "沒有這個"}}
        ]}),
    );
    assert!(!e.contains("第 1 項"), "{e}");
}

#[test]
fn the_old_one_at_a_time_shape_still_works() {
    // 工具描述只教 items，但模型有時會憑印象送出舊形狀。
    // 那時候回一個它得重試才懂的錯誤，不如照做。
    let mut ws = empty();
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單", "external": false}),
    );
    assert_eq!(ws.project().unwrap().logical.systems.len(), 1);
}

#[test]
fn the_whole_batch_undoes_in_one_press() {
    // Agent 建了一批、人看過覺得不對——這正是最需要一鍵退回的時候。
    let mut ws = empty();
    logical_layer_in_one_call(&mut ws);

    let h = ws.0.as_mut().unwrap();
    assert!(h.undo(), "退不回去");
    assert!(
        h.project().logical.systems.is_empty(),
        "按一次復原只退了一部分"
    );
    assert!(!h.undo(), "一批應該只留一步");
}

#[test]
fn every_connection_in_an_environment_goes_in_one_call() {
    // 連線通常是整份專案裡數量最多的東西。一條一趟會慢到不能用。
    let mut ws = empty();
    logical_layer_in_one_call(&mut ws);
    ok(
        &mut ws,
        "create_nodes",
        json!({"environment": "prod", "container": "apache", "count": 1,
               "address_template": "10.0.0.{ip}:8080", "ip_start": 11}),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({"environment": "prod", "container": "redis", "count": 3,
               "address_template": "10.0.1.{ip}:6379", "ip_start": 11}),
    );

    let out = ok(
        &mut ws,
        "add_connection",
        json!({"environment": "prod", "items": [
            {"relationship": "user-to-apache"},
            {"relationship": "apache-to-redis"}
        ]}),
    );
    assert!(out.contains("建好 2 段"), "{out}");
    assert!(
        lint(ws.project().unwrap()).is_empty(),
        "一批送完之後專案應該是乾淨的"
    );
}

#[test]
fn both_segments_of_an_f5_hop_fit_in_the_same_batch() {
    // 拆兩段是 Agent 最容易做錯的一件事。既然要它拆，就不該還要它送兩趟。
    let mut ws = empty();
    ok(
        &mut ws,
        "create",
        json!({"items": [
            {"kind": "system", "fields": {"slug": "shop", "name": "訂單", "external": false}},
            {"kind": "container", "fields": {"slug": "apache", "name": "RP", "system": "shop"}},
            {"kind": "container", "fields": {"slug": "gateway", "name": "GW", "system": "shop"}},
            {"kind": "endpoint_def", "fields": {"owner": "apache", "slug": "http", "protocol": "tcp"}},
            {"kind": "endpoint_def", "fields": {"owner": "gateway", "slug": "http", "protocol": "tcp"}},
            {"kind": "relationship", "fields": {
                "slug": "apache-to-gateway", "purpose": "轉送",
                "from": "container:apache", "to": "container:gateway", "to_endpoint": "http"}},
            {"kind": "environment", "fields": {"slug": "prod", "name": "正式"}},
            {"kind": "infra", "fields": {"environment": "prod", "slug": "f5-01"}},
            {"kind": "infra_endpoint", "fields": {
                "environment": "prod", "owner": "f5-01",
                "slug": "vip-gateway", "address": "10.0.0.100:8080"}}
        ]}),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({"environment": "prod", "container": "apache", "count": 3,
               "address_template": "10.0.0.{ip}:8080", "ip_start": 11}),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({"environment": "prod", "container": "gateway", "count": 3,
               "address_template": "10.0.1.{ip}:8080", "ip_start": 11}),
    );

    ok(
        &mut ws,
        "add_connection",
        json!({"environment": "prod", "items": [
            {"relationship": "apache-to-gateway", "from": "apache-*", "to": "f5-01 : vip-gateway"},
            {"relationship": "apache-to-gateway", "from": "f5-01", "to": "gateway-*"}
        ]}),
    );
    assert_eq!(ok(&mut ws, "lint", json!({})), "沒有發現任何缺漏。");
}

// ── 刪除 ────────────────────────────────────────────────────────

mod deleting {
    use super::*;

    /// 一份建好又 lint 乾淨的小專案，接著要對它動刀。
    fn a_finished_project() -> Desk {
        let mut ws = empty();
        logical_layer_in_one_call(&mut ws);
        ok(
            &mut ws,
            "create_nodes",
            json!({"environment": "prod", "container": "apache", "count": 1,
                   "address_template": "10.0.0.{ip}:8080", "ip_start": 11}),
        );
        ok(
            &mut ws,
            "create_nodes",
            json!({"environment": "prod", "container": "redis", "count": 3,
                   "address_template": "10.0.1.{ip}:6379", "ip_start": 11}),
        );
        ok(
            &mut ws,
            "add_connection",
            json!({"environment": "prod", "items": [
                {"relationship": "user-to-apache"},
                {"relationship": "apache-to-redis"}
            ]}),
        );
        assert!(lint(ws.project().unwrap()).is_empty(), "素材本身就不乾淨");
        ws
    }

    #[test]
    fn the_first_call_only_shows_what_would_happen() {
        // cascade 是這個專案裡唯一一個「按一下消失幾十個東西」的操作。
        // 對一個賣點是怕漏的工具，讓 Agent 一句話掃掉半個模型是說不過去的。
        let mut ws = a_finished_project();
        let redis = id_of(&mut ws, "服務", "redis");
        let before = ws.project().unwrap().clone();

        let out = ok(&mut ws, "delete", json!({"ids": [redis], "cascade": true}));

        assert!(out.contains("還沒有刪任何東西"), "{out}");
        assert!(out.contains("confirm"), "沒告訴它下一步怎麼做：{out}");
        assert_eq!(ws.project().unwrap(), &before, "預覽卻改了東西");
    }

    #[test]
    fn the_preview_lists_everything_that_will_be_swept_up() {
        let mut ws = a_finished_project();
        let redis = id_of(&mut ws, "服務", "redis");

        let out = ok(&mut ws, "delete", json!({"ids": [redis], "cascade": true}));

        assert!(out.contains("服務 redis"), "{out}");
        assert!(out.contains("契約 apache-to-redis"), "{out}");
        assert!(out.contains("服務實體 redis-01"), "{out}");
        assert!(out.contains("prod 的一條連線"), "{out}");
    }

    #[test]
    fn confirming_does_it_and_leaves_nothing_dangling() {
        // 這是連帶刪除唯一的驗收標準：掃完之後不該還有懸空的參照。
        let mut ws = a_finished_project();
        let redis = id_of(&mut ws, "服務", "redis");

        let out = ok(
            &mut ws,
            "delete",
            json!({"ids": [redis], "cascade": true, "confirm": true}),
        );
        assert!(out.contains("連帶刪掉"), "{out}");

        let left = lint(ws.project().unwrap());
        assert!(
            !left.iter().any(|f| f.rule.code() == "L012"),
            "還有懸空的參照：{left:?}"
        );
        assert!(
            ws.project()
                .unwrap()
                .logical
                .containers
                .iter()
                .all(|c| c.slug != "redis")
        );
    }

    #[test]
    fn a_cascade_delete_still_undoes_in_one_press() {
        // 掃掉三十個東西之後只能一個一個復原的話，Agent 一樣不敢用它。
        let mut ws = a_finished_project();
        let redis = id_of(&mut ws, "服務", "redis");
        let before = ws.project().unwrap().clone();

        ok(
            &mut ws,
            "delete",
            json!({"ids": [redis], "cascade": true, "confirm": true}),
        );
        assert!(ws.0.as_mut().unwrap().undo());
        assert_eq!(ws.project().unwrap(), &before, "一次復原沒有全部退回來");
    }

    #[test]
    fn without_cascade_the_preview_warns_about_what_will_dangle() {
        // 不連帶刪除仍然是預設，因為人在畫面上一個一個處理時那樣才對。
        // 但要講清楚代價，並且告訴它有 cascade 這個選項。
        let mut ws = a_finished_project();
        let redis = id_of(&mut ws, "服務", "redis");

        let out = ok(&mut ws, "delete", json!({"ids": [redis]}));

        assert!(out.contains("L012"), "沒說會留下懸空的參照：{out}");
        assert!(out.contains("cascade"), "沒提到有這個選項：{out}");
    }

    #[test]
    fn deleting_something_harmless_says_so_plainly() {
        let mut ws = a_finished_project();
        let redis_01 = id_of(&mut ws, "服務實體", "redis-01");

        // 三台裡的一台。連線用的是 redis-* 萬用字元，所以拿掉一台
        // 不會弄壞參照，只會讓 expect 對不上（L004）。
        let out = ok(&mut ws, "delete", json!({"ids": [redis_01]}));
        assert!(out.contains("服務實體 redis-01"), "{out}");
        assert!(out.contains("還沒有刪任何東西"), "{out}");
    }

    #[test]
    fn several_ids_go_in_one_call() {
        let mut ws = a_finished_project();
        let a = id_of(&mut ws, "服務實體", "redis-01");
        let b = id_of(&mut ws, "服務實體", "redis-02");

        let out = ok(&mut ws, "delete", json!({"ids": [a, b], "confirm": true}));
        assert!(out.contains("刪掉了 2 個"), "{out}");
        assert_eq!(ws.project().unwrap().environments[0].instances().len(), 2);
    }

    #[test]
    fn one_bad_id_stops_the_whole_delete() {
        let mut ws = a_finished_project();
        let good = id_of(&mut ws, "服務實體", "redis-01");
        let before = ws.project().unwrap().clone();

        let e = err(
            &mut ws,
            "delete",
            json!({"ids": [good, "根本沒這個"], "confirm": true}),
        );
        assert!(e.contains("找不到"), "{e}");
        assert!(e.contains("describe"), "沒告訴它怎麼查：{e}");
        assert_eq!(ws.project().unwrap(), &before, "壞掉的那一批卻刪了一半");
    }

    #[test]
    fn the_old_singular_id_shape_still_works() {
        let mut ws = a_finished_project();
        let redis_01 = id_of(&mut ws, "服務實體", "redis-01");
        let out = ok(&mut ws, "delete", json!({"id": redis_01, "confirm": true}));
        assert!(out.contains("刪掉了 1 個"), "{out}");
    }
}
