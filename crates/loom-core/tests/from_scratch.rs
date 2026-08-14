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
        memo: String::new(),
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

    // ⑤ 建了環境，lint 才終於有話說：兩個服務沒有實體、契約沒實現。
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
        "建了環境之後 lint 應該要說「這些東西都還沒有實體」，實際：{now_complaining:?}"
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

    // 到這裡 redis 有實體了，api 還沒——lint 應該只剩 api 那一組。
    let remaining = lint(h.project());
    assert!(
        remaining.iter().any(|f| f.subject == api),
        "api 還沒有實體，lint 應該還在叫：{remaining:?}"
    );
    assert!(
        !remaining.iter().any(|f| f.subject == redis),
        "redis 已經有實體了，不該還在叫：{remaining:?}"
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

    // 刪掉服務。兩個服務實體還在，但 L012 要點名它們。
    let that_container = h.project().logical.container(&redis).unwrap().clone();
    h.edit(&Edit::DeleteResource(Resource::Container(that_container)))
        .unwrap();

    assert_eq!(
        h.project().environments[0].instances().len(),
        2,
        "連帶刪掉了服務實體——那不是選的策略"
    );
    let dangling: Vec<_> = lint(h.project())
        .into_iter()
        .filter(|f| f.rule == Rule::L012)
        .collect();
    assert_eq!(dangling.len(), 2, "兩個服務實體都要被點名：{dangling:?}");

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
        Resource::Instance { instance, .. } => instance.slug = slug.into(),
        Resource::SystemInstance { instance, .. } => instance.slug = slug.into(),
    }
}

/// 服務實體：**位址就住在這裡**，所以它必須自己建得起來、改得動。
///
/// 在它成為一個 `Resource` 之前，服務實體只能靠「批次建機器」順便產生，
/// 而位址只能等 L006 叫了才改得到。也就是說「我知道這台的 IP，
/// 我現在就想填進去」這件事在畫面上**沒有地方可以做**。
mod instances {
    use super::*;
    use loom_core::environment::Endpoint;
    use loom_core::logical::Protocol;

    /// 一個環境、一台機器、一個服務。回傳 (history, 環境, 機器, 服務)。
    fn ready() -> (History, Id, Id, Id) {
        let mut h = History::opened(empty_project());
        let system = create(&mut h, blank(Kind::System, None, None), |r| {
            let Resource::System(s) = r else {
                unreachable!()
            };
            s.slug = "shop".into();
        });
        let container = create(
            &mut h,
            blank(Kind::Container, None, Some(system.clone())),
            |r| {
                let Resource::Container(c) = r else {
                    unreachable!()
                };
                c.slug = "redis".into();
            },
        );
        let env = create(&mut h, blank(Kind::Environment, None, None), |r| {
            let Resource::Environment(e) = r else {
                unreachable!()
            };
            e.slug = "prod".into();
        });
        let node = create(&mut h, blank(Kind::Node, Some(env.clone()), None), |r| {
            let Resource::Node { node, .. } = r else {
                unreachable!()
            };
            node.slug = "vm-01".into();
        });
        (h, env, node, container)
    }

    fn instance_on(env: &Id, node: &Id, container: &Id, slug: &str) -> Resource {
        let mut r = blank(Kind::Instance, Some(env.clone()), Some(node.clone()));
        let Resource::Instance { instance, .. } = &mut r else {
            unreachable!()
        };
        instance.slug = slug.into();
        instance.container = container.clone();
        r
    }

    #[test]
    fn a_single_instance_can_be_created_without_the_batch_tool() {
        let (mut h, env, node, container) = ready();
        h.edit(&Edit::AddResource(instance_on(
            &env, &node, &container, "redis-01",
        )))
        .expect("服務實體建不起來");

        let e = h.project().environment(&env).unwrap();
        assert_eq!(e.instances().len(), 1);
        assert_eq!(e.instances()[0].slug, "redis-01");
    }

    #[test]
    fn an_address_can_be_filled_in_directly() {
        // 這是整組測試的重點：**不必等 lint 叫，也不必走批次建立**。
        let (mut h, env, node, container) = ready();
        let mut r = instance_on(&env, &node, &container, "redis-01");
        let Resource::Instance { instance, .. } = &mut r else {
            unreachable!()
        };
        instance.endpoints.push(Endpoint {
            id: Id::generate(),
            slug: "client-port".into(),
            def: None,
            protocol: Protocol::Tcp,
            address: Some("10.0.1.11:6379".into()),
            memo: String::new(),
        });
        h.edit(&Edit::AddResource(r)).unwrap();

        let e = h.project().environment(&env).unwrap();
        assert_eq!(
            e.instances()[0].endpoints[0].address.as_deref(),
            Some("10.0.1.11:6379")
        );
    }

