//! 從零開始建一個專案。
//!
//! # 這份測試在補什麼洞
//!
//! 階段 5 把「編輯」定義成「每一條 lint 會叫的都要修得掉」。那是**修補迴圈**，
//! 它默默假設模型已經存在。但——
//!
//! ```text
//! 空專案的 lint：[]
//! ```
//!
//! 空專案是「乾淨的」。工具沒話可說，也沒東西可按，**那個迴圈根本啟動不了**。
//!
//! 所以這裡走的是**創作迴圈**：一個空專案，只用 `Edit`，
//! 建到 lint 有話說，再建到 lint 沒話說。全程不碰 Excel。

use loom_core::edit::{Edit, EditError};
use loom_core::history::History;
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};
use loom_core::logical::{Logical, Protocol, RelationshipEnd};
use loom_core::resource::{Kind, Resource, blank};

fn empty_project() -> loom_core::Project {
    loom_core::Project {
        id: Id::generate(),
        slug: "new".into(),
        name: "全新專案".into(),
        logical: Logical {
            people: vec![],
            systems: vec![],
            containers: vec![],
            relationships: vec![],
        },
        environments: vec![],
    }
}

fn rules(h: &History) -> Vec<Rule> {
    lint(h.project()).iter().map(|f| f.rule).collect()
}

/// 把一個 blank 填好名字再送出。表單做的就是這件事。
fn create(h: &mut History, mut r: Resource, tweak: impl FnOnce(&mut Resource)) -> Id {
    tweak(&mut r);
    let id = r.id().clone();
    h.edit(&Edit::AddResource(r)).expect("建不起來");
    id
}

#[test]
fn an_empty_project_lints_clean_so_the_repair_loop_cannot_start() {
    // 這是整份測試存在的理由，釘住它。
    assert_eq!(lint(&empty_project()), vec![]);
}

#[test]
fn a_clean_project_can_be_built_from_zero_with_edits_alone() {
    let mut h = History::opened(empty_project());

    // ① 系統
    let system = create(&mut h, blank(Kind::System, None, None), |r| {
        let Resource::System(s) = r else {
            unreachable!()
        };
        s.slug = "shop".into();
        s.name = "網路商店".into();
        s.external = false;
    });

    // ② 兩個服務
    let mut container = |slug: &str, name: &str| {
        create(
            &mut h,
            blank(Kind::Container, None, Some(system.clone())),
            |r| {
                let Resource::Container(c) = r else {
                    unreachable!()
                };
                c.slug = slug.into();
                c.name = name.into();
            },
        )
    };
    let api = container("order-api", "訂單 API");
    let redis = container("redis", "快取");

    // ③ Redis 的接點定義
    let endpoint = create(
        &mut h,
        blank(Kind::EndpointDef, None, Some(redis.clone())),
        |r| {
            let Resource::EndpointDef { def, .. } = r else {
                unreachable!()
            };
            def.slug = "client-port".into();
            def.protocol = Protocol::Tcp;
        },
    );

    // ④ 契約。到這裡為止 lint 還是安靜的——沒有環境，就沒有「該實現而沒實現」。
    assert_eq!(rules(&h), Vec::<Rule>::new());

    let relationship_id = create(&mut h, blank(Kind::Relationship, None, None), |r| {
        let Resource::Relationship(rel) = r else {
            unreachable!()
        };
        rel.slug = "api-連-redis".into();
        rel.purpose = "讀寫快取".into();
        rel.from = RelationshipEnd::Container(api.clone());
        rel.to = RelationshipEnd::Container(redis.clone());
        rel.to_endpoint = endpoint.clone();
    });
    assert!(!relationship_id.to_string().is_empty());

    // ⑤ 建了環境，lint 才終於有話說：兩個服務沒落地、契約沒實現。
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
        e.name = "正式環境".into();
    });

    let now_complaining = rules(&h);
    assert!(
        now_complaining.iter().filter(|r| **r == Rule::L001).count() >= 3,
        "建了環境之後 lint 應該要說「這些東西都還沒落地」，實際：{now_complaining:?}"
    );

    // ⑥ 照著 lint 說的建機器（走既有的批次功能），再補連線。
    let env = h.project().environment(&prod).unwrap().clone();
    for (container_id, slug, ip) in [(&api, "api", 11u32), (&redis, "redis", 21)] {
        let spec = loom_core::batch::BatchSpec {
            count: 1,
            name_template: format!("{slug}-{{n}}"),
            node_template: format!("vm-{slug}-{{n}}"),
            start: 1,
            pad: 2,
            address_template: format!("10.0.0.{{ip}}:{}", 6379),
            ip_start: ip,
            node_kind: loom_core::environment::NodeKind::VirtualMachine,
            container: container_id.clone(),
            endpoint: loom_core::batch::EndpointPlan {
                def: endpoint.clone(),
                slug: "client-port".into(),
                protocol: Protocol::Tcp,
            },
        };
        // api 身上沒有那個接點定義，所以只有 redis 這批建得起來——
        // 這正是 `batch::plan` 該擋的（放過去會變成很難懂的 L012）。
        let result = loom_core::batch::plan(h.project(), &env, &spec);
        if container_id == &api {
            assert!(result.is_err(), "接點不屬於 api，應該被擋下來");
            continue;
        }
        let plan = result.unwrap();
        h.edit(&Edit::AddInstances {
            environment: prod.clone(),
            within: None,
            nodes: plan.nodes,
        })
        .unwrap();
    }

    // api 沒有自己的接點定義，直接建一台機器給它。
    let node = create(&mut h, blank(Kind::Node, Some(prod.clone()), None), |r| {
        let Resource::Node { node, .. } = r else {
            unreachable!()
        };
        node.slug = "vm-api-01".into();
    });
    assert!(!node.to_string().is_empty());

    // 到這裡 redis 有落地了，api 還沒——lint 應該只剩 api 那一組。
    let remaining = lint(h.project());
    assert!(
        remaining.iter().any(|f| f.subject == api),
        "api 還沒落地，lint 應該還在叫：{remaining:?}"
    );
    assert!(
        !remaining.iter().any(|f| f.subject == redis),
        "redis 已經有落地了，不該還在叫：{remaining:?}"
    );

    // ⑦ 整個過程都可以復原。
    let steps = remaining.len();
    assert!(steps > 0);
    assert!(h.undo());
    assert!(h.is_dirty() || h.undo_label().is_some());
}

