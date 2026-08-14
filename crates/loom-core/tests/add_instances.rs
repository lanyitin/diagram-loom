//! 批次建立機器：L001「這個服務一台都還沒建」的修法。
//!
//! # 為什麼這是必要的，不是方便
//!
//! 補連線的提案會說「目標的服務 redis 在 dev 一台都還沒建，要先建機器」。
//! 若建不了機器，那句話就是死路——工具叫你去做一件它不讓你做的事。
//!
//! 而且既然 App 是真相來源（見 `docs/decisions.md`），
//! 「新增六台 Redis」是日常操作，不該逼人回去改 Excel。

mod common;

use common::*;
use loom_core::batch::{BatchSpec, EndpointPlan};
use loom_core::edit::{self, Edit, Fix};
use loom_core::environment::NodeKind;
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};
use loom_core::logical::Protocol;

fn spec(count: u32) -> BatchSpec {
    BatchSpec {
        count,
        name_template: "redis-{n}".into(),
        node_template: "vm-redis-{n}".into(),
        start: 1,
        pad: 2,
        address_template: "10.9.9.{ip}:6379".into(),
        ip_start: 51,
        node_kind: NodeKind::VirtualMachine,
        container: Id::new(REDIS),
        endpoint: EndpointPlan {
            def: Id::new(REDIS_CLIENT),
            slug: "client-port".into(),
            protocol: Protocol::Tcp,
        },
    }
}

/// dev 環境把 Redis 整個拿掉——L001「服務一台都沒建」。
fn project_without_redis() -> loom_core::Project {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes.retain(|n| n.slug != "vm-redis-01");
    dev.connections.retain(|c| c.serves != Id::new(REL_CACHE));
    project
}

#[test]
fn l001_container_with_no_nodes_is_fixed_by_batch_creation() {
    let mut project = project_without_redis();
    assert!(
        lint(&project)
            .iter()
            .any(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS)),
        "素材應該要有「服務沒有實體」的 L001"
    );

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(2)).unwrap();

    edit::apply(
        &mut project,
        &Edit::AddInstances {
            environment: env.id.clone(),
            within: None,
            nodes: plan.nodes.clone(),
        },
    )
    .unwrap();

    // 機器建好了，服務實體的那條 L001 就消失了。
    // 剩下的是「還沒接線」——那是補連線的事，另一個修法。
    let remaining: Vec<Rule> = lint(&project).iter().map(|f| f.rule).collect();
    assert!(
        !lint(&project)
            .iter()
            .any(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS)),
        "建了機器 L001 卻還在：{remaining:?}"
    );
}

#[test]
fn create_nodes_then_add_the_connection_and_the_environment_is_clean() {
    // 這才是完整的一圈：L001 說沒機器 → 建機器 → L001 說沒連線 → 補連線 → 乾淨。
    // 兩個修法各自只走一半，串起來才等於「使用者真的把問題解決了」。
    let mut project = project_without_redis();
    let env_id = project.environments[2].id.clone();

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(2)).unwrap();
    edit::apply(
        &mut project,
        &Edit::AddInstances {
            environment: env_id.clone(),
            within: None,
            nodes: plan.nodes,
        },
    )
    .unwrap();

    let env = project.environments[2].clone();
    let p = loom_core::connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();
    assert!(p.is_complete(), "建了機器卻還是擬不出連線：{:?}", p.notes);

    edit::apply(
        &mut project,
        &Edit::AddConnection {
            environment: env_id,
            id: p.id,
            serves: p.serves,
            purpose: p.purpose,
            kind: loom_core::environment::ConnectionKind::Primary,
            from: p.from.unwrap(),
            to: p.to.unwrap(),
        },
    )
    .unwrap();

    assert_eq!(
        lint(&project).iter().map(|f| f.rule).collect::<Vec<_>>(),
        Vec::<Rule>::new()
    );
}

#[test]
fn a_slug_clash_rejects_the_whole_batch() {
    // 同一個環境有兩台 redis-01 會讓萬用字元數到 2，
    // 而使用者以為那是兩台不同的機器——正好是這個工具要防的誤會。
    let project = healthy_project(); // dev 已經有一台 redis-01
    let env = &project.environments[2];

    // 機器名先撞到（dev 那台叫 vm-redis-01），服務實體名也會撞，
    // 兩者都足以擋下整批。
    let err = loom_core::batch::plan(&project, env, &spec(3)).unwrap_err();
    assert_eq!(
        err,
        loom_core::batch::BatchError::Taken("vm-redis-01".into())
    );
}