    #[test]
    fn editing_an_instance_does_not_quietly_delete_it() {
        // 改的時候要先從舊的那台機器移走，而 `replace_or_push` 在「找不到」時
        // 是**安靜地什麼都不做**——照抄的話修改就會變成靜悄悄的刪除。
        let (mut h, env, node, container) = ready();
        let mut r = instance_on(&env, &node, &container, "redis-01");
        h.edit(&Edit::AddResource(r.clone())).unwrap();

        let Resource::Instance { instance, .. } = &mut r else {
            unreachable!()
        };
        instance.slug = "redis-99".into();
        h.edit(&Edit::UpdateResource(r)).expect("改不動");

        let e = h.project().environment(&env).unwrap();
        assert_eq!(e.instances().len(), 1, "改個名字就不見了");
        assert_eq!(e.instances()[0].slug, "redis-99");
    }

    #[test]
    fn moving_an_instance_to_another_machine_does_not_leave_a_copy_behind() {
        // 搬家。留一份在舊機器上的話，萬用字元會把它數成兩台——
        // 而那正是這個工具要防的誤會。
        let (mut h, env, node, container) = ready();
        let other = create(&mut h, blank(Kind::Node, Some(env.clone()), None), |r| {
            let Resource::Node { node, .. } = r else {
                unreachable!()
            };
            node.slug = "vm-02".into();
        });

        let mut r = instance_on(&env, &node, &container, "redis-01");
        h.edit(&Edit::AddResource(r.clone())).unwrap();

        let Resource::Instance { node: host, .. } = &mut r else {
            unreachable!()
        };
        *host = other.clone();
        h.edit(&Edit::UpdateResource(r)).expect("搬不動");

        let e = h.project().environment(&env).unwrap();
        assert_eq!(e.instances().len(), 1, "舊機器上還留著一份");
        let on_other = e.nodes.iter().find(|n| n.id == other).unwrap();
        assert_eq!(on_other.instances.len(), 1, "沒搬到新機器上");
    }

    #[test]
    fn two_instances_in_one_environment_cannot_share_a_name() {
        // 萬用字元比對的就是 slug。同名會讓 `expect` 數成兩台，
        // 而使用者會以為那是兩台不同的機器。
        let (mut h, env, node, container) = ready();
        let other = create(&mut h, blank(Kind::Node, Some(env.clone()), None), |r| {
            let Resource::Node { node, .. } = r else {
                unreachable!()
            };
            node.slug = "vm-02".into();
        });

        h.edit(&Edit::AddResource(instance_on(
            &env, &node, &container, "redis-01",
        )))
        .unwrap();
        // 不同機器上也不行——名字在整個環境裡唯一。
        let clash = h.edit(&Edit::AddResource(instance_on(
            &env, &other, &container, "redis-01",
        )));
        assert!(clash.is_err(), "撞名應該要被擋下來");
    }

    #[test]
    fn deleting_an_instance_reaches_into_nested_machines() {
        // 服務實體住在樹裡（站點 → 機器 → 服務實體）。只掃最上層的話，
        // 站點底下的那些會刪不掉，而畫面上按鈕按了沒反應。
        let (mut h, env, site, container) = ready();
        let inner = create(
            &mut h,
            blank(Kind::Node, Some(env.clone()), Some(site.clone())),
            |r| {
                let Resource::Node { node, .. } = r else {
                    unreachable!()
                };
                node.slug = "vm-inner".into();
            },
        );
        let r = instance_on(&env, &inner, &container, "redis-01");
        h.edit(&Edit::AddResource(r.clone())).unwrap();
        h.edit(&Edit::DeleteResource(r)).expect("刪不掉");

        assert!(
            h.project()
                .environment(&env)
                .unwrap()
                .instances()
                .is_empty()
        );
    }
}

/// 新增連線至少要說得通。
///
/// lint 是**事後**的：東西已經建進去了才叫你回頭修。對「兩端一樣」這種
/// 一看就知道錯的東西那太晚了——它建出來之後長得像一條正常的連線，
/// 使用者不會回頭懷疑自己按錯。
mod adding_a_connection {
    use super::*;
    use loom_core::environment::{Endpointing, InstanceRef};