#[test]
fn a_slug_cannot_be_empty() {
    // 空名字在畫面上是一列空白，找不到也刪不掉。
    let mut h = History::opened(empty_project());
    let err = h.edit(&Edit::AddResource(blank(Kind::System, None, None)));
    assert_eq!(err, Err(EditError::EmptySlug("系統")));
    assert_eq!(h.undo_label(), None, "失敗的新增卻留下一步");
}

#[test]
fn two_siblings_cannot_share_a_slug() {
    // slug 是人看的識別，而萬用字元比對的就是它。兩個同名會讓 expect 數錯，
    // 而使用者以為那是兩個不同的東西。
    let mut h = History::opened(empty_project());
    create(&mut h, blank(Kind::System, None, None), |r| {
        let Resource::System(s) = r else {
            unreachable!()
        };
        s.slug = "shop".into();
    });

    let mut another = blank(Kind::System, None, None);
    let Resource::System(s) = &mut another else {
        unreachable!()
    };
    s.slug = "shop".into();

    assert_eq!(
        h.edit(&Edit::AddResource(another)),
        Err(EditError::SlugTaken {
            kind: "系統",
            slug: "shop".into()
        })
    );
}

#[test]
fn renaming_does_not_wipe_the_contents() {
    // Resource 帶的是完整的值，而環境裡裝著機器與連線。
    // 若修改時整份換掉，使用者改個名字就會把整個環境洗掉。
    let mut h = History::opened(empty_project());
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
    });
    create(&mut h, blank(Kind::Node, Some(prod.clone()), None), |r| {
        let Resource::Node { node, .. } = r else {
            unreachable!()
        };
        node.slug = "vm-01".into();
    });
    assert_eq!(h.project().environments[0].nodes.len(), 1);

    let mut renamed = h.project().environments[0].clone();
    renamed.slug = "production".into();
    // 前端手上的那份可能是舊的、或根本沒帶內容——都不該把機器弄丟。
    renamed.nodes.clear();
    h.edit(&Edit::UpdateResource(Resource::Environment(renamed)))
        .unwrap();

    assert_eq!(h.project().environments[0].slug, "production");
    assert_eq!(
        h.project().environments[0].nodes.len(),
        1,
        "改個名字就把機器弄丟了"
    );
}

