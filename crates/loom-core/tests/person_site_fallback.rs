//! 三個為了接住真實架構而加的東西：Person 端點、站點、備援路徑。
//!
//! 每一個都是拿一個實際系統建模時撞出來的（見 `docs/domain-model.md`）。
//! 這裡釘住它們的行為，特別是**加了之後有沒有讓 lint 誤報**——
//! 對一個以「找漏洞」為賣點的工具來說，誤報跟漏報一樣糟。

mod common;

use common::*;
use loom_core::environment::{
    Connection, ConnectionKind, DeploymentNode, Endpointing, InstanceRef, NodeKind,
};
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};
use loom_core::logical::{Person, Relationship, RelationshipEnd};
use loom_core::store::FileStore;
use loom_core::table::{SideKind, rows};

const USER: &str = "p-使用者";

/// 在健康的專案上加一條「使用者 → 訂單 API」，三個環境都接好。
fn project_with_a_person() -> loom_core::Project {
    let mut project = healthy_project();

    project.logical.people.push(Person {
        id: Id::new(USER),
        slug: "end-user".into(),
        name: "End User".into(),
    });
    project.logical.relationships.push(Relationship {
        id: Id::new("r-使用者"),
        slug: "使用者-連-api".into(),
        purpose: "使用者從瀏覽器下單".into(),
        from: RelationshipEnd::Person(Id::new(USER)),
        to: RelationshipEnd::Container(Id::new(API)),
        to_endpoint: Id::new(API_EGRESS),
    });

    for env in &mut project.environments {
        let slug = env.slug.clone();
        env.connections.push(Connection {
            id: Id::new(format!("conn-{slug}-使用者")),
            serves: Id::new("r-使用者"),
            purpose: "使用者連進來".into(),
            kind: ConnectionKind::Primary,
            from: Endpointing::Person {
                person: Id::new(USER),
            },
            to: Endpointing::Instance {
                target: InstanceRef::One(Id::new(format!("i-{slug}-api-01"))),
                endpoint: Some(Id::new(API_EGRESS)),
            },
        });
    }
    project
}

// ── A：Person 端點 ──────────────────────────────────────

#[test]
fn a_person_wired_into_the_system_draws_no_complaints() {
    // 加一個新的端點種類最容易犯的錯，是讓既有規則對它誤報。
    let found = lint(&project_with_a_person());
    assert!(found.is_empty(), "接了使用者卻報出問題：{found:?}");
}

#[test]
fn a_person_needs_no_deployment() {
    // L001 要求邏輯層的東西在每個環境都要實現。但人不是部署出來的——
    // 如果 L001 把 Person 也算進去，每個專案都會永遠紅著。
    let project = project_with_a_person();
    let person_complaints: Vec<_> = lint(&project)
        .into_iter()
        .filter(|f| f.subject == Id::new(USER))
        .collect();
    assert!(
        person_complaints.is_empty(),
        "不該要求人有落地：{person_complaints:?}"
    );
}

#[test]
fn a_missing_person_hop_is_caught() {
    // 反過來說，契約本身還是要在每個環境實現——只是實現的方式是
    // 「有一條從人出發的連線」，而不是「人要有落地」。
    let mut project = project_with_a_person();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new("r-使用者"));

    let found = lint(&project);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rule, Rule::L001);
    assert_eq!(found[0].subject, Id::new("r-使用者"));
}

#[test]
fn pointing_at_a_missing_person_is_an_error() {
    let mut project = project_with_a_person();
    project.environments[0].connections.last_mut().unwrap().from = Endpointing::Person {
        person: Id::new("p-不存在"),
    };

    let found = lint(&project);
    assert!(found.iter().any(|f| f.rule == Rule::L003), "{found:?}");
}

#[test]
fn the_connection_table_shows_a_person_by_name() {
    let project = project_with_a_person();
    let row = rows(&project)
        .into_iter()
        .find(|r| r.id == Id::new("conn-prod-使用者"))
        .unwrap();

    assert_eq!(row.from.kind, SideKind::Person);
    assert_eq!(row.from.label, "end-user");
    assert!(row.from.addresses.is_empty(), "人沒有位址");
}

// ── B：站點 ────────────────────────────────────────────

#[test]
fn a_site_holds_nodes_without_affecting_any_check() {
    // 把 prod 既有的機器全部塞進一個站點底下。
    // 巢狀是既有能力，這裡確認新的 NodeKind 不會讓它出問題。
    let mut project = project_with_a_person();
    let original = std::mem::take(&mut project.environments[0].nodes);
    project.environments[0].nodes = vec![DeploymentNode {
        id: Id::new("n-prod-dc"),
        slug: "dc-主中心".into(),
        kind: NodeKind::Site,
        children: original,
        instances: vec![],
    }];

    let found = lint(&project);
    assert!(found.is_empty(), "包進站點之後卻報錯：{found:?}");
}