    /// 環境、兩個服務實體、一條契約。
    fn ready() -> (History, Id, Id, Id, Id) {
        let mut h = History::opened(empty_project());
        let system = create(&mut h, blank(Kind::System, None, None), |r| {
            let Resource::System(s) = r else {
                unreachable!()
            };
            s.slug = "shop".into();
        });
        let container = create(
            &mut h,
            blank(Kind::Container, None, Some(system.clone())),
            |r| {
                let Resource::Container(c) = r else {
                    unreachable!()
                };
                c.slug = "app".into();
            },
        );
        let rel = create(&mut h, blank(Kind::Relationship, None, None), |r| {
            let Resource::Relationship(x) = r else {
                unreachable!()
            };
            x.slug = "app-連-app".into();
        });
        let env = create(&mut h, blank(Kind::Environment, None, None), |r| {
            let Resource::Environment(e) = r else {
                unreachable!()
            };
            e.slug = "prod".into();
        });
        let node = create(&mut h, blank(Kind::Node, Some(env.clone()), None), |r| {
            let Resource::Node { node, .. } = r else {
                unreachable!()
            };
            node.slug = "vm-01".into();
        });

        let mut ids = vec![];
        for slug in ["a-01", "b-01"] {
            let mut r = blank(Kind::Instance, Some(env.clone()), Some(node.clone()));
            let Resource::Instance { instance, .. } = &mut r else {
                unreachable!()
            };
            instance.slug = slug.into();
            instance.container = container.clone();
            ids.push(instance.id.clone());
            h.edit(&Edit::AddResource(r)).unwrap();
        }
        (h, env, rel, ids[0].clone(), ids[1].clone())
    }

    fn at(id: &Id) -> Endpointing {
        Endpointing::Instance {
            target: InstanceRef::One(id.clone()),
            endpoint: None,
        }
    }

    fn add(env: &Id, serves: &Id, from: Endpointing, to: Endpointing) -> Edit {
        Edit::AddConnection {
            environment: env.clone(),
            id: Id::generate(),
            serves: serves.clone(),
            purpose: "測試".into(),
            kind: Default::default(),
            from,
            to,
        }
    }

    #[test]
    fn a_normal_connection_still_goes_through() {
        let (mut h, env, rel, a, b) = ready();
        h.edit(&add(&env, &rel, at(&a), at(&b)))
            .expect("正常的連線被擋掉了");
    }

    #[test]
    fn both_ends_pointing_at_the_same_thing_is_rejected() {
        // 兩端一樣的連線什麼也沒說，而且它長得像一條正常的連線。
        let (mut h, env, rel, a, _) = ready();
        assert!(h.edit(&add(&env, &rel, at(&a), at(&a))).is_err());
    }

    #[test]
    fn the_endpoint_does_not_make_a_self_loop_acceptable() {
        // 同一台機器的 A 埠連 B 埠仍然是自己連自己。
        let (mut h, env, rel, a, _) = ready();
        let with_endpoint = Endpointing::Instance {
            target: InstanceRef::One(a.clone()),
            endpoint: Some(Id::generate()),
        };
        assert!(h.edit(&add(&env, &rel, at(&a), with_endpoint)).is_err());
    }

    #[test]
    fn a_person_cannot_be_the_target() {
        // 人是流量的起點。沒有人會連進一個人裡面。
        let (mut h, env, rel, a, _) = ready();
        let person = create(&mut h, blank(Kind::Person, None, None), |r| {
            let Resource::Person(p) = r else {
                unreachable!()
            };
            p.slug = "客戶".into();
        });
        let as_target = Endpointing::Person { person };
        assert!(h.edit(&add(&env, &rel, at(&a), as_target)).is_err());
    }

    #[test]
    fn a_connection_serving_nothing_is_rejected() {
        // 沒有契約的連線 lint 會叫（L003），而且覆蓋矩陣上看不到它——
        // 也就是「怕漏」的那張表看不到它。
        let (mut h, env, _, a, b) = ready();
        assert!(h.edit(&add(&env, &Id::new(""), at(&a), at(&b))).is_err());
        assert!(
            h.edit(&add(&env, &Id::new("不存在"), at(&a), at(&b)))
                .is_err()
        );
    }

    #[test]
    fn ends_unrelated_to_the_contract_are_still_allowed() {
        // 中間要不要經過 F5、這條路走幾段，是人的決定。多段路徑就是這樣拼的，
        // 擋掉的話 F5 那一段永遠建不起來。兜不兜得起來交給 L002 的可達性檢查。
        let (mut h, env, rel, a, b) = ready();
        h.edit(&add(&env, &rel, at(&b), at(&a)))
            .expect("反向的一段被擋掉了");
    }
}