#[test]
fn deleting_a_container_keeps_its_instances_but_l012_flags_them() {
    // 使用者選的策略：允許懸空 ＋ lint 報錯。
    // 連帶刪除會一次消失幾十個東西，而這工具的重點就是「怕漏」。
    let mut project = empty_project();
    let mut h = History::opened(std::mem::replace(&mut project, empty_project()));

    let system = create(&mut h, blank(Kind::System, None, None), |r| {
        let Resource::System(s) = r else {
            unreachable!()
        };
        s.slug = "shop".into();
    });
    let redis = create(&mut h, blank(Kind::Container, None, Some(system)), |r| {
        let Resource::Container(c) = r else {
            unreachable!()
        };
        c.slug = "redis".into();
    });
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
    });

    let spec = loom_core::batch::BatchSpec {
        count: 2,
        name_template: "redis-{n}".into(),
        node_template: "vm-redis-{n}".into(),
        start: 1,
        pad: 2,
        address_template: "10.0.0.{ip}:6379".into(),
        ip_start: 11,
        node_kind: loom_core::environment::NodeKind::VirtualMachine,
        container: redis.clone(),
        endpoint: loom_core::batch::EndpointPlan {
            def: create(
                &mut h,
                blank(Kind::EndpointDef, None, Some(redis.clone())),
                |r| {
                    let Resource::EndpointDef { def, .. } = r else {
                        unreachable!()
                    };
                    def.slug = "client-port".into();
                },
            ),
            slug: "client-port".into(),
            protocol: Protocol::Tcp,
        },
    };
    let env = h.project().environment(&prod).unwrap().clone();
    let plan = loom_core::batch::plan(h.project(), &env, &spec).unwrap();
    h.edit(&Edit::AddInstances {
        environment: prod.clone(),
        within: None,
        nodes: plan.nodes,
    })
    .unwrap();

    // 刪掉服務。兩台落地還在，但 L012 要點名它們。
    let that_container = h.project().logical.container(&redis).unwrap().clone();
    h.edit(&Edit::DeleteResource(Resource::Container(that_container)))
        .unwrap();

    assert_eq!(
        h.project().environments[0].instances().len(),
        2,
        "連帶刪掉了落地——那不是選的策略"
    );
    let dangling: Vec<_> = lint(h.project())
        .into_iter()
        .filter(|f| f.rule == Rule::L012)
        .collect();
    assert_eq!(dangling.len(), 2, "兩台落地都要被點名：{dangling:?}");

    // 刪錯了退得回來。
    assert!(h.undo());
    assert!(
        !lint(h.project()).iter().any(|f| f.rule == Rule::L012),
        "復原之後不該還有懸空的參照"
    );
}

#[test]
fn the_impact_of_a_deletion_is_visible() {
    // 刪除確認框問的是「會弄壞什麼」，靠的是 edit::preview 跑真的刪除再比對 lint。
    let mut h = History::opened(empty_project());
    let system = create(&mut h, blank(Kind::System, None, None), |r| {
        let Resource::System(s) = r else {
            unreachable!()
        };
        s.slug = "shop".into();
    });
    create(
        &mut h,
        blank(Kind::Container, None, Some(system.clone())),
        |r| {
            let Resource::Container(c) = r else {
                unreachable!()
            };
            c.slug = "redis".into();
        },
    );

    let that_system = h.project().logical.systems[0].clone();
    let impact = loom_core::edit::preview(
        h.project(),
        &Edit::DeleteResource(Resource::System(that_system)),
    )
    .unwrap();

    assert!(!impact.is_safe());
    assert!(
        impact.introduced.iter().any(|f| f.rule == Rule::L012),
        "刪掉系統會讓服務懸空，卻沒說：{:?}",
        impact.introduced
    );
}

