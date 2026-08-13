//! 模擬一個 Agent 從零建出一份專案。
//!
//! # 這份測試在驗什麼
//!
//! 不是「工具會不會回 200」，是**一個看不到畫面、只能讀文字的東西，
//! 有沒有辦法靠這些工具把事情做完**。
//!
//! 所以每一步都只用工具的回應決定下一步，而且刻意犯幾個 Agent 真的會犯的錯
//! （名字打錯、順序顛倒、忘記接點），確認錯誤訊息**告訴得了它怎麼修**。
//!
//! 「找不到 redis」這種錯誤等於沒有——它只會再猜一次。

mod common;

use serde_json::json;

use common::*;
use loom_core::lint::lint;
use loom_mcp::{Workspace, tools};

#[test]
fn an_agent_can_build_a_clean_project_from_prose() {
    // 假設 Agent 手上的原始資料是：
    //   「訂單系統。前面一台 Apache 反向代理，後面三台 Redis 做快取。
    //     prod 環境。使用者從外面連進 Apache。」
    let mut ws = empty();

    // ① 先看看有什麼——這是 skill 教它的第一步。
    let before = ok(&mut ws, "describe", json!({}));
    assert!(before.contains("環境：一個都還沒有"));

    // ② 邏輯層：系統 → 服務 → 接點定義 → 契約
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單系統", "external": false}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "apache", "name": "反向代理", "system": "shop"}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "redis", "name": "快取", "system": "shop"}),
    );
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "apache", "slug": "http", "protocol": "tcp"}),
    );
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "redis", "slug": "client-port", "protocol": "tcp"}),
    );
    create(
        &mut ws,
        "person",
        json!({"slug": "customer", "name": "使用者"}),
    );

    create(
        &mut ws,
        "relationship",
        json!({
            "slug": "user-to-apache", "purpose": "使用者連進系統",
            "from": "person:customer", "to": "container:apache", "to_endpoint": "http"
        }),
    );
    create(
        &mut ws,
        "relationship",
        json!({
            "slug": "apache-to-redis", "purpose": "讀寫快取",
            "from": "container:apache", "to": "container:redis", "to_endpoint": "client-port"
        }),
    );

    // 到這裡還沒有環境，所以 lint 應該是安靜的——邏輯層自己不會缺什麼。
    assert_eq!(ok(&mut ws, "lint", json!({})), "沒有發現任何缺漏。");

    // ③ 環境。建了之後 lint 才開始有話說。
    create(
        &mut ws,
        "environment",
        json!({"slug": "prod", "name": "正式環境"}),
    );
    let complaints = ok(&mut ws, "lint", json!({}));
    assert!(
        complaints.contains("L001"),
        "建了環境卻沒說缺什麼：{complaints}"
    );

    // ④ 照 lint 說的建機器。
    ok(
        &mut ws,
        "create_nodes",
        json!({
            "environment": "prod", "container": "apache", "count": 1,
            "address_template": "10.0.0.{ip}:8080", "ip_start": 11
        }),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({
            "environment": "prod", "container": "redis", "count": 3,
            "address_template": "10.0.1.{ip}:6379", "ip_start": 11
        }),
    );

    // ⑤ 補連線。兩條都用提案（都是直達）。
    ok(
        &mut ws,
        "add_connection",
        json!({"environment": "prod", "relationship": "user-to-apache"}),
    );
    ok(
        &mut ws,
        "add_connection",
        json!({"environment": "prod", "relationship": "apache-to-redis"}),
    );

    // ⑥ 收工的時候 lint 要是乾淨的。
    assert_eq!(
        ok(&mut ws, "lint", json!({})),
        "沒有發現任何缺漏。",
        "Agent 走完整套流程卻沒把專案建乾淨"
    );
    assert!(lint(ws.project().unwrap()).is_empty());

    // ⑦ 三台 Redis 要收成萬用字元，不是三條連線——
    //    列出每一台的話，之後每加一台機器都要記得補一條。
    let env = &ws.project().unwrap().environments[0];
    assert_eq!(env.connections.len(), 2, "應該是兩段，不是每台一條");
    assert_eq!(env.instances().len(), 4);
}

