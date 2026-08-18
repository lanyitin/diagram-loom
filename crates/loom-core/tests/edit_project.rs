//! 編輯的整合測試。
//!
//! # 這裡真正在守的東西
//!
//! 每一項 lint 發現都必須**修得掉**。若一條規則會叫，卻沒有任何編輯能讓它
//! 閉嘴，使用者就只能學會忽略它——而一旦養成忽略 lint 的習慣，
//! 這個工具就沒有價值了。
//!
//! 所以測試的形狀一律是：**弄壞 → 用編輯修 → lint 乾淨**。
//! 只斷言「欄位變了」是不夠的，那不代表使用者的問題解決了。

mod common;

use common::*;
use loom_core::edit::{self, Edit, EditError, Fix, FixValue};
use loom_core::environment::{ConnectionEnd, ConnectionKind, Endpointing, InstanceRef};
use loom_core::history::History;
use loom_core::id::Id;
use loom_core::lint::{Rule, lint};

fn rules(project: &loom_core::Project) -> Vec<Rule> {
    lint(project).iter().map(|f| f.rule).collect()
}

const PROD: &str = "env-prod";
const DEV: &str = "env-dev";

#[test]
fn l006_missing_address_is_fixed_by_one_edit() {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let endpoint = dev.nodes[1].instances[0].endpoints[0].id.clone();

    assert_eq!(rules(&project), vec![Rule::L006]);

    edit::apply(
        &mut project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint,
            address: Some("10.9.9.9:6379".into()),
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn a_blank_address_does_not_count_as_filled() {
    // 否則使用者按個空白鍵就把 L006 騙過去了，而缺漏正是這工具要抓的東西。
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let endpoint = dev.nodes[1].instances[0].endpoints[0].id.clone();

    edit::apply(
        &mut project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint,
            address: Some("   ".into()),
        },
    )
    .unwrap();

    assert_eq!(rules(&project), vec![Rule::L006], "空白被當成填好了");
}

#[test]
fn l007_missing_purpose_is_fixable_in_both_layers() {
    let mut project = healthy_project();
    project.environments[2].connections[0].purpose = "  ".into();
    project.logical.relationships[0].purpose = String::new();

    assert_eq!(rules(&project), vec![Rule::L007, Rule::L007]);

    let connection = project.environments[2].connections[0].id.clone();
    edit::apply(
        &mut project,
        &Edit::SetPurpose {
            environment: Some(Id::new(DEV)),
            subject: connection,
            purpose: "查快取".into(),
        },
    )
    .unwrap();

    let relationship_id = project.logical.relationships[0].id.clone();
    edit::apply(
        &mut project,
        &Edit::SetPurpose {
            environment: None,
            subject: relationship_id,
            purpose: "訂單服務讀寫快取".into(),
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn l005_wildcard_without_expect_can_be_filled_in() {
    let mut project = healthy_project();
    let prod = &mut project.environments[0];
    if let Endpointing::Instance { target, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None;
    }
    let connection = prod.connections[1].id.clone();

    assert_eq!(rules(&project), vec![Rule::L005]);

    edit::apply(
        &mut project,
        &Edit::SetExpect {
            environment: Id::new(PROD),
            connection,
            side: ConnectionEnd::To,
            expect: Some(3),
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn setting_expect_on_a_non_wildcard_end_errors() {
    // 安靜地不做事會讓使用者以為填好了。寧可讓他看到一句話。
    let mut project = healthy_project();
    let connection = project.environments[0].connections[1].id.clone();

    let err = edit::apply(
        &mut project,
        &Edit::SetExpect {
            environment: Id::new(PROD),
            connection: connection.clone(),
            side: ConnectionEnd::From,
            expect: Some(3),
        },
    );

    assert_eq!(
        err,
        Err(EditError::NotAPattern {
            connection,
            side: ConnectionEnd::From
        })
    );
}

#[test]
fn l008_marking_a_standby_standalone_silences_it() {
    let mut project = healthy_project();
    {
        let dev = &mut project.environments[2];
        let mut standby = dev.nodes[1].instances[0].clone();
        standby.id = Id::new("i-dev-redis-備援");
        standby.slug = "redis-standby".into();
        dev.nodes[0].instances.push(standby);
    }
    assert_eq!(rules(&project), vec![Rule::L008]);

    edit::apply(
        &mut project,
        &Edit::SetStandalone {
            environment: Id::new(DEV),
            subject: Id::new("i-dev-redis-備援"),
            standalone: true,
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn a_missing_target_leaves_the_project_untouched() {
    let mut project = healthy_project();
    let original = project.clone();

    for bogus in [
        Edit::SetAddress {
            environment: Id::new("env-根本沒這個"),
            endpoint: Id::new("x"),
            address: Some("1.2.3.4".into()),
        },
        Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint: Id::new("ep-沒這個"),
            address: Some("1.2.3.4".into()),
        },
        Edit::SetStandalone {
            environment: Id::new(DEV),
            subject: Id::new("i-沒這台"),
            standalone: true,
        },
        Edit::DeleteConnection {
            environment: Id::new(DEV),
            connection: Id::new("conn-沒這條"),
        },
    ] {
        assert!(
            edit::apply(&mut project, &bogus).is_err(),
            "{bogus:?} 竟然成功了"
        );
        assert_eq!(project, original, "{bogus:?} 失敗了卻留下痕跡");
    }
}

#[test]
fn deleting_a_connection_shows_what_it_will_break() {
    // prod 的快取走兩段：API → F5 → Redis。砍掉第二段，
    // 剩下的每一條單看都合法——只有整條路徑串起來才知道到不了。
    // 這正是使用者按下刪除之前必須被告知的事。
    let project = healthy_project();
    let second_hop = project.environments[0].connections[1].id.clone();

    let impact = edit::preview(
        &project,
        &Edit::DeleteConnection {
            environment: Id::new(PROD),
            connection: second_hop,
        },
    )
    .unwrap();

    assert!(!impact.is_safe(), "刪掉整條路徑的中段卻說沒事");
    let broken: Vec<Rule> = impact.introduced.iter().map(|f| f.rule).collect();
    assert!(
        broken.contains(&Rule::L002),
        "應該要說走不通了，實際說的是：{:?}",
        impact.introduced
    );
    // 三台 Redis 頓時沒人碰，也該一起說出來。
    assert_eq!(broken.iter().filter(|r| **r == Rule::L008).count(), 3);
    assert!(impact.resolved.is_empty());

    // 預覽不動原件。
    assert_eq!(project.environments[0].connections.len(), 3);
}

#[test]
fn preview_reports_a_finding_as_resolved() {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let endpoint = dev.nodes[1].instances[0].endpoints[0].id.clone();

    let impact = edit::preview(
        &project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint,
            address: Some("10.9.9.9:6379".into()),
        },
    )
    .unwrap();

    assert!(impact.is_safe());
    assert_eq!(
        impact.resolved.iter().map(|f| f.rule).collect::<Vec<_>>(),
        vec![Rule::L006]
    );
}

#[test]
fn every_finding_that_fires_has_a_fix() {
    // 這是本檔案的重點。L001／L002／L003 沒有單欄位修法是刻意的
    // （它們要新增或改接連線，屬於另一個層級的操作），除此之外
    // 每一條規則都必須交得出一個 Fix，否則使用者只能學會忽略它。
    let mut project = healthy_project();
    {
        let dev = &mut project.environments[2];
        dev.nodes[1].instances[0].endpoints[0].address = None; // L006
        dev.connections[0].purpose = String::new(); // L007
        let mut standby = dev.nodes[1].instances[0].clone();
        standby.id = Id::new("i-dev-孤兒");
        standby.slug = "redis-standby".into();
        dev.nodes[0].instances.push(standby); // L008
    }
    if let Endpointing::Instance { target, .. } = &mut project.environments[0].connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None; // L005
    }

    let findings = lint(&project);
    assert!(findings.len() >= 4);

    for f in &findings {
        let fix = edit::fix_for(&project, f);
        match f.rule {
            // L003 是「指到了不存在的東西」，要改既有連線的接法，還沒做。
            Rule::L003 => assert!(fix.is_none(), "L003 目前不該有修法"),
            _ => assert!(fix.is_some(), "{f:?} 會叫，卻沒有任何修法"),
        }
    }
}

#[test]
fn filling_in_each_fix_cleans_a_broken_project() {
    // 這是整個階段 5 的驗收：模擬前端只會做兩件事——
    // ① 照 `Fix` 長出控制項 ② 把使用者填的值原封不動送回來。
    // 它**完全不知道** Edit 有哪些變體、哪條規則對應哪個欄位。
    //
    // 如果哪天新增一條規則卻忘了給它修法，這個測試會停在原地跑不完。
    let mut project = healthy_project();
    {
        let dev = &mut project.environments[2];

        // 先複製一台當孤兒，而且給它自己的 endpoint id——
        // 沿用原本的 id 會讓等一下的「清空位址」同時打到兩台，多出一項 L006。
        let mut standby = dev.nodes[1].instances[0].clone();
        standby.id = Id::new("i-dev-孤兒");
        standby.slug = "redis-standby".into();
        for ep in &mut standby.endpoints {
            ep.id = Id::new(format!("{}-孤兒", ep.id));
        }
        dev.nodes[0].instances.push(standby); // L008

        dev.nodes[1].instances[0].endpoints[0].address = None; // L006
        dev.connections[0].purpose = String::new(); // L007
    }
    project.logical.relationships[0].purpose = String::new(); // 邏輯層的 L007
    if let Endpointing::Instance { target, .. } = &mut project.environments[0].connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None; // L005
    }
    assert_eq!(lint(&project).len(), 5);

    // 一次修一項，每次都重跑 lint——就跟使用者在畫面上做的一樣。
    let mut fixes = 0;
    while let Some(f) = lint(&project)
        .into_iter()
        .find(|f| !matches!(f.rule, Rule::L001 | Rule::L002 | Rule::L003))
    {
        let value = match edit::fix_for(&project, &f).expect("會叫卻沒有修法") {
            Fix::Text { .. } => FixValue::Text("補上去了".into()),
            Fix::Count { suggestion } => FixValue::Count(suggestion),
            Fix::Toggle { .. } => FixValue::Toggle(true),
            // 這幾種不是「填一格」，不在這個迴圈裡。
            Fix::AddConnection { .. } | Fix::AddInstances { .. } | Fix::AddResource { .. } => {
                unreachable!("這份素材不該有 L001")
            }
            Fix::Manual { hint } => unreachable!("這份素材不該有只能手動處理的發現：{hint}"),
        };
        let e = edit::edit_for(&f, &value).expect("交不出 Edit");
        edit::apply(&mut project, &e).expect("套用失敗");

        fixes += 1;
        assert!(
            fixes <= 10,
            "修了十次還沒收斂，多半是某個修法沒真的解掉問題"
        );
    }

    assert_eq!(fixes, 5);
    assert_eq!(rules(&project), Vec::<Rule>::new());
}

#[test]
fn a_value_of_the_wrong_type_is_rejected() {
    // 前端如果把數字送進位址欄，該看到一句話，而不是安靜地不作用。
    let mut project = healthy_project();
    project.environments[2].nodes[1].instances[0].endpoints[0].address = None;
    let f = lint(&project).into_iter().next().unwrap();

    assert!(edit::edit_for(&f, &FixValue::Count(Some(3))).is_err());
}

#[test]
fn the_expect_fix_suggests_the_actual_count() {
    let mut project = healthy_project();
    if let Endpointing::Instance { target, .. } = &mut project.environments[0].connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = Some(99);
    }

    let f = lint(&project)
        .into_iter()
        .find(|f| f.rule == Rule::L004)
        .unwrap();
    // prod 有三台 Redis，所以輸入框該預帶 3，而不是叫使用者自己數。
    assert_eq!(
        edit::fix_for(&project, &f),
        Some(Fix::Count {
            suggestion: Some(3)
        })
    );
}

#[test]
fn the_address_fix_carries_the_current_value() {
    let project = healthy_project();
    let endpoint = project.environments[2].nodes[1].instances[0].endpoints[0].clone();
    let f = loom_core::lint::Finding {
        rule: Rule::L006,
        environment: Some(Id::new(DEV)),
        subject: endpoint.id.clone(),
        end: None,
        detail: String::new(),
    };

    let Some(Fix::Text { current, .. }) = edit::fix_for(&project, &f) else {
        panic!("L006 應該是填一段文字");
    };
    assert_eq!(current, endpoint.address);
}

#[test]
fn marking_a_path_fallback_changes_presentation_not_lint() {
    // 備援一樣要建立、防火牆一樣要開，所以 lint 的要求完全相同。
    let mut project = healthy_project();
    let connection = project.environments[2].connections[0].id.clone();

    edit::apply(
        &mut project,
        &Edit::SetConnectionKind {
            environment: Id::new(DEV),
            connection,
            kind: ConnectionKind::Fallback,
        },
    )
    .unwrap();

    assert_eq!(rules(&project), Vec::<Rule>::new());
    assert_eq!(
        project.environments[2].connections[0].kind,
        ConnectionKind::Fallback
    );
}

#[test]
fn a_connection_memo_is_saved_without_silencing_l007() {
    // 備註與用途是刻意分開的兩個欄位：L007 在看 `purpose`，而備註是規則
    // 管不到的話。寫進同一個欄位的話，「等年底汰換」會讓 L007 從此不再叫——
    // 那是使用者最不會發現的一種失效。
    let mut project = healthy_project();
    let connection = project.environments[2].connections[0].id.clone();
    project.environments[2].connections[0].purpose = String::new();

    edit::apply(
        &mut project,
        &Edit::SetConnectionMemo {
            environment: Id::new(DEV),
            connection,
            memo: "等年底汰換".into(),
        },
    )
    .unwrap();

    assert_eq!(project.environments[2].connections[0].memo, "等年底汰換");
    assert!(project.environments[2].connections[0].purpose.is_empty());
    assert!(rules(&project).contains(&Rule::L007), "備註把 L007 蓋掉了");
}

#[test]
fn a_connection_memo_can_be_cleared() {
    // 備註沒有規則在看，所以寫錯了沒有第二條路可以救。清得掉是必要的。
    let mut project = healthy_project();
    let connection = project.environments[2].connections[0].id.clone();
    project.environments[2].connections[0].memo = "寫錯了".into();

    edit::apply(
        &mut project,
        &Edit::SetConnectionMemo {
            environment: Id::new(DEV),
            connection,
            memo: String::new(),
        },
    )
    .unwrap();

    assert!(project.environments[2].connections[0].memo.is_empty());
}

#[test]
fn a_deletion_can_be_undone() {
    // 刪除是唯一會讓資料消失的操作，所以它跟復原必須是同一個故事。
    let mut h = History::opened(healthy_project());
    let connection = h.project().environments[0].connections[1].id.clone();

    h.edit(&Edit::DeleteConnection {
        environment: Id::new(PROD),
        connection,
    })
    .unwrap();

    assert_eq!(h.project().environments[0].connections.len(), 2);
    assert!(!lint(h.project()).is_empty(), "刪掉中段卻沒人抱怨");

    assert!(h.undo());
    assert_eq!(h.project().environments[0].connections.len(), 3);
    assert!(lint(h.project()).is_empty(), "復原之後專案沒有回到乾淨狀態");
    assert!(!h.is_dirty());
}

/// 兩端都是萬用字元的連線。
///
/// 真實樣本裡的 `apigw-* → common-*` 就是這個形狀，而它揭露了一個
/// 會靜靜出錯的 bug：發現沒說是哪一端，`edit_for` 只好挑「第一個萬用字元」，
/// 於是修目標端的問題卻改到了來源端。使用者按下套用，錯誤還在，
/// 而且因為來源端的值沒變、專案沒被改動，連存檔鍵都不會亮。
mod wildcards_on_both_ends {
    use super::*;
    use loom_core::environment::{Connection, ConnectionKind, Endpointing, InstanceRef};

    /// prod：`api-* (1 台) → redis-* (3 台)`，兩端都寫萬用字元。
    fn project(from_expect: u32, to_expect: u32) -> loom_core::Project {
        let mut project = healthy_project();
        let prod = &mut project.environments[0];
        // 只換掉快取那條（原本走 F5 分成兩段），金流那條留著，
        // 否則會冒出一堆跟這組測試無關的 L001／L008。
        prod.connections.retain(|c| c.serves == Id::new(REL_PAY));
        prod.connections.push(Connection {
            id: Id::new("conn-兩端"),
            serves: Id::new(REL_CACHE),
            purpose: "讀寫快取".into(),
            kind: ConnectionKind::Primary,
            from: Endpointing::Instance {
                target: InstanceRef::Pattern {
                    slug_pattern: "api-*".into(),
                    within: None,
                    expect: Some(from_expect),
                },
                endpoint: None,
            },
            to: Endpointing::Instance {
                target: InstanceRef::Pattern {
                    slug_pattern: "redis-*".into(),
                    within: None,
                    expect: Some(to_expect),
                },
                endpoint: Some(Id::new(REDIS_CLIENT)),
            },
            memo: String::new(),
        });
        project
    }

    fn expects(project: &loom_core::Project) -> (Option<u32>, Option<u32>) {
        let conn = project.environments[0]
            .connections
            .iter()
            .find(|c| c.id == Id::new("conn-兩端"))
            .expect("那條連線不見了");
        let read = |side: &Endpointing| match side {
            Endpointing::Instance {
                target: InstanceRef::Pattern { expect, .. },
                ..
            } => *expect,
            _ => None,
        };
        (read(&conn.from), read(&conn.to))
    }

    #[test]
    fn a_finding_names_which_end() {
        // 來源寫 9（實際 1 台）、目標寫 9（實際 3 台）→ 兩項 L004。
        // 沒有 end 的話這兩項的 rule 與 subject 完全一樣，分不出誰是誰。
        let project = project(9, 9);
        let pair: Vec<_> = lint(&project)
            .into_iter()
            .filter(|f| f.rule == Rule::L004)
            .collect();

        assert_eq!(pair.len(), 2);
        assert_eq!(pair[0].end, Some(ConnectionEnd::From));
        assert_eq!(pair[1].end, Some(ConnectionEnd::To));
    }

    #[test]
    fn fixing_the_target_side_does_not_touch_the_source_side() {
        // 來源端本來就對（1 台寫 1），只有目標端錯（3 台卻寫 9）。
        let mut project = project(1, 9);
        let f = lint(&project)
            .into_iter()
            .find(|f| f.rule == Rule::L004)
            .unwrap();
        assert_eq!(f.end, Some(ConnectionEnd::To));

        let e = edit::edit_for(&f, &FixValue::Count(Some(3))).unwrap();
        edit::apply(&mut project, &e).unwrap();

        assert_eq!(expects(&project), (Some(1), Some(3)), "改到了另一端");
        assert_eq!(rules(&project), Vec::<Rule>::new());
    }

    #[test]
    fn the_suggested_count_belongs_to_that_end() {
        // 來源 1 台、目標 3 台。修目標端時輸入框該預帶 3，不是 1——
        // 預帶 1 的話使用者按下套用反而製造出一個新的 L004。
        let project = project(1, 9);
        let f = lint(&project)
            .into_iter()
            .find(|f| f.rule == Rule::L004)
            .unwrap();

        assert_eq!(
            edit::fix_for(&project, &f),
            Some(Fix::Count {
                suggestion: Some(3)
            })
        );
    }

    #[test]
    fn when_both_ends_are_wrong_each_is_fixed_separately() {
        let mut project = project(9, 9);
        for _ in 0..2 {
            let f = lint(&project)
                .into_iter()
                .find(|f| f.rule == Rule::L004)
                .unwrap();
            let Some(Fix::Count { suggestion }) = edit::fix_for(&project, &f) else {
                panic!("L004 應該是填一個數字");
            };
            let e = edit::edit_for(&f, &FixValue::Count(suggestion)).unwrap();
            edit::apply(&mut project, &e).unwrap();
        }

        assert_eq!(expects(&project), (Some(1), Some(3)));
        assert_eq!(rules(&project), Vec::<Rule>::new());
    }
}

/// 修改**不可以偷偷丟掉欄位**。
///
/// # 這一組守的是什麼
///
/// `write_into` 對某些資源是逐欄複製，那是刻意的：環境與機器底下掛著
/// 巢狀的子結構（機器、連線、子節點），整份換掉會把它們洗掉。
///
/// 但逐欄複製有個安靜的失敗模式——**漏抄一個欄位**。漏掉的那一欄每次
/// 修改都被丟回預設值，而畫面上看起來完全正常：使用者填了、按了儲存、
/// 沒有任何錯誤，只是東西沒進去。
///
/// 外部系統實體真的踩過：`endpoints` 被漏掉，於是「位址」這個欄位
/// **從畫面到 MCP 都存不進去**，而 lint 一直說沒有位址。
mod updating_keeps_every_field {
    use super::*;
    use loom_core::environment::Endpoint;
    use loom_core::logical::Protocol;
    use loom_core::resource::Resource;

    #[test]
    fn an_external_system_instance_keeps_its_address() {
        let mut project = healthy_project();
        let env = project.environments[0].clone();
        let mut instance = env.systems[0].clone();

        instance.endpoints = vec![Endpoint {
            id: Id::new("ep-新的"),
            slug: "https".into(),
            def: None,
            protocol: Protocol::Tcp,
            address: Some("pay.example.com:443".into()),
            memo: String::new(),
        }];

        edit::apply(
            &mut project,
            &Edit::UpdateResource(Resource::SystemInstance {
                environment: env.id.clone(),
                instance,
            }),
        )
        .expect("套用失敗");

        let after = &project.environments[0].systems[0];
        assert_eq!(
            after
                .endpoints
                .iter()
                .filter_map(|e| e.address.as_deref())
                .collect::<Vec<_>>(),
            ["pay.example.com:443"],
            "位址被丟掉了——逐欄複製漏抄 endpoints",
        );
    }

    #[test]
    fn renaming_it_does_not_wipe_the_address() {
        // 更陰險的版本：使用者只是改個名字，位址卻不見了。
        let mut project = healthy_project();
        let env = project.environments[0].clone();
        let before: Vec<String> = env.systems[0]
            .endpoints
            .iter()
            .filter_map(|e| e.address.clone())
            .collect();
        assert!(!before.is_empty(), "這份素材本來就該有位址");

        let mut instance = env.systems[0].clone();
        instance.slug = "改個名字".into();

        edit::apply(
            &mut project,
            &Edit::UpdateResource(Resource::SystemInstance {
                environment: env.id.clone(),
                instance,
            }),
        )
        .expect("套用失敗");

        let after: Vec<String> = project.environments[0].systems[0]
            .endpoints
            .iter()
            .filter_map(|e| e.address.clone())
            .collect();
        assert_eq!(after, before);
    }
}