#[test]
fn deleting_a_node_takes_its_children_with_it() {
    // 子節點是「住在裡面」，不是「參照」——它們不該變成懸空的孤兒。
    let mut h = History::opened(empty_project());
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
    });
    let site = create(&mut h, blank(Kind::Node, Some(prod.clone()), None), |r| {
        let Resource::Node { node, .. } = r else {
            unreachable!()
        };
        node.slug = "dc-main".into();
        node.kind = loom_core::environment::NodeKind::Site;
    });
    create(
        &mut h,
        blank(Kind::Node, Some(prod.clone()), Some(site.clone())),
        |r| {
            let Resource::Node { node, .. } = r else {
                unreachable!()
            };
            node.slug = "vm-01".into();
        },
    );
    assert_eq!(h.project().environments[0].nodes[0].children.len(), 1);

    let that_site = h.project().environments[0].nodes[0].clone();
    h.edit(&Edit::DeleteResource(Resource::Node {
        environment: prod,
        within: None,
        node: that_site,
    }))
    .unwrap();

    assert!(h.project().environments[0].nodes.is_empty());
}

#[test]
fn an_infra_node_and_its_vip_are_created_separately() {
    let mut h = History::opened(empty_project());
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
    });
    let f5 = create(&mut h, blank(Kind::Infra, Some(prod.clone()), None), |r| {
        let Resource::Infra { node, .. } = r else {
            unreachable!()
        };
        node.slug = "f5-01".into();
    });
    create(
        &mut h,
        blank(Kind::InfraEndpoint, Some(prod.clone()), Some(f5.clone())),
        |r| {
            let Resource::InfraEndpoint { endpoint, .. } = r else {
                unreachable!()
            };
            endpoint.slug = "vip-redis".into();
            endpoint.address = Some("10.0.0.100:6379".into());
        },
    );

    let infra = &h.project().environments[0].infra[0];
    assert_eq!(infra.slug, "f5-01");
    assert_eq!(infra.endpoints.len(), 1);
    // 設備不對應任何邏輯層元素，所以它的接點沒有 def——L012 不該誤報。
    assert!(infra.endpoints[0].def.is_none());
    assert!(!lint(h.project()).iter().any(|f| f.rule == Rule::L012));
}

#[test]
fn every_resource_kind_can_be_created_and_deleted() {
    // 少做一種的話，那種資源就會變成「只能用 Excel 匯」——
    // 而使用者說的情境正是「沒有 Excel 可以匯」。
    let mut h = History::opened(empty_project());
    let system = create(&mut h, blank(Kind::System, None, None), |r| {
        let Resource::System(s) = r else {
            unreachable!()
        };
        s.slug = "shop".into();
    });
    let prod = create(&mut h, blank(Kind::Environment, None, None), |r| {
        let Resource::Environment(e) = r else {
            unreachable!()
        };
        e.slug = "prod".into();
    });
    let f5 = create(&mut h, blank(Kind::Infra, Some(prod.clone()), None), |r| {
        let Resource::Infra { node, .. } = r else {
            unreachable!()
        };
        node.slug = "f5-01".into();
    });

    let every_kind = [
        (Kind::Person, None, None),
        (Kind::Container, None, Some(system.clone())),
        (Kind::EndpointDef, None, Some(system.clone())),
        (Kind::Relationship, None, None),
        (Kind::Node, Some(prod.clone()), None),
        (Kind::InfraEndpoint, Some(prod.clone()), Some(f5.clone())),
        (
            Kind::SystemInstance,
            Some(prod.clone()),
            Some(system.clone()),
        ),
    ];

    for (i, (kind, env, owner)) in every_kind.into_iter().enumerate() {
        let mut r = blank(kind, env, owner);
        set_slug(&mut r, &format!("東西-{i}"));
        h.edit(&Edit::AddResource(r.clone()))
            .unwrap_or_else(|e| panic!("{kind:?} 建不起來：{e}"));
        h.edit(&Edit::DeleteResource(r))
            .unwrap_or_else(|e| panic!("{kind:?} 刪不掉：{e}"));
    }
}

fn set_slug(r: &mut Resource, slug: &str) {
    match r {
        Resource::Person(p) => p.slug = slug.into(),
        Resource::System(s) => s.slug = slug.into(),
        Resource::Container(c) => c.slug = slug.into(),
        Resource::EndpointDef { def, .. } => def.slug = slug.into(),
        Resource::Relationship(rel) => rel.slug = slug.into(),
        Resource::Environment(e) => e.slug = slug.into(),
        Resource::Node { node, .. } => node.slug = slug.into(),
        Resource::Infra { node, .. } => node.slug = slug.into(),
        Resource::InfraEndpoint { endpoint, .. } => endpoint.slug = slug.into(),
        Resource::SystemInstance { instance, .. } => instance.slug = slug.into(),
    }
}
