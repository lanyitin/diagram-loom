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

fn 規則(project: &loom_core::Project) -> Vec<Rule> {
    lint(project).iter().map(|f| f.rule).collect()
}

const PROD: &str = "env-prod";
const DEV: &str = "env-dev";

#[test]
fn l006_缺位址_可以用一次編輯修好() {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let 端點 = dev.nodes[1].instances[0].endpoints[0].id.clone();

    assert_eq!(規則(&project), vec![Rule::L006]);

    edit::apply(
        &mut project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint: 端點,
            address: Some("10.9.9.9:6379".into()),
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn 只填空白的位址不算填了() {
    // 否則使用者按個空白鍵就把 L006 騙過去了，而缺漏正是這工具要抓的東西。
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let 端點 = dev.nodes[1].instances[0].endpoints[0].id.clone();

    edit::apply(
        &mut project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint: 端點,
            address: Some("   ".into()),
        },
    )
    .unwrap();

    assert_eq!(規則(&project), vec![Rule::L006], "空白被當成填好了");
}

#[test]
fn l007_沒填用途_環境層與邏輯層都修得掉() {
    let mut project = healthy_project();
    project.environments[2].connections[0].purpose = "  ".into();
    project.logical.relationships[0].purpose = String::new();

    assert_eq!(規則(&project), vec![Rule::L007, Rule::L007]);

    let 連線 = project.environments[2].connections[0].id.clone();
    edit::apply(
        &mut project,
        &Edit::SetPurpose {
            environment: Some(Id::new(DEV)),
            subject: 連線,
            purpose: "查快取".into(),
        },
    )
    .unwrap();

    let 契約 = project.logical.relationships[0].id.clone();
    edit::apply(
        &mut project,
        &Edit::SetPurpose {
            environment: None,
            subject: 契約,
            purpose: "訂單服務讀寫快取".into(),
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn l005_萬用字元沒填期望數量_可以補上() {
    let mut project = healthy_project();
    let prod = &mut project.environments[0];
    if let Endpointing::Instance { target, .. } = &mut prod.connections[1].to
        && let InstanceRef::Pattern { expect, .. } = target
    {
        *expect = None;
    }
    let 連線 = prod.connections[1].id.clone();

    assert_eq!(規則(&project), vec![Rule::L005]);

    edit::apply(
        &mut project,
        &Edit::SetExpect {
            environment: Id::new(PROD),
            connection: 連線,
            side: ConnectionEnd::To,
            expect: Some(3),
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn 對不是萬用字元的那一端設期望數量會報錯() {
    // 安靜地不做事會讓使用者以為填好了。寧可讓他看到一句話。
    let mut project = healthy_project();
    let 連線 = project.environments[0].connections[1].id.clone();

    let err = edit::apply(
        &mut project,
        &Edit::SetExpect {
            environment: Id::new(PROD),
            connection: 連線.clone(),
            side: ConnectionEnd::From,
            expect: Some(3),
        },
    );

    assert_eq!(
        err,
        Err(EditError::NotAPattern {
            connection: 連線,
            side: ConnectionEnd::From
        })
    );
}

#[test]
fn l008_冷備機標記刻意獨立就安靜了() {
    let mut project = healthy_project();
    {
        let dev = &mut project.environments[2];
        let mut 冷備 = dev.nodes[1].instances[0].clone();
        冷備.id = Id::new("i-dev-redis-備援");
        冷備.slug = "redis-standby".into();
        dev.nodes[0].instances.push(冷備);
    }
    assert_eq!(規則(&project), vec![Rule::L008]);

    edit::apply(
        &mut project,
        &Edit::SetStandalone {
            environment: Id::new(DEV),
            subject: Id::new("i-dev-redis-備援"),
            standalone: true,
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn 找不到目標時專案完全沒被動過() {
    let mut project = healthy_project();
    let 原本 = project.clone();

    for 亂改 in [
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
            edit::apply(&mut project, &亂改).is_err(),
            "{亂改:?} 竟然成功了"
        );
        assert_eq!(project, 原本, "{亂改:?} 失敗了卻留下痕跡");
    }
}

#[test]
fn 刪連線之前看得到會弄壞什麼() {
    // prod 的快取走兩段：API → F5 → Redis。砍掉第二段，
    // 剩下的每一條單看都合法——只有整條路徑串起來才知道到不了。
    // 這正是使用者按下刪除之前必須被告知的事。
    let project = healthy_project();
    let 第二段 = project.environments[0].connections[1].id.clone();

    let impact = edit::preview(
        &project,
        &Edit::DeleteConnection {
            environment: Id::new(PROD),
            connection: 第二段,
        },
    )
    .unwrap();

    assert!(!impact.is_safe(), "刪掉整條路徑的中段卻說沒事");
    let 弄壞的: Vec<Rule> = impact.introduced.iter().map(|f| f.rule).collect();
    assert!(
        弄壞的.contains(&Rule::L002),
        "應該要說走不通了，實際說的是：{:?}",
        impact.introduced
    );
    // 三台 Redis 頓時沒人碰，也該一起說出來。
    assert_eq!(弄壞的.iter().filter(|r| **r == Rule::L008).count(), 3);
    assert!(impact.resolved.is_empty());

    // 預覽不動原件。
    assert_eq!(project.environments[0].connections.len(), 3);
}

#[test]
fn 修好一項問題時預覽會說它被解掉了() {
    let mut project = healthy_project();
    let dev = &mut project.environments[2];
    dev.nodes[1].instances[0].endpoints[0].address = None;
    let 端點 = dev.nodes[1].instances[0].endpoints[0].id.clone();

    let impact = edit::preview(
        &project,
        &Edit::SetAddress {
            environment: Id::new(DEV),
            endpoint: 端點,
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
fn 每一項會叫的發現都要有辦法修() {
    // 這是本檔案的重點。L001／L002／L003 沒有單欄位修法是刻意的
    // （它們要新增或改接連線，屬於另一個層級的操作），除此之外
    // 每一條規則都必須交得出一個 Fix，否則使用者只能學會忽略它。
    let mut project = healthy_project();
    {
        let dev = &mut project.environments[2];
        dev.nodes[1].instances[0].endpoints[0].address = None; // L006
        dev.connections[0].purpose = String::new(); // L007
        let mut 冷備 = dev.nodes[1].instances[0].clone();
        冷備.id = Id::new("i-dev-孤兒");
        冷備.slug = "redis-standby".into();
        dev.nodes[0].instances.push(冷備); // L008
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
fn 照著修法填一格_就能把一個壞掉的專案修乾淨() {
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
        let mut 冷備 = dev.nodes[1].instances[0].clone();
        冷備.id = Id::new("i-dev-孤兒");
        冷備.slug = "redis-standby".into();
        for ep in &mut 冷備.endpoints {
            ep.id = Id::new(format!("{}-孤兒", ep.id));
        }
        dev.nodes[0].instances.push(冷備); // L008

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
    let mut 修了 = 0;
    while let Some(f) = lint(&project)
        .into_iter()
        .find(|f| !matches!(f.rule, Rule::L001 | Rule::L002 | Rule::L003))
    {
        let 填什麼 = match edit::fix_for(&project, &f).expect("會叫卻沒有修法") {
            Fix::Text { .. } => FixValue::Text("補上去了".into()),
            Fix::Count { suggestion } => FixValue::Count(suggestion),
            Fix::Toggle { .. } => FixValue::Toggle(true),
            // 這兩種要開表單，不在這個「填一格」的迴圈裡。
            Fix::AddConnection { .. } | Fix::AddInstances { .. } => {
                unreachable!("這份素材不該有 L001")
            }
        };
        let e = edit::edit_for(&f, &填什麼).expect("交不出 Edit");
        edit::apply(&mut project, &e).expect("套用失敗");

        修了 += 1;
        assert!(修了 <= 10, "修了十次還沒收斂，多半是某個修法沒真的解掉問題");
    }

    assert_eq!(修了, 5);
    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn 拿錯型別的值去填會被擋下來() {
    // 前端如果把數字送進位址欄，該看到一句話，而不是安靜地不作用。
    let mut project = healthy_project();
    project.environments[2].nodes[1].instances[0].endpoints[0].address = None;
    let f = lint(&project).into_iter().next().unwrap();

    assert!(edit::edit_for(&f, &FixValue::Count(Some(3))).is_err());
}

#[test]
fn 期望數量的修法會建議實際符合的數字() {
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
fn 缺位址的修法會帶出目前的值() {
    let project = healthy_project();
    let 端點 = project.environments[2].nodes[1].instances[0].endpoints[0].clone();
    let f = loom_core::lint::Finding {
        rule: Rule::L006,
        environment: Some(Id::new(DEV)),
        subject: 端點.id.clone(),
        end: None,
        detail: String::new(),
    };

    let Some(Fix::Text { current, .. }) = edit::fix_for(&project, &f) else {
        panic!("L006 應該是填一段文字");
    };
    assert_eq!(current, 端點.address);
}

#[test]
fn 改成備援路徑不影響lint只影響呈現() {
    // 備援一樣要建立、防火牆一樣要開，所以 lint 的要求完全相同。
    let mut project = healthy_project();
    let 連線 = project.environments[2].connections[0].id.clone();

    edit::apply(
        &mut project,
        &Edit::SetConnectionKind {
            environment: Id::new(DEV),
            connection: 連線,
            kind: ConnectionKind::Fallback,
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
    assert_eq!(
        project.environments[2].connections[0].kind,
        ConnectionKind::Fallback
    );
}

#[test]
fn 刪掉之後可以復原回來() {
    // 刪除是唯一會讓資料消失的操作，所以它跟復原必須是同一個故事。
    let mut h = History::opened(healthy_project());
    let 連線 = h.project().environments[0].connections[1].id.clone();

    h.edit(&Edit::DeleteConnection {
        environment: Id::new(PROD),
        connection: 連線,
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
mod 兩端都是萬用字元 {
    use super::*;
    use loom_core::environment::{Connection, ConnectionKind, Endpointing, InstanceRef};

    /// prod：`api-* (1 台) → redis-* (3 台)`，兩端都寫萬用字元。
    fn 專案(來源期望: u32, 目標期望: u32) -> loom_core::Project {
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
                    expect: Some(來源期望),
                },
                endpoint: None,
            },
            to: Endpointing::Instance {
                target: InstanceRef::Pattern {
                    slug_pattern: "redis-*".into(),
                    within: None,
                    expect: Some(目標期望),
                },
                endpoint: Some(Id::new(REDIS_CLIENT)),
            },
        });
        project
    }

    fn 期望值(project: &loom_core::Project) -> (Option<u32>, Option<u32>) {
        let conn = project.environments[0]
            .connections
            .iter()
            .find(|c| c.id == Id::new("conn-兩端"))
            .expect("那條連線不見了");
        let 拿 = |side: &Endpointing| match side {
            Endpointing::Instance {
                target: InstanceRef::Pattern { expect, .. },
                ..
            } => *expect,
            _ => None,
        };
        (拿(&conn.from), 拿(&conn.to))
    }

    #[test]
    fn 發現會指名是哪一端() {
        // 來源寫 9（實際 1 台）、目標寫 9（實際 3 台）→ 兩項 L004。
        // 沒有 end 的話這兩項的 rule 與 subject 完全一樣，分不出誰是誰。
        let project = 專案(9, 9);
        let 兩項: Vec<_> = lint(&project)
            .into_iter()
            .filter(|f| f.rule == Rule::L004)
            .collect();

        assert_eq!(兩項.len(), 2);
        assert_eq!(兩項[0].end, Some(ConnectionEnd::From));
        assert_eq!(兩項[1].end, Some(ConnectionEnd::To));
    }

    #[test]
    fn 修目標端的問題不會改到來源端() {
        // 來源端本來就對（1 台寫 1），只有目標端錯（3 台卻寫 9）。
        let mut project = 專案(1, 9);
        let f = lint(&project)
            .into_iter()
            .find(|f| f.rule == Rule::L004)
            .unwrap();
        assert_eq!(f.end, Some(ConnectionEnd::To));

        let e = edit::edit_for(&f, &FixValue::Count(Some(3))).unwrap();
        edit::apply(&mut project, &e).unwrap();

        assert_eq!(期望值(&project), (Some(1), Some(3)), "改到了另一端");
        assert_eq!(規則(&project), Vec::<Rule>::new());
    }

    #[test]
    fn 建議的數字是那一端自己的數字() {
        // 來源 1 台、目標 3 台。修目標端時輸入框該預帶 3，不是 1——
        // 預帶 1 的話使用者按下套用反而製造出一個新的 L004。
        let project = 專案(1, 9);
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
    fn 兩端都錯時各修各的() {
        let mut project = 專案(9, 9);
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

        assert_eq!(期望值(&project), (Some(1), Some(3)));
        assert_eq!(規則(&project), Vec::<Rule>::new());
    }
}