#[test]
fn a_site_is_not_reported_as_an_unused_node() {
    // 站點不跑任何東西，所以它底下沒有 Instance。L008 是針對 Instance 的，
    // 不該因為「這個節點沒有連線碰到」就對站點發警告。
    let mut project = project_with_a_person();
    project.environments[0].nodes.push(DeploymentNode {
        id: Id::new("n-prod-空站點"),
        slug: "dc-還沒建的機房".into(),
        kind: NodeKind::Site,
        children: vec![],
        instances: vec![],
    });

    assert!(lint(&project).is_empty());
}

// ── E：備援路徑 ────────────────────────────────────────

#[test]
fn fallback_paths_are_checked_exactly_like_primary_ones() {
    // 標成 Fallback 不代表可以放寬——它一樣要被建立、防火牆一樣要開。
    // 如果 lint 對它比較寬鬆，那漏掉備援路徑就抓不到了，
    // 而備援路徑正是最常漏的東西。
    let mut project = project_with_a_person();

    let fallback = {
        let c = project.environments[0].connections[1].clone();
        Connection {
            id: Id::new("conn-prod-備援"),
            kind: ConnectionKind::Fallback,
            to: Endpointing::Instance {
                // 期望 5 台，實際只有 3 台——標成備援也一樣要被抓到。
                target: InstanceRef::Pattern {
                    slug_pattern: "redis-*".into(),
                    within: None,
                    expect: Some(5),
                },
                endpoint: Some(Id::new(REDIS_CLIENT)),
            },
            ..c
        }
    };
    project.environments[0].connections.push(fallback);

    let found = lint(&project);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, Rule::L004);
    assert_eq!(found[0].subject, Id::new("conn-prod-備援"));
}

#[test]
fn a_fallback_path_still_counts_towards_reachability() {
    // 只剩備援路徑時，路還是通的——它是真的能走，只是平常不走。
    let mut project = project_with_a_person();
    for c in &mut project.environments[2].connections {
        c.kind = ConnectionKind::Fallback;
    }

    assert!(
        !lint(&project).iter().any(|f| f.rule == Rule::L002),
        "全部標成備援之後被判定走不通"
    );
}

#[test]
fn the_kind_shows_up_in_the_connection_table() {
    let mut project = project_with_a_person();
    project.environments[0].connections[0].kind = ConnectionKind::Fallback;

    let all = rows(&project);
    let fallback_rows = all
        .iter()
        .filter(|r| r.kind == ConnectionKind::Fallback)
        .count();
    assert_eq!(fallback_rows, 1);
}

#[test]
fn a_connection_without_a_kind_defaults_to_primary() {
    // 既有的 YAML 沒有這個欄位，讀回來必須是 Primary，
    // 否則所有舊專案的連線都會突然變成備援。
    // 走 repository 的真實路徑，順便確認 YAML 佈局照舊。
    let mut store = loom_core::store::MemoryStore::new();
    loom_core::repository::save(&project_with_a_person(), &mut store).unwrap();

    // 把 kind 欄位從檔案裡整個拿掉，模擬舊專案。
    let path = "environments/prod.yaml";
    let original = store.read(path).unwrap();
    assert!(
        !original.contains("kind: primary"),
        "預設值不該被寫進 YAML，否則每個既有檔案都會多出一堆雜訊"
    );

    let loaded = loom_core::repository::load(&store).unwrap();
    assert!(
        loaded.environments[0]
            .connections
            .iter()
            .all(|c| c.kind == ConnectionKind::Primary),
        "沒寫 kind 的連線讀回來必須是正常路徑"
    );
}

// ── C：把萬用字元限定在某個站點底下 ─────────────────────

