//! 新增連線：L001／L002 的修法。
//!
//! 這兩條是 lint 裡最重要的（它們正對應「怕漏」），也是最後兩條修不掉的。
//! 所以這裡守的是同一件事：**弄壞 → 修 → lint 乾淨**。

mod common;

use common::*;
use loom_core::connect::{self, Proposal};
use loom_core::edit::{self, Edit};
use loom_core::environment::{ConnectionEnd, ConnectionKind, Endpointing, InstanceRef};
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};

fn rules(project: &loom_core::Project) -> Vec<Rule> {
    lint(project).iter().map(|f| f.rule).collect()
}

/// 把提案變成一次新增。使用者按下「建立」時做的就是這件事。
fn create(project: &mut loom_core::Project, env: &Id, p: &Proposal) {
    edit::apply(
        project,
        &Edit::AddConnection {
            environment: env.clone(),
            id: p.id.clone(),
            serves: p.serves.clone(),
            purpose: p.purpose.clone(),
            kind: ConnectionKind::Primary,
            from: p.from.clone().expect("提案沒有來源"),
            to: p.to.clone().expect("提案沒有目標"),
        },
    )
    .unwrap();
}

#[test]
fn l001_unrealised_contract_is_fixed_by_the_proposed_connection() {
    let mut project = healthy_project();
    // dev 的快取那條整個拿掉 → L001。
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));
    assert!(rules(&project).contains(&Rule::L001));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();
    assert!(p.is_complete(), "擬不出來：{:?}", p.notes);

    create(&mut project, &env.id, &p);
    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn l002_unreachable_is_fixed_by_adding_the_missing_segment() {
    // prod 的快取走兩段：API → F5 → Redis。砍掉第二段就走不通。
    let mut project = healthy_project();
    let second_hop = project.environments[0].connections[1].clone();
    project.environments[0]
        .connections
        .retain(|c| c.id != second_hop.id);
    assert!(rules(&project).contains(&Rule::L002));

    // 提案擬的是直達那條（工具不知道中間要走 F5，這是人的決定），
    // 所以這裡示範的是使用者自己挑兩端——那正是 `choices` 的用途。
    edit::apply(
        &mut project,
        &Edit::AddConnection {
            environment: Id::new("env-prod"),
            id: Id::new("conn-補回來"),
            serves: Id::new(REL_CACHE),
            purpose: "F5 轉給 Redis".into(),
            kind: ConnectionKind::Primary,
            from: second_hop.from,
            to: second_hop.to,
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn the_proposal_reuses_the_contract_purpose() {
    // 留白換來的只是一個 L007，而使用者剛剛才修好一個問題。
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let contract_purpose = &project.logical.relationships[0].purpose;
    assert!(!contract_purpose.is_empty());
    assert_eq!(&p.purpose, contract_purpose);
}

#[test]
fn several_instances_are_proposed_as_a_wildcard_not_one_by_one() {
    // 列出每一台會產生 N 條連線，之後每加一台機器都要記得補一條——
    // 那正是「怕漏」要防的事。
    let mut project = healthy_project();
    project.environments[0]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[0].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let Some(Endpointing::Instance {
        target:
            InstanceRef::Pattern {
                slug_pattern,
                expect,
                ..
            },
        ..
    }) = &p.to
    else {
        panic!("prod 有三台 Redis，應該擬成萬用字元，實際是 {:?}", p.to);
    };
    assert_eq!(slug_pattern, "redis-*");
    assert_eq!(*expect, Some(3));
}

#[test]
fn with_a_single_instance_it_is_named_directly() {
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    assert!(
        matches!(
            &p.to,
            Some(Endpointing::Instance {
                target: InstanceRef::One(_),
                ..
            })
        ),
        "dev 只有一台，不該用萬用字元：{:?}",
        p.to
    );
}

#[test]
fn an_incomplete_proposal_says_why() {
    // 服務一台都還沒建的時候，「請選擇來源」是個無解的問題。
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.connections.clear();
    dev.nodes[1].instances.clear(); // Redis 一台都沒有

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    assert!(!p.is_complete());
    assert!(p.to.is_none());
    assert!(
        p.notes.iter().any(|n| n.contains("一台都還沒建")),
        "沒說為什麼擬不出來：{:?}",
        p.notes
    );
}

#[test]
fn the_proposal_states_its_assumptions() {
    // 工具不知道中間要不要經過 F5——那是人的決定。
    // 擬得像真的卻不說，使用者會直接按下去，然後少掉一段。
    let mut project = healthy_project();
    project.environments[0]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[0].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    assert!(
        p.notes.iter().any(|n| n.contains("直達")),
        "沒講出它只擬了直達的一段：{:?}",
        p.notes
    );
}

#[test]
fn the_proposal_mints_the_id_so_apply_is_deterministic() {
    // 若 id 在 apply 裡才產生，預覽算出來的跟真正套用的就是兩份不同的東西，
    // 復原之後重做也會得到一條 id 不一樣的連線。
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let mut first = project.clone();
    create(&mut first, &env.id, &p);
    let mut second = project.clone();
    create(&mut second, &env.id, &p);
    assert_eq!(first, second);
}

#[test]
fn the_same_connection_cannot_be_added_twice() {
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();
    create(&mut project, &env.id, &p);

    let again = edit::apply(
        &mut project,
        &Edit::AddConnection {
            environment: env.id.clone(),
            id: p.id.clone(),
            serves: p.serves.clone(),
            purpose: p.purpose.clone(),
            kind: ConnectionKind::Primary,
            from: p.from.clone().unwrap(),
            to: p.to.clone().unwrap(),
        },
    );
    assert!(again.is_err(), "同一個 id 竟然加得進去第二次");
}

#[test]
fn an_addition_can_be_undone() {
    use loom_core::history::History;

    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));
    let original = project.clone();

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let mut h = History::opened(project);
    h.edit(&Edit::AddConnection {
        environment: env.id.clone(),
        id: p.id.clone(),
        serves: p.serves.clone(),
        purpose: p.purpose.clone(),
        kind: ConnectionKind::Primary,
        from: p.from.clone().unwrap(),
        to: p.to.clone().unwrap(),
    })
    .unwrap();

    assert!(lint(h.project()).is_empty());
    assert!(h.undo());
    assert_eq!(h.project(), &original);
    assert_eq!(h.undo_label(), None);
    assert_eq!(h.redo_label(), Some("新增連線"));
}

mod connectable_places {
    use super::*;

    #[test]
    fn a_person_only_appears_on_the_source_side() {
        // 人是流量的起點，不會有人「連到一個人」。
        let mut project = healthy_project();
        project.logical.people.push(loom_core::logical::Person {
            id: Id::new("p-客戶"),
            slug: "customer".into(),
            name: "客戶".into(),
            memo: String::new(),
        });
        let env = &project.environments[0];

        let source = connect::choices(&project, env, ConnectionEnd::From);
        let target = connect::choices(&project, env, ConnectionEnd::To);

        assert!(source.iter().any(|c| c.group == "人"));
        assert!(!target.iter().any(|c| c.group == "人"));
    }

    #[test]
    fn the_source_side_offers_no_specific_endpoint() {
        // 客戶端的 port 通常是作業系統分配的。
        let project = healthy_project();
        let env = &project.environments[0];

        let source = connect::choices(&project, env, ConnectionEnd::From);
        assert!(source.iter().any(|c| c.label.contains("不指定接點")));

        let target = connect::choices(&project, env, ConnectionEnd::To);
        assert!(!target.iter().any(|c| c.label.contains("不指定接點")));
    }

    #[test]
    fn a_container_with_several_instances_offers_a_group_choice() {
        let project = healthy_project();
        let env = &project.environments[0]; // prod 有三台 Redis

        let target = connect::choices(&project, env, ConnectionEnd::To);
        let group: Vec<_> = target
            .iter()
            .filter(|c| c.group == "服務（整群）")
            .collect();

        assert!(!group.is_empty());
        assert!(group[0].label.contains("redis-*"));
        assert!(group[0].label.contains("3 台"));
    }

    #[test]
    fn a_single_instance_environment_offers_no_group_choice() {
        // 一台也給「整群」只是多一個一定要想一下的選項。
        let project = healthy_project();
        let env = &project.environments[2]; // dev 只有一台 Redis

        let target = connect::choices(&project, env, ConnectionEnd::To);
        assert!(!target.iter().any(|c| c.group == "服務（整群）"));
    }

    #[test]
    fn infra_and_external_systems_are_both_connectable() {
        let project = healthy_project();
        let env = &project.environments[0];
        let target = connect::choices(&project, env, ConnectionEnd::To);

        assert!(target.iter().any(|c| c.group == "設備"));
        assert!(target.iter().any(|c| c.group == "外部系統"));
    }

    #[test]
    fn a_picked_choice_can_build_a_connection_as_is() {
        // 這是整組的重點：`choices` 給的 `endpointing` 是**不透明值**，
        // 前端原樣帶回來就能用。若這裡要前端再加工，規則就漏到前端了。
        let mut project = healthy_project();
        project.environments[2]
            .connections
            .retain(|c| c.serves != Id::new(REL_CACHE));
        let env = project.environments[2].clone();

        let source = connect::choices(&project, &env, ConnectionEnd::From);
        let target = connect::choices(&project, &env, ConnectionEnd::To);
        let api = source
            .iter()
            .find(|c| c.label.starts_with("api-01（不指定接點）"))
            .expect("找不到 api-01");
        let redis = target
            .iter()
            .find(|c| c.label.starts_with("redis-01 : "))
            .expect("找不到 redis-01");

        edit::apply(
            &mut project,
            &Edit::AddConnection {
                environment: env.id.clone(),
                id: Id::new("conn-手挑的"),
                serves: Id::new(REL_CACHE),
                purpose: "讀寫快取".into(),
                kind: ConnectionKind::Primary,
                from: api.endpointing.clone(),
                to: redis.endpointing.clone(),
            },
        )
        .unwrap();

        assert_eq!(rules(&project), Vec::<Rule>::new());
    }
}

/// 一個服務底下混著不同角色的實體。
///
/// # 這是真的資料長出來的
///
/// Redis 一個 `Container` 底下有 master、replica 與三台 sentinel：
/// 前兩台提供 6379，後三台只提供 26379。**五台都是同一個服務的實體**，
/// 這在模型上完全合法——沒有任何規則說「同一個服務的每一台都要提供
/// 每一個接點」。
///
/// 而擬提案與列選單原本都只問「這個服務有幾台」，沒問「哪幾台有這個接點」。
/// 於是選單寫著「redis-* : redis-tcp（5 台）」，實際上只有 2 台在跑 6379。
/// 挑下去會展開到三台沒有 6379 的機器上，換來三項 L003——
/// **選單先騙了人，lint 才在後面收拾。**
mod mixed_roles {
    use super::*;
    use loom_core::logical::{EndpointDef, Protocol};

    const SENTINEL: &str = "e-redis-sentinel";

    /// prod 的 Redis 從 3 台變成 6 台：3 台原本的（6379）＋ 3 台 sentinel（26379）。
    fn mixed() -> loom_core::Project {
        let mut project = healthy_project();

        project
            .logical
            .containers
            .iter_mut()
            .find(|c| c.id == Id::new(REDIS))
            .expect("找不到 redis")
            .endpoints
            .push(EndpointDef {
                id: Id::new(SENTINEL),
                slug: "sentinel".into(),
                protocol: Protocol::Tcp,
                memo: String::new(),
            });

        let sentinels: Vec<_> = (1..=3)
            .map(|n| {
                let mut i = instance(
                    "prod",
                    &format!("redis-sentinel-{n}"),
                    REDIS,
                    SENTINEL,
                    &format!("10.0.1.2{n}:26379"),
                );
                i.endpoints[0].slug = "sentinel".into();
                i
            })
            .collect();
        project.environments[0].nodes.extend(vms("prod", sentinels));

        project
    }

    /// 目標端那一串裡，屬於「服務（整群）」的那幾項。
    fn groups(project: &loom_core::Project) -> Vec<String> {
        let env = &project.environments[0];
        connect::choices(project, env, ConnectionEnd::To)
            .into_iter()
            .filter(|c| c.group == "服務（整群）")
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn a_group_counts_only_the_instances_that_actually_serve_that_endpoint() {
        // 六台 Redis 實體，但只有三台提供 client-port。
        let project = mixed();

        let client = groups(&project)
            .into_iter()
            .find(|l| l.contains("client-port"))
            .expect("找不到 client-port 的整群選項");

        assert!(
            client.contains("3 台"),
            "數的是全部的實體，不是有這個接點的：{client}"
        );
        assert!(!client.contains("6 台"), "把 sentinel 也算進去了：{client}");
    }

    #[test]
    fn each_role_gets_its_own_pattern() {
        // 兩群的樣式要分得開。`redis-*` 兩群都抓得到，所以它對誰都不對。
        let project = mixed();
        let labels = groups(&project);

        assert!(
            labels
                .iter()
                .any(|l| l.starts_with("redis-0* : client-port")),
            "client-port 那群的樣式不該框到 sentinel：{labels:?}",
        );
        assert!(
            labels
                .iter()
                .any(|l| l.starts_with("redis-sentinel-* : sentinel")),
            "sentinel 那群沒有自己的樣式：{labels:?}",
        );
    }

    #[test]
    fn a_pattern_that_over_matches_says_so_in_the_label() {
        // 有時候真的擠不出剛好的樣式（glob 只有 `*`）。那就**寫在標籤上**，
        // 不要安靜地拿掉——消失的選項跟「工具壞了」長得一樣。
        let mut project = mixed();
        // 把 sentinel 改名成 redis-04／05／06，兩群就再也分不開了：
        // `redis-*` 與 `redis-0*` 都會同時抓到兩群。
        let mut n = 3;
        for node in project.environments[0].nodes.iter_mut() {
            for i in node.instances.iter_mut() {
                if i.slug.starts_with("redis-sentinel-") {
                    n += 1;
                    i.slug = format!("redis-0{n}");
                }
            }
        }

        let client = groups(&project)
            .into_iter()
            .find(|l| l.contains("client-port"))
            .expect("找不到 client-port 的整群選項");

        assert!(client.contains("3 台"), "{client}");
        assert!(
            client.contains("會抓到 6 台"),
            "樣式會多抓，但標籤沒說：{client}"
        );
    }

    #[test]
    fn an_instance_is_not_offered_an_endpoint_it_does_not_have() {
        // 逐台那一串也是同一個病：sentinel 身上沒有 client-port，
        // 列出來只是給使用者一個一送出就變成 L003 的選項。
        let project = mixed();
        let env = &project.environments[0];

        let labels: Vec<String> = connect::choices(&project, env, ConnectionEnd::To)
            .into_iter()
            .map(|c| c.label)
            .collect();

        assert!(
            !labels.iter().any(|l| l == "redis-sentinel-1 : client-port"),
            "sentinel 身上沒有 client-port，不該列：{labels:?}",
        );
        assert!(
            labels.iter().any(|l| l == "redis-sentinel-1 : sentinel"),
            "它自己真的有的那個反而不見了：{labels:?}",
        );
    }

    #[test]
    fn the_proposal_leaves_out_the_instances_without_that_endpoint() {
        let project = mixed();
        let env = &project.environments[0];

        let p = connect::propose(&project, env, &Id::new(REL_CACHE)).expect("擬不出來");
        let Some(Endpointing::Instance {
            target:
                InstanceRef::Pattern {
                    slug_pattern,
                    expect,
                    ..
                },
            ..
        }) = p.to
        else {
            panic!("目標端不是萬用字元：{:?}", p.to)
        };

        assert_eq!(expect, Some(3), "期望數量把 sentinel 也算進去了");
        assert_eq!(slug_pattern, "redis-0*");
    }

    #[test]
    fn the_proposed_connection_is_clean() {
        // 這是整組的重點：照提案建出來的連線**不該**帶著 L003／L004 出生。
        let mut project = mixed();
        project.environments[0]
            .connections
            .retain(|c| c.serves != Id::new(REL_CACHE));
        let env = project.environments[0].id.clone();

        let p = connect::propose(
            &project,
            &project.environments[0].clone(),
            &Id::new(REL_CACHE),
        )
        .expect("擬不出來");
        create(&mut project, &env, &p);

        let broken: Vec<Rule> = rules(&project)
            .into_iter()
            .filter(|r| matches!(r, Rule::L003 | Rule::L004))
            .collect();
        assert_eq!(broken, Vec::<Rule>::new(), "提案自己帶了錯誤出生");
    }

    #[test]
    fn nowhere_to_land_says_which_of_the_two_problems_it_is() {
        // 「一台都沒建」與「建了但沒有那個接點」要修的地方完全不同。
        let mut project = mixed();
        // 三台 6379 全部拔掉那個接點，只留 sentinel。
        for node in project.environments[0].nodes.iter_mut() {
            for i in node.instances.iter_mut() {
                i.endpoints.retain(|e| e.def != Some(Id::new(REDIS_CLIENT)));
            }
        }

        let env = project.environments[0].clone();
        let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).expect("擬不出來");

        assert!(p.to.is_none());
        assert!(
            p.notes.iter().any(|n| n.contains("沒有一台實現接點")),
            "說成「一台都還沒建」會把人送去建機器，而機器是有的：{:?}",
            p.notes,
        );
    }
}