#[test]
fn a_hop_through_an_f5_is_two_segments() {
    // 這是 Agent 最容易做錯的一件事：把經過 F5 的流量寫成一條。
    // 寫成一條的話 lint 不會叫（每一條單看都合法），但圖上就少了 F5。
    let mut ws = empty();
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單系統", "external": false}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "apache", "name": "RP", "system": "shop"}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "gateway", "name": "Gateway", "system": "shop"}),
    );
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "gateway", "slug": "http", "protocol": "tcp"}),
    );
    create(
        &mut ws,
        "relationship",
        json!({
            "slug": "apache-to-gateway", "purpose": "轉送",
            "from": "container:apache", "to": "container:gateway", "to_endpoint": "http"
        }),
    );
    create(
        &mut ws,
        "environment",
        json!({"slug": "prod", "name": "正式"}),
    );
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "apache", "slug": "http", "protocol": "tcp"}),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({
            "environment": "prod", "container": "apache", "count": 3,
            "address_template": "10.0.0.{ip}:8080", "ip_start": 11
        }),
    );
    ok(
        &mut ws,
        "create_nodes",
        json!({
            "environment": "prod", "container": "gateway", "count": 3,
            "address_template": "10.0.1.{ip}:8080", "ip_start": 11
        }),
    );
    create(
        &mut ws,
        "infra",
        json!({"environment": "prod", "slug": "f5-01"}),
    );
    create(
        &mut ws,
        "infra_endpoint",
        json!({
            "environment": "prod", "owner": "f5-01", "slug": "vip-gateway", "address": "10.0.0.100:8080"
        }),
    );

    // 只建第一段 → lint 要說走不通。
    ok(
        &mut ws,
        "add_connection",
        json!({
            "environment": "prod", "relationship": "apache-to-gateway",
            "from": "apache-*", "to": "f5-01 : vip-gateway"
        }),
    );
    let 走不通 = ok(&mut ws, "lint", json!({}));
    assert!(走不通.contains("L002"), "少了第二段卻沒說走不通：{走不通}");
    assert!(
        走不通.contains("補上缺的那一段"),
        "沒告訴 Agent 怎麼修：{走不通}"
    );

    // 補上第二段 → 通了。
    ok(
        &mut ws,
        "add_connection",
        json!({
            "environment": "prod", "relationship": "apache-to-gateway",
            "from": "f5-01", "to": "gateway-*"
        }),
    );
    assert_eq!(ok(&mut ws, "lint", json!({})), "沒有發現任何缺漏。");
}

mod errors_have_to_teach {
    use super::*;

    #[test]
    fn a_wrong_name_lists_the_real_ones() {
        // 只說「找不到」的話 Agent 只會再猜一次，而且會猜一個很像的。
        let mut ws = empty();
        create(
            &mut ws,
            "system",
            json!({"slug": "shop", "name": "訂單", "external": false}),
        );

        let e = err(
            &mut ws,
            "create",
            json!({
                "kind": "container", "fields": {"slug": "redis", "name": "快取", "system": "shopp"}
            }),
        );
        assert!(e.contains("找不到系統 shopp"), "{e}");
        assert!(e.contains("shop"), "沒有列出真正有的：{e}");
    }

    #[test]
    fn a_missing_endpoint_says_which_ones_exist() {
        let mut ws = empty();
        create(
            &mut ws,
            "system",
            json!({"slug": "shop", "name": "訂單", "external": false}),
        );
        create(
            &mut ws,
            "container",
            json!({"slug": "redis", "name": "快取", "system": "shop"}),
        );
        create(
            &mut ws,
            "endpoint_def",
            json!({"owner": "redis", "slug": "client-port", "protocol": "tcp"}),
        );

        let e = err(
            &mut ws,
            "create",
            json!({
                "kind": "relationship", "fields": {
                    "slug": "x", "purpose": "y",
                    "from": "container:redis", "to": "container:redis", "to_endpoint": "clientport"
                }
            }),
        );
        assert!(e.contains("client-port"), "沒有列出真正有的接點：{e}");
    }

    #[test]
    fn a_service_with_no_endpoint_says_to_create_one_first() {
        // Agent 很容易跳過接點定義直接建機器。
        let mut ws = empty();
        create(
            &mut ws,
            "system",
            json!({"slug": "shop", "name": "訂單", "external": false}),
        );
        create(
            &mut ws,
            "container",
            json!({"slug": "redis", "name": "快取", "system": "shop"}),
        );
        create(
            &mut ws,
            "environment",
            json!({"slug": "prod", "name": "正式"}),
        );

        let e = err(
            &mut ws,
            "create_nodes",
            json!({
                "environment": "prod", "container": "redis", "count": 3,
                "address_template": "10.0.1.{ip}:6379"
            }),
        );
        assert!(
            e.contains("先 create endpoint_def"),
            "沒說下一步該做什麼：{e}"
        );
    }

    #[test]
    fn several_endpoints_force_a_choice_instead_of_guessing() {
        let mut ws = empty();
        create(
            &mut ws,
            "system",
            json!({"slug": "shop", "name": "訂單", "external": false}),
        );
        create(
            &mut ws,
            "container",
            json!({"slug": "redis", "name": "快取", "system": "shop"}),
        );
        create(
            &mut ws,
            "endpoint_def",
            json!({"owner": "redis", "slug": "client-port", "protocol": "tcp"}),
        );
        create(
            &mut ws,
            "endpoint_def",
            json!({"owner": "redis", "slug": "cluster-bus", "protocol": "tcp"}),
        );
        create(
            &mut ws,
            "environment",
            json!({"slug": "prod", "name": "正式"}),
        );

        let e = err(
            &mut ws,
            "create_nodes",
            json!({
                "environment": "prod", "container": "redis", "count": 3,
                "address_template": "10.0.1.{ip}:6379"
            }),
        );
        assert!(e.contains("要指定哪一個"), "{e}");
        assert!(
            e.contains("cluster-bus") && e.contains("client-port"),
            "{e}"
        );
    }