#[test]
fn a_missing_container_is_rejected() {
    let project = healthy_project();
    let env = &project.environments[2];
    let mut spec = spec(2);
    spec.container = Id::new("c-根本沒這個服務");

    assert!(loom_core::batch::plan(&project, env, &spec).is_err());
}

#[test]
fn an_endpoint_from_another_container_is_rejected() {
    // 拿 API 的接點去建 Redis，lint 之後會變成一個很難懂的 L003。
    // 在源頭擋掉比較好解釋。
    let project = healthy_project();
    let env = &project.environments[2];
    let mut spec = spec(2);
    spec.endpoint.def = Id::new(API_EGRESS);

    assert!(loom_core::batch::plan(&project, env, &spec).is_err());
}

#[test]
fn the_preview_spells_out_every_node() {
    // 一次建六台是會後悔的操作，套用前一定要看得到清單。
    let project = project_without_redis();
    let env = &project.environments[2];
    let plan = loom_core::batch::plan(&project, env, &spec(3)).unwrap();

    assert_eq!(
        plan.preview,
        vec![
            "vm-redis-01 / redis-01 @ 10.9.9.51:6379",
            "vm-redis-02 / redis-02 @ 10.9.9.52:6379",
            "vm-redis-03 / redis-03 @ 10.9.9.53:6379",
        ]
    );
}

#[test]
fn can_be_created_under_a_site() {
    let mut project = project_without_redis();
    // 先給 dev 一個站點。
    let site = loom_core::environment::DeploymentNode {
        id: Id::new("n-dev-機房"),
        slug: "dc-main".into(),
        kind: NodeKind::Site,
        children: vec![],
        instances: vec![],
        memo: String::new(),
    };
    project.environments[2].nodes.push(site);

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(2)).unwrap();
    edit::apply(
        &mut project,
        &Edit::AddInstances {
            environment: env.id.clone(),
            within: Some(Id::new("n-dev-機房")),
            nodes: plan.nodes,
        },
    )
    .unwrap();

    let site = project.environments[2]
        .nodes
        .iter()
        .find(|n| n.slug == "dc-main")
        .unwrap();
    assert_eq!(site.children.len(), 2);
    // 巢狀底下的服務實體一樣算得到，萬用字元才數得對。
    assert_eq!(
        project.environments[2].instances_matching("redis-*").len(),
        2
    );
}

#[test]
fn creating_under_a_missing_node_errors() {
    let mut project = project_without_redis();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(2)).unwrap();

    assert!(
        edit::apply(
            &mut project,
            &Edit::AddInstances {
                environment: env.id.clone(),
                within: Some(Id::new("n-沒這個節點")),
                nodes: plan.nodes,
            },
        )
        .is_err()
    );
}

#[test]
fn the_same_batch_cannot_be_created_twice() {
    let mut project = project_without_redis();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(2)).unwrap();

    let this_batch = Edit::AddInstances {
        environment: env.id.clone(),
        within: None,
        nodes: plan.nodes,
    };
    edit::apply(&mut project, &this_batch).unwrap();
    assert!(
        edit::apply(&mut project, &this_batch).is_err(),
        "同一批竟然建得進去第二次"
    );
}

#[test]
fn the_whole_batch_can_be_undone() {
    use loom_core::history::History;

    let project = project_without_redis();
    let original = project.clone();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &spec(6)).unwrap();

    let mut h = History::opened(project);
    h.edit(&Edit::AddInstances {
        environment: env.id.clone(),
        within: None,
        nodes: plan.nodes,
    })
    .unwrap();
    assert_eq!(
        h.project().environments[2]
            .instances_matching("redis-*")
            .len(),
        6
    );

    assert!(h.undo());
    assert_eq!(h.project(), &original, "復原之後沒有回到原狀");
    assert_eq!(h.redo_label(), Some("批次建立機器"));
}

#[test]
fn l001_on_a_container_offers_creating_nodes() {
    // 同樣是 L001，報在契約上要補連線、報在服務上要建機器。
    // 這個分辨是規則，不是畫面——所以由 `fix_for` 決定。
    let project = project_without_redis();

    let container_findings = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS))
        .expect("應該要有服務沒有實體的 L001");
    assert_eq!(
        edit::fix_for(&project, &container_findings),
        Some(Fix::AddInstances {
            container: Id::new(REDIS)
        })
    );

    let contract_findings = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L001 && f.subject == Id::new(REL_CACHE))
        .expect("應該要有契約沒實現的 L001");
    assert_eq!(
        edit::fix_for(&project, &contract_findings),
        Some(Fix::AddConnection {
            relationship: Id::new(REL_CACHE)
        })
    );
}