/// prod 改成兩個站點，Redis 各站三台。
fn two_site_project() -> loom_core::Project {
    let mut project = project_with_a_person();
    let prod = &mut project.environments[0];

    // 再加三台 Redis，湊成兩站各三台。
    for n in 4..=6 {
        prod.nodes.extend(vms(
            "prod",
            vec![instance(
                "prod",
                &format!("redis-0{n}"),
                REDIS,
                REDIS_CLIENT,
                &format!("10.0.2.1{n}:6379"),
            )],
        ));
    }

    // 前三台進主中心，後三台進異地；其餘機器留在主中心。
    let all = std::mem::take(&mut prod.nodes);
    let (dr_site_nodes, main_site_nodes): (Vec<_>, Vec<_>) = all.into_iter().partition(|n| {
        matches!(
            n.slug.as_str(),
            "vm-redis-04" | "vm-redis-05" | "vm-redis-06"
        )
    });

    prod.nodes = vec![
        DeploymentNode {
            id: Id::new("n-主中心"),
            slug: "dc-主中心".into(),
            kind: NodeKind::Site,
            children: main_site_nodes,
            instances: vec![],
        },
        DeploymentNode {
            id: Id::new("n-異地"),
            slug: "dc-異地".into(),
            kind: NodeKind::Site,
            children: dr_site_nodes,
            instances: vec![],
        },
    ];

    // 快取那條改成兩條：每站各三台。
    prod.connections[1].to = Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: "redis-*".into(),
            within: Some(Id::new("n-主中心")),
            expect: Some(3),
        },
        endpoint: Some(Id::new(REDIS_CLIENT)),
    };
    let mut dr_connection = prod.connections[1].clone();
    dr_connection.id = Id::new("conn-prod-redis-異地");
    dr_connection.to = Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: "redis-*".into(),
            within: Some(Id::new("n-異地")),
            expect: Some(3),
        },
        endpoint: Some(Id::new(REDIS_CLIENT)),
    };
    prod.connections.push(dr_connection);

    project
}

#[test]
fn three_at_each_site_is_healthy() {
    let found = lint(&two_site_project());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_node_moved_between_sites_is_caught() {
    // 這是 L-C 的整個重點。
    //
    // 寫成 `redis-* expect 6` 的話，搬一台過去總數還是 6，lint 完全不會叫——
    // 而「東西還在但位置錯了」正是最難用眼睛發現的那種漏。
    let mut project = two_site_project();

    let prod = &mut project.environments[0];
    let moved_away = {
        let main_site = prod
            .nodes
            .iter_mut()
            .find(|n| n.slug == "dc-主中心")
            .unwrap();
        let i = main_site
            .children
            .iter()
            .position(|n| n.slug == "vm-redis-03")
            .unwrap();
        main_site.children.remove(i)
    };
    prod.nodes
        .iter_mut()
        .find(|n| n.slug == "dc-異地")
        .unwrap()
        .children
        .push(moved_away);

    let found = lint(&project);
    let wrong_count: Vec<_> = found.iter().filter(|f| f.rule == Rule::L004).collect();
    assert_eq!(
        wrong_count.len(),
        2,
        "主中心少一台、異地多一台，兩邊都該叫：{found:?}"
    );
    assert!(wrong_count.iter().all(|f| f.detail.contains("限定在")));
}

#[test]
fn without_a_scope_a_move_goes_unnoticed() {
    // 反過來證明上一條測的是真的東西：不用 within 就抓不到。
    // 這也是為什麼 within 值得加。
    let mut project = two_site_project();

    let prod = &mut project.environments[0];
    // 改回「全部六台」的寫法。
    prod.connections
        .retain(|c| c.id != Id::new("conn-prod-redis-異地"));
    prod.connections[1].to = Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: "redis-*".into(),
            within: None,
            expect: Some(6),
        },
        endpoint: Some(Id::new(REDIS_CLIENT)),
    };

    // 一樣把一台搬到異地。
    let moved_away = {
        let main_site = prod
            .nodes
            .iter_mut()
            .find(|n| n.slug == "dc-主中心")
            .unwrap();
        let i = main_site
            .children
            .iter()
            .position(|n| n.slug == "vm-redis-03")
            .unwrap();
        main_site.children.remove(i)
    };
    prod.nodes
        .iter_mut()
        .find(|n| n.slug == "dc-異地")
        .unwrap()
        .children
        .push(moved_away);

    assert!(
        lint(&project).is_empty(),
        "沒有 within 的話這種搬動本來就抓不到——這正是加它的理由"
    );
}

#[test]
fn within_pointing_at_a_missing_node_is_an_error() {
    // 打錯節點 id 會讓 expect 永遠是 0，看起來像「一台都沒建」。
    // 如果不特別報出來，使用者會去找根本不存在的問題。
    let mut project = two_site_project();
    project.environments[0].connections[1].to = Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: "redis-*".into(),
            within: Some(Id::new("n-打錯的節點")),
            expect: Some(3),
        },
        endpoint: Some(Id::new(REDIS_CLIENT)),
    };

    let found = lint(&project);
    assert!(
        found
            .iter()
            .any(|f| f.rule == Rule::L003 && f.detail.contains("within")),
        "{found:?}"
    );
}

#[test]
fn the_scope_shows_up_in_the_connection_table() {
    // 只寫「3 台」的話，讀的人會以為全部只有 3 台。
    let project = two_site_project();
    let row = rows(&project)
        .into_iter()
        .find(|r| r.id == Id::new("conn-prod-2"))
        .unwrap();
    assert_eq!(row.to.label, "redis-* @ dc-主中心");
}