    #[test]
    fn a_duplicate_slug_is_rejected_by_the_same_rule_as_the_app() {
        // Agent 讀同一段文字兩次是常有的事。規則只有一套，所以它會被擋下來。
        let mut ws = empty();
        create(
            &mut ws,
            "system",
            json!({"slug": "shop", "name": "訂單", "external": false}),
        );
        let e = err(
            &mut ws,
            "create",
            json!({
                "kind": "system", "fields": {"slug": "shop", "name": "又一次", "external": false}
            }),
        );
        assert!(e.contains("已經有一個叫 shop"), "{e}");
    }

    #[test]
    fn no_project_open_says_so_in_plain_words() {
        let mut ws = Desk(None);
        let e = err(&mut ws, "describe", json!({}));
        assert!(e.contains("還沒有開啟任何專案"), "{e}");
    }
}

#[test]
fn every_change_reports_what_it_broke() {
    // 「建好了」對 Agent 沒有資訊量。多冒出問題要當下就講，
    // 不然它會一路做到最後才發現前面都錯了。
    let mut ws = empty();
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單", "external": false}),
    );
    let out = create(
        &mut ws,
        "container",
        json!({"slug": "redis", "name": "快取", "system": "shop"}),
    );
    assert!(out.contains("沒有多出任何問題"), "{out}");

    create(
        &mut ws,
        "environment",
        json!({"slug": "prod", "name": "正式"}),
    );
    // 建了環境之後，那個沒有實體的服務就變成一個問題了。
    let out = create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "Consul", "system": "shop"}),
    );
    assert!(
        out.contains("但多出"),
        "建了一個沒有實體的服務卻說沒事：{out}"
    );
    assert!(out.contains("L001"), "{out}");
}

#[test]
fn nothing_is_written_to_disk() {
    // 這是刻意的：Agent 會弄錯，而這個工具的賣點就是「怕漏」。
    // 讓它直接寫進磁碟等於把最後一道人工檢查拿掉。
    let mut ws = empty();
    let out = create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單", "external": false}),
    );
    assert!(out.contains("尚未存檔"), "沒有提醒還沒存檔：{out}");
    assert!(ws.0.as_ref().unwrap().is_dirty());
}

#[test]
fn the_user_can_undo_everything_the_agent_did() {
    // Agent 的每一次修改都走同一條 History，所以 ⌘Z 退得掉。
    let mut ws = empty();
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單", "external": false}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "redis", "name": "快取", "system": "shop"}),
    );

    let h = ws.0.as_mut().unwrap();
    assert_eq!(h.undo_label(), Some("新增服務"));
    assert!(h.undo());
    assert!(h.undo());
    assert!(h.project().logical.systems.is_empty());
    assert!(!h.is_dirty(), "全部退回去之後應該回到未修改的狀態");
}

mod what_the_agent_is_told {
    use super::*;

    #[test]
    fn the_instructions_carry_the_four_house_rules() {
        // 這段是 Agent 唯一會無條件讀到的說明——工具的描述要它主動看，
        // 這段不用。所以最重要的四件事一定要在裡面。
        let text = loom_mcp::INSTRUCTIONS;
        assert!(text.contains("describe"), "沒叫它先看現況");
        assert!(text.contains("lint"), "沒叫它自我檢查");
        assert!(text.contains("不會自動存檔"), "沒說清楚存檔是人的決定");
        // 這一條特別容易漏掉，因為一個一個送**每次都會成功**——
        // 沒有任何錯誤訊息會提醒它這樣很慢。
        assert!(text.contains("一次送一批"), "沒叫它批次送");
    }

    #[test]
    fn every_tool_has_a_description_and_a_schema() {
        // 描述文字是 Agent 唯一的說明書。少寫一個它就會用錯，
        // 而用錯的代價是使用者的專案被弄髒。
        for t in tools::list().as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            assert!(!name.is_empty());
            assert!(
                t["description"].as_str().unwrap().len() > 40,
                "{name} 的說明太短，Agent 看不懂該怎麼用"
            );
            assert_eq!(t["inputSchema"]["type"], "object", "{name}");
        }
    }

    #[test]
    fn an_unknown_tool_name_is_an_error_not_a_panic() {
        // 協定由 rmcp 負責，但「沒有這個工具」還是要由這裡回答——
        // rmcp 不知道我們有哪些工具。
        let mut ws = empty();
        assert!(err(&mut ws, "刪掉全部", json!({})).contains("沒有這個工具"));
    }
}
