//! 一台機器上跑好幾個服務。
//!
//! # 為什麼這一組測試存在
//!
//! 這是真的用起來才發現的：Agent 回報「沒辦法讓多個服務綁到同一台機器上」。
//! 它說得對——在這之前，MCP 建服務實體只有 `create_nodes` 一條路，而它
//! **一定會替每個服務實體配一台新機器**（`node_template`）。
//!
//! 於是 Agent 建得出來的模型，跟使用者的機房長得不一樣：一台 VM 上跑著
//! app 和 log-agent 是常態，而工具只表達得出「兩台機器各跑一個」。
//!
//! 畫面上一直做得到（表單裡有「跑在哪台機器上」）。**做不到的只有 Agent，
//! 而那是最看不出來的一種缺口**——它不會報錯，只會安靜地建出另一種架構。

mod common;

use common::*;
use serde_json::json;

/// prod 環境、一個服務、一台機器。
fn one_machine() -> Desk {
    let mut ws = empty();
    create(
        &mut ws,
        "system",
        json!({"slug": "shop", "name": "訂單系統"}),
    );
    create(
        &mut ws,
        "container",
        json!({"slug": "app", "name": "訂單服務", "system": "shop"}),
    );
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "app", "slug": "http", "protocol": "tcp"}),
    );
    create(
        &mut ws,
        "environment",
        json!({"slug": "prod", "name": "正式"}),
    );
    create(
        &mut ws,
        "node",
        json!({"environment": "prod", "slug": "vm-01", "kind": "virtual-machine"}),
    );
    ws
}

/// 再加一個服務，好讓兩個服務可以擠在同一台機器上。
fn add_agent(ws: &mut Desk) {
    create(
        ws,
        "container",
        json!({"slug": "log-agent", "name": "日誌收集", "system": "shop"}),
    );
    create(
        ws,
        "endpoint_def",
        json!({"owner": "log-agent", "slug": "http", "protocol": "tcp"}),
    );
}

#[test]
fn two_services_can_live_on_the_same_machine() {
    let mut ws = one_machine();
    add_agent(&mut ws);

    create(
        &mut ws,
        "instance",
        json!({"environment": "prod", "node": "vm-01", "slug": "app-01",
               "container": "app", "address": "10.0.1.11:8080"}),
    );
    create(
        &mut ws,
        "instance",
        json!({"environment": "prod", "node": "vm-01", "slug": "agent-01",
               "container": "log-agent", "address": "10.0.1.11:9100"}),
    );

    let env = &ws.0.as_ref().unwrap().project().environments[0];
    assert_eq!(env.nodes.len(), 1, "不該長出第二台機器");
    let slugs: Vec<&str> = env.nodes[0]
        .instances
        .iter()
        .map(|i| i.slug.as_str())
        .collect();
    assert_eq!(slugs, ["app-01", "agent-01"]);
}

#[test]
fn the_address_lands_on_the_endpoint_of_that_service() {
    // 位址住在 Endpoint 上，不是實體上。建完卻沒地方填位址的話，
    // lint 會立刻叫 L006，而 Agent 手上沒有工具解得掉。
    let mut ws = one_machine();
    create(
        &mut ws,
        "instance",
        json!({"environment": "prod", "node": "vm-01", "slug": "app-01",
               "container": "app", "address": "10.0.1.11:8080"}),
    );

    let instance = &ws.0.as_ref().unwrap().project().environments[0].nodes[0].instances[0];
    assert_eq!(instance.endpoints.len(), 1);
    assert_eq!(
        instance.endpoints[0].address.as_deref(),
        Some("10.0.1.11:8080")
    );
}

#[test]
fn several_endpoint_defs_force_a_choice_instead_of_guessing() {
    // 猜錯的話位址會落在錯的接點上，症狀是「明明填了位址卻還是說缺位址」。
    let mut ws = one_machine();
    create(
        &mut ws,
        "endpoint_def",
        json!({"owner": "app", "slug": "grpc", "protocol": "tcp"}),
    );

    let complaint = err(
        &mut ws,
        "create",
        json!({"kind": "instance", "fields": {
            "environment": "prod", "node": "vm-01", "slug": "app-01",
            "container": "app", "address": "10.0.1.11:8080"}}),
    );

    assert!(complaint.contains("endpoint"), "沒說怎麼指定：{complaint}");
    assert!(complaint.contains("grpc"), "沒列出候選：{complaint}");
}

#[test]
fn a_machine_that_does_not_exist_lists_the_ones_that_do() {
    // 打錯一個字，跟「那台機器還沒建」是兩件事，而 Agent 分不出來
    // 就會走上完全不同的一條路。
    let mut ws = one_machine();

    let complaint = err(
        &mut ws,
        "create",
        json!({"kind": "instance", "fields": {
            "environment": "prod", "node": "vm-1", "slug": "app-01",
            "container": "app", "address": "10.0.1.11:8080"}}),
    );

    assert!(complaint.contains("vm-01"), "沒列出真的有的：{complaint}");
}

