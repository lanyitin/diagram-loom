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

fn 規則(project: &loom_core::Project) -> Vec<Rule> {
    lint(project).iter().map(|f| f.rule).collect()
}

/// 把提案變成一次新增。使用者按下「建立」時做的就是這件事。
fn 建立(project: &mut loom_core::Project, env: &Id, p: &Proposal) {
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
fn l001_整條契約沒實現_照提案建一條就修好了() {
    let mut project = healthy_project();
    // dev 的快取那條整個拿掉 → L001。
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));
    assert!(規則(&project).contains(&Rule::L001));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();
    assert!(p.is_complete(), "擬不出來：{:?}", p.notes);

    建立(&mut project, &env.id, &p);
    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn l002_走不通_補上缺的那一段就通了() {
    // prod 的快取走兩段：API → F5 → Redis。砍掉第二段就走不通。
    let mut project = healthy_project();
    let 第二段 = project.environments[0].connections[1].clone();
    project.environments[0]
        .connections
        .retain(|c| c.id != 第二段.id);
    assert!(規則(&project).contains(&Rule::L002));

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
            from: 第二段.from,
            to: 第二段.to,
        },
    )
    .unwrap();

    assert_eq!(規則(&project), Vec::<Rule>::new());
}

#[test]
fn 提案沿用契約的用途_不留白() {
    // 留白換來的只是一個 L007，而使用者剛剛才修好一個問題。
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let 契約用途 = &project.logical.relationships[0].purpose;
    assert!(!契約用途.is_empty());
    assert_eq!(&p.purpose, 契約用途);
}

#[test]
fn 多台的時候擬成萬用字元_不是列出每一台() {
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
fn 只有一台的時候就指名那一台() {
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
fn 擬不出來時說清楚為什麼_而不是給一個空表單() {
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
fn 提案會講出它做了什麼假設() {
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
fn 提案先發好_id_所以套用是決定性的() {
    // 若 id 在 apply 裡才產生，預覽算出來的跟真正套用的就是兩份不同的東西，
    // 復原之後重做也會得到一條 id 不一樣的連線。
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();

    let mut 甲 = project.clone();
    建立(&mut 甲, &env.id, &p);
    let mut 乙 = project.clone();
    建立(&mut 乙, &env.id, &p);
    assert_eq!(甲, 乙);
}

#[test]
fn 同一條不會被加兩次() {
    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));

    let env = project.environments[2].clone();
    let p = connect::propose(&project, &env, &Id::new(REL_CACHE)).unwrap();
    建立(&mut project, &env.id, &p);

    let 再一次 = edit::apply(
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
    assert!(再一次.is_err(), "同一個 id 竟然加得進去第二次");
}

#[test]
fn 新增之後可以復原掉() {
    use loom_core::history::History;

    let mut project = healthy_project();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new(REL_CACHE));
    let 原本 = project.clone();

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
    assert_eq!(h.project(), &原本);
    assert_eq!(h.undo_label(), None);
    assert_eq!(h.redo_label(), Some("新增連線"));
}

mod 可以接的地方 {
    use super::*;

    #[test]
    fn 人只出現在來源端() {
        // 人是流量的起點，不會有人「連到一個人」。
        let mut project = healthy_project();
        project.logical.people.push(loom_core::logical::Person {
            id: Id::new("p-客戶"),
            slug: "customer".into(),
            name: "客戶".into(),
        });
        let env = &project.environments[0];

        let 來源 = connect::choices(&project, env, ConnectionEnd::From);
        let 目標 = connect::choices(&project, env, ConnectionEnd::To);

        assert!(來源.iter().any(|c| c.group == "人"));
        assert!(!目標.iter().any(|c| c.group == "人"));
    }

    #[test]
    fn 來源端多一個不指定接點的選項() {
        // 客戶端的 port 通常是作業系統分配的。
        let project = healthy_project();
        let env = &project.environments[0];

        let 來源 = connect::choices(&project, env, ConnectionEnd::From);
        assert!(來源.iter().any(|c| c.label.contains("不指定接點")));

        let 目標 = connect::choices(&project, env, ConnectionEnd::To);
        assert!(!目標.iter().any(|c| c.label.contains("不指定接點")));
    }

    #[test]
    fn 多台的服務會多一個整群的選項() {
        let project = healthy_project();
        let env = &project.environments[0]; // prod 有三台 Redis

        let 目標 = connect::choices(&project, env, ConnectionEnd::To);
        let 整群: Vec<_> = 目標.iter().filter(|c| c.group == "服務（整群）").collect();

        assert!(!整群.is_empty());
        assert!(整群[0].label.contains("redis-*"));
        assert!(整群[0].label.contains("3 台"));
    }

    #[test]
    fn 只有一台的環境不提供整群的選項() {
        // 一台也給「整群」只是多一個一定要想一下的選項。
        let project = healthy_project();
        let env = &project.environments[2]; // dev 只有一台 Redis

        let 目標 = connect::choices(&project, env, ConnectionEnd::To);
        assert!(!目標.iter().any(|c| c.group == "服務（整群）"));
    }

    #[test]
    fn 設備與外部系統都接得上() {
        let project = healthy_project();
        let env = &project.environments[0];
        let 目標 = connect::choices(&project, env, ConnectionEnd::To);

        assert!(目標.iter().any(|c| c.group == "設備"));
        assert!(目標.iter().any(|c| c.group == "外部系統"));
    }

    #[test]
    fn 挑出來的東西可以直接拿去建連線() {
        // 這是整組的重點：`choices` 給的 `endpointing` 是**不透明值**，
        // 前端原樣帶回來就能用。若這裡要前端再加工，規則就漏到前端了。
        let mut project = healthy_project();
        project.environments[2]
            .connections
            .retain(|c| c.serves != Id::new(REL_CACHE));
        let env = project.environments[2].clone();

        let 來源 = connect::choices(&project, &env, ConnectionEnd::From);
        let 目標 = connect::choices(&project, &env, ConnectionEnd::To);
        let api = 來源
            .iter()
            .find(|c| c.label.starts_with("api-01（不指定接點）"))
            .expect("找不到 api-01");
        let redis = 目標
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

        assert_eq!(規則(&project), Vec::<Rule>::new());
    }
}
