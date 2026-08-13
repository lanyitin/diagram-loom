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

const 使用者: &str = "p-使用者";

/// 在健康的專案上加一條「使用者 → 訂單 API」，三個環境都接好。
fn 有使用者的專案() -> loom_core::Project {
    let mut project = healthy_project();

    project.logical.people.push(Person {
        id: Id::new(使用者),
        slug: "end-user".into(),
        name: "End User".into(),
    });
    project.logical.relationships.push(Relationship {
        id: Id::new("r-使用者"),
        slug: "使用者-連-api".into(),
        purpose: "使用者從瀏覽器下單".into(),
        from: RelationshipEnd::Person(Id::new(使用者)),
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
                person: Id::new(使用者),
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
fn 使用者接上系統之後不該有任何抱怨() {
    // 加一個新的端點種類最容易犯的錯，是讓既有規則對它誤報。
    let found = lint(&有使用者的專案());
    assert!(found.is_empty(), "接了使用者卻報出問題：{found:?}");
}

#[test]
fn 人不需要落地() {
    // L001 要求邏輯層的東西在每個環境都要實現。但人不是部署出來的——
    // 如果 L001 把 Person 也算進去，每個專案都會永遠紅著。
    let project = 有使用者的專案();
    let 抱怨人的: Vec<_> = lint(&project)
        .into_iter()
        .filter(|f| f.subject == Id::new(使用者))
        .collect();
    assert!(抱怨人的.is_empty(), "不該要求人有落地：{抱怨人的:?}");
}

#[test]
fn 少接使用者那條會被抓到() {
    // 反過來說，契約本身還是要在每個環境實現——只是實現的方式是
    // 「有一條從人出發的連線」，而不是「人要有落地」。
    let mut project = 有使用者的專案();
    project.environments[2]
        .connections
        .retain(|c| c.serves != Id::new("r-使用者"));

    let found = lint(&project);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rule, Rule::L001);
    assert_eq!(found[0].subject, Id::new("r-使用者"));
}

#[test]
fn 指向不存在的人是錯誤() {
    let mut project = 有使用者的專案();
    project.environments[0].connections.last_mut().unwrap().from = Endpointing::Person {
        person: Id::new("p-不存在"),
    };

    let found = lint(&project);
    assert!(found.iter().any(|f| f.rule == Rule::L003), "{found:?}");
}

#[test]
fn 連線表把人顯示成名字不是id() {
    let project = 有使用者的專案();
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
fn 站點可以裝機器而且不影響任何檢查() {
    // 把 prod 既有的機器全部塞進一個站點底下。
    // 巢狀是既有能力，這裡確認新的 NodeKind 不會讓它出問題。
    let mut project = 有使用者的專案();
    let 原本 = std::mem::take(&mut project.environments[0].nodes);
    project.environments[0].nodes = vec![DeploymentNode {
        id: Id::new("n-prod-dc"),
        slug: "dc-主中心".into(),
        kind: NodeKind::Site,
        children: 原本,
        instances: vec![],
    }];

    let found = lint(&project);
    assert!(found.is_empty(), "包進站點之後卻報錯：{found:?}");
}

#[test]
fn 站點本身不會被當成沒人用的機器() {
    // 站點不跑任何東西，所以它底下沒有 Instance。L008 是針對 Instance 的，
    // 不該因為「這個節點沒有連線碰到」就對站點發警告。
    let mut project = 有使用者的專案();
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
fn 備援路徑的檢查標準跟正常路徑完全一樣() {
    // 標成 Fallback 不代表可以放寬——它一樣要被建立、防火牆一樣要開。
    // 如果 lint 對它比較寬鬆，那漏掉備援路徑就抓不到了，
    // 而備援路徑正是最常漏的東西。
    let mut project = 有使用者的專案();

    let 備援 = {
        let c = project.environments[0].connections[1].clone();
        Connection {
            id: Id::new("conn-prod-備援"),
            kind: ConnectionKind::Fallback,
            to: Endpointing::Instance {
                // 期望 5 台，實際只有 3 台——標成備援也一樣要被抓到。
                target: InstanceRef::Pattern {
                    slug_pattern: "redis-*".into(),
                    expect: Some(5),
                },
                endpoint: Some(Id::new(REDIS_CLIENT)),
            },
            ..c
        }
    };
    project.environments[0].connections.push(備援);

    let found = lint(&project);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, Rule::L004);
    assert_eq!(found[0].subject, Id::new("conn-prod-備援"));
}

#[test]
fn 備援路徑一樣算進可達性() {
    // 只剩備援路徑時，路還是通的——它是真的能走，只是平常不走。
    let mut project = 有使用者的專案();
    for c in &mut project.environments[2].connections {
        c.kind = ConnectionKind::Fallback;
    }

    assert!(
        !lint(&project).iter().any(|f| f.rule == Rule::L002),
        "全部標成備援之後被判定走不通"
    );
}

#[test]
fn 種類會出現在連線表上() {
    let mut project = 有使用者的專案();
    project.environments[0].connections[0].kind = ConnectionKind::Fallback;

    let 全部 = rows(&project);
    let 備援的 = 全部
        .iter()
        .filter(|r| r.kind == ConnectionKind::Fallback)
        .count();
    assert_eq!(備援的, 1);
}

#[test]
fn 沒標種類的連線預設是正常路徑() {
    // 既有的 YAML 沒有這個欄位，讀回來必須是 Primary，
    // 否則所有舊專案的連線都會突然變成備援。
    // 走 repository 的真實路徑，順便確認 YAML 佈局照舊。
    let mut store = loom_core::store::MemoryStore::new();
    loom_core::repository::save(&有使用者的專案(), &mut store).unwrap();

    // 把 kind 欄位從檔案裡整個拿掉，模擬舊專案。
    let path = "environments/prod.yaml";
    let 原本 = store.read(path).unwrap();
    assert!(
        !原本.contains("kind: primary"),
        "預設值不該被寫進 YAML，否則每個既有檔案都會多出一堆雜訊"
    );

    let 讀回來 = loom_core::repository::load(&store).unwrap();
    assert!(
        讀回來.environments[0]
            .connections
            .iter()
            .all(|c| c.kind == ConnectionKind::Primary),
        "沒寫 kind 的連線讀回來必須是正常路徑"
    );
}