#[test]
fn forgetting_the_machine_says_so_before_anything_is_built() {
    let mut ws = one_machine();

    let complaint = err(
        &mut ws,
        "create",
        json!({"kind": "instance", "fields": {
            "environment": "prod", "slug": "app-01", "container": "app"}}),
    );

    assert!(complaint.contains("node"), "沒點名是哪個欄位：{complaint}");
}

#[test]
fn the_same_name_twice_in_one_environment_is_rejected() {
    // 萬用字元比對的就是這個名字，兩台同名會讓 expect 數錯。
    let mut ws = one_machine();
    add_agent(&mut ws);
    create(
        &mut ws,
        "instance",
        json!({"environment": "prod", "node": "vm-01", "slug": "app-01",
               "container": "app", "address": "10.0.1.11:8080"}),
    );

    let complaint = err(
        &mut ws,
        "create",
        json!({"kind": "instance", "fields": {
            "environment": "prod", "node": "vm-01", "slug": "app-01",
            "container": "log-agent", "address": "10.0.1.11:9100"}}),
    );

    assert!(complaint.contains("app-01"), "{complaint}");
}

mod moving_house {
    use super::*;

    /// 兩台機器，服務實體先待在第一台。
    fn ready() -> (Desk, String) {
        let mut ws = one_machine();
        create(
            &mut ws,
            "node",
            json!({"environment": "prod", "slug": "vm-02", "kind": "virtual-machine"}),
        );
        create(
            &mut ws,
            "instance",
            json!({"environment": "prod", "node": "vm-01", "slug": "app-01",
                   "container": "app", "address": "10.0.1.11:8080"}),
        );
        let id = id_of(&mut ws, "服務實體", "app-01");
        (ws, id)
    }

    #[test]
    fn a_service_can_be_moved_to_another_machine() {
        let (mut ws, id) = ready();

        ok(
            &mut ws,
            "update",
            json!({"id": id, "fields": {"node": "vm-02"}}),
        );

        let nodes = &ws.0.as_ref().unwrap().project().environments[0].nodes;
        let on: Vec<(&str, usize)> = nodes
            .iter()
            .map(|n| (n.slug.as_str(), n.instances.len()))
            .collect();
        assert_eq!(on, [("vm-01", 0), ("vm-02", 1)]);
    }

    #[test]
    fn moving_does_not_leave_a_copy_behind() {
        // 留一份的話萬用字元會把它數成兩台，而畫面上兩台都長得像真的。
        let (mut ws, id) = ready();

        ok(
            &mut ws,
            "update",
            json!({"id": id, "fields": {"node": "vm-02"}}),
        );

        let env = &ws.0.as_ref().unwrap().project().environments[0];
        assert_eq!(env.instances().len(), 1);
    }

    #[test]
    fn the_address_survives_the_move() {
        // 搬家只換機器。位址跟著實體走——`write_into` 整份帶著走，
        // 不是逐欄複製（那個坑修過一次了）。
        let (mut ws, id) = ready();

        ok(
            &mut ws,
            "update",
            json!({"id": id, "fields": {"node": "vm-02"}}),
        );

        let env = &ws.0.as_ref().unwrap().project().environments[0];
        let moved = &env.instances()[0];
        assert_eq!(
            moved.endpoints.first().and_then(|e| e.address.as_deref()),
            Some("10.0.1.11:8080")
        );
    }

    #[test]
    fn changing_the_environment_is_refused_not_ignored() {
        // 這是這一輪真正的紀律：`environment` 由 create 消化，`fill` 沒看它。
        // 不擋的話 update 會回一句「改好了」而東西沒動——Agent 會照著往下走。
        let (mut ws, id) = ready();
        create(
            &mut ws,
            "environment",
            json!({"slug": "uat", "name": "驗收"}),
        );

        let complaint = err(
            &mut ws,
            "update",
            json!({"id": id, "fields": {"environment": "uat"}}),
        );

        assert!(complaint.contains("environment"), "{complaint}");
        assert!(complaint.contains("delete"), "沒講怎麼做：{complaint}");
    }

    #[test]
    fn moving_a_machine_into_a_site_is_refused_the_same_way() {
        // 同一個家族：`within` 也是只有 create 讀得到。
        let mut ws = one_machine();
        create(
            &mut ws,
            "node",
            json!({"environment": "prod", "slug": "機房甲", "kind": "site"}),
        );
        let id = id_of(&mut ws, "機器", "vm-01");

        let complaint = err(
            &mut ws,
            "update",
            json!({"id": id, "fields": {"within": "機房甲"}}),
        );

        assert!(complaint.contains("within"), "{complaint}");
    }
}
