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

fn 規格(count: u32) -> BatchSpec {
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
fn 沒有redis的專案() -> loom_core::Project {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes.retain(|n| n.slug != "vm-redis-01");
    dev.connections.retain(|c| c.serves != Id::new(REL_CACHE));
    project
}

#[test]
fn l001_服務一台都沒建_批次建出來就修好了() {
    let mut project = 沒有redis的專案();
    assert!(
        lint(&project)
            .iter()
            .any(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS)),
        "素材應該要有「服務沒落地」的 L001"
    );

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(2)).unwrap();

    edit::apply(
        &mut project,
        &Edit::AddInstances {
            environment: env.id.clone(),
            within: None,
            nodes: plan.nodes.clone(),
        },
    )
    .unwrap();

    // 機器建好了，服務落地的那條 L001 就消失了。
    // 剩下的是「還沒接線」——那是補連線的事，另一個修法。
    let 剩下的: Vec<Rule> = lint(&project).iter().map(|f| f.rule).collect();
    assert!(
        !lint(&project)
            .iter()
            .any(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS)),
        "建了機器 L001 卻還在：{剩下的:?}"
    );
}

#[test]
fn 建完機器再補連線_整個環境就乾淨了() {
    // 這才是完整的一圈：L001 說沒機器 → 建機器 → L001 說沒連線 → 補連線 → 乾淨。
    // 兩個修法各自只走一半，串起來才等於「使用者真的把問題解決了」。
    let mut project = 沒有redis的專案();
    let env_id = project.environments[2].id.clone();

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(2)).unwrap();
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
fn 撞名的整批擋下來_不會建一半() {
    // 同一個環境有兩台 redis-01 會讓萬用字元數到 2，
    // 而使用者以為那是兩台不同的機器——正好是這個工具要防的誤會。
    let project = healthy_project(); // dev 已經有一台 redis-01
    let env = &project.environments[2];

    // 機器名先撞到（dev 那台叫 vm-redis-01），落地名也會撞，
    // 兩者都足以擋下整批。
    let err = loom_core::batch::plan(&project, env, &規格(3)).unwrap_err();
    assert_eq!(
        err,
        loom_core::batch::BatchError::Taken("vm-redis-01".into())
    );
}

#[test]
fn 服務不存在時擋下來() {
    let project = healthy_project();
    let env = &project.environments[2];
    let mut spec = 規格(2);
    spec.container = Id::new("c-根本沒這個服務");

    assert!(loom_core::batch::plan(&project, env, &spec).is_err());
}

#[test]
fn 接點不屬於那個服務時擋下來() {
    // 拿 API 的接點去建 Redis，lint 之後會變成一個很難懂的 L003。
    // 在源頭擋掉比較好解釋。
    let project = healthy_project();
    let env = &project.environments[2];
    let mut spec = 規格(2);
    spec.endpoint.def = Id::new(API_EGRESS);

    assert!(loom_core::batch::plan(&project, env, &spec).is_err());
}

#[test]
fn 預覽說得出每一台會長什麼樣() {
    // 一次建六台是會後悔的操作，套用前一定要看得到清單。
    let project = 沒有redis的專案();
    let env = &project.environments[2];
    let plan = loom_core::batch::plan(&project, env, &規格(3)).unwrap();

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
fn 可以建在站點底下() {
    let mut project = 沒有redis的專案();
    // 先給 dev 一個站點。
    let 站點 = loom_core::environment::DeploymentNode {
        id: Id::new("n-dev-機房"),
        slug: "dc-main".into(),
        kind: NodeKind::Site,
        children: vec![],
        instances: vec![],
    };
    project.environments[2].nodes.push(站點);

    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(2)).unwrap();
    edit::apply(
        &mut project,
        &Edit::AddInstances {
            environment: env.id.clone(),
            within: Some(Id::new("n-dev-機房")),
            nodes: plan.nodes,
        },
    )
    .unwrap();

    let 機房 = project.environments[2]
        .nodes
        .iter()
        .find(|n| n.slug == "dc-main")
        .unwrap();
    assert_eq!(機房.children.len(), 2);
    // 巢狀底下的落地一樣算得到，萬用字元才數得對。
    assert_eq!(
        project.environments[2].instances_matching("redis-*").len(),
        2
    );
}

#[test]
fn 建到不存在的節點底下會報錯() {
    let mut project = 沒有redis的專案();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(2)).unwrap();

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
fn 同一批不會被建兩次() {
    let mut project = 沒有redis的專案();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(2)).unwrap();

    let 這一批 = Edit::AddInstances {
        environment: env.id.clone(),
        within: None,
        nodes: plan.nodes,
    };
    edit::apply(&mut project, &這一批).unwrap();
    assert!(
        edit::apply(&mut project, &這一批).is_err(),
        "同一批竟然建得進去第二次"
    );
}

#[test]
fn 建完可以整批復原掉() {
    use loom_core::history::History;

    let project = 沒有redis的專案();
    let 原本 = project.clone();
    let env = project.environments[2].clone();
    let plan = loom_core::batch::plan(&project, &env, &規格(6)).unwrap();

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
    assert_eq!(h.project(), &原本, "復原之後沒有回到原狀");
    assert_eq!(h.redo_label(), Some("批次建立機器"));
}

#[test]
fn l001_報在服務上時給的是建機器的修法() {
    // 同樣是 L001，報在契約上要補連線、報在服務上要建機器。
    // 這個分辨是規則，不是畫面——所以由 `fix_for` 決定。
    let project = 沒有redis的專案();

    let 服務的 = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L001 && f.subject == Id::new(REDIS))
        .expect("應該要有服務沒落地的 L001");
    assert_eq!(
        edit::fix_for(&project, &服務的),
        Some(Fix::AddInstances {
            container: Id::new(REDIS)
        })
    );

    let 契約的 = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L001 && f.subject == Id::new(REL_CACHE))
        .expect("應該要有契約沒實現的 L001");
    assert_eq!(
        edit::fix_for(&project, &契約的),
        Some(Fix::AddConnection {
            relationship: Id::new(REL_CACHE)
        })
    );
}
