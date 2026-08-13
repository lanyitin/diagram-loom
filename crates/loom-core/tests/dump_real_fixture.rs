//! 產生 `fixtures/通路系統.loom`：`mise run fixture:real`。
//!
//! 照著一個**真實系統的架構**建的樣本，用來驗證領域模型接不接得住現實。
//! 跟 `sample.loom` 那份「刻意留破洞的教學素材」不同，這份的重點是規模與形狀。
//!
//! # 架構
//!
//! ```text
//! 使用者 → F5 → Apache RP ×3 → Gateway ×3 ─┬→ Channel ×3 ─┬→ Redis ×6（Session）
//!                                            │              ├→ F5 → API GW ×3 → 共通服務 ×3
//!                                            │              └→ Consul（服務註冊）
//!                                            └→ Consul ×6（服務探索）
//!       Gateway / Channel / 共通服務 ──────────────────────→ 資料庫 ×1
//! ```
//!
//! 主中心與異地各一套（2+1）。Consul 分兩個 Cluster（各站 3 台），
//! Redis 六節點一個 Cluster（主中心 3 台預設 master、異地 3 台預設 slave），
//! 兩者**共用同一批 VM**（G1–G6）。
//!
//! # 同站優先 + 跨站備援
//!
//! Gateway→Channel 與 API GW→共通服務都是「同中心優先，某中心出問題時
//! 才交叉」。這在模型裡拆成**四條連線**：兩條同站、兩條跨站。
//!
//! 寫成一條扁平的 `gateway-* → channel-*` 也會通過 lint，但那樣就
//! **看不出跨中心那條路有沒有真的開通**——而防火牆規則最常漏的就是那條。
//!
//! 代價是模型分不出「這條是正常路徑」與「這條只在故障時走」，
//! 四條線看起來一樣重。見 `docs/domain-model.md` 的限制 E。
//!
//! # 建的過程撞到的模型限制
//!
//! 搜尋「⚠️ 限制」看得到全部，摘要在 `docs/domain-model.md`。

use loom_core::Project;
use loom_core::environment::{
    Connection, ConnectionKind, ContainerInstance, DeploymentNode, Endpoint, Endpointing,
    Environment, InfrastructureNode, InstanceRef, NodeKind, SoftwareSystemInstance,
};
use loom_core::id::Id;
use loom_core::logical::{
    Container, EndpointDef, Logical, Person, Protocol, Relationship, RelationshipEnd,
    SoftwareSystem,
};

const 系統: &str = "s-通路";

/// 服務、顯示名、對外接點、協定。
const 服務清單: &[(&str, &str, &str, Protocol)] = &[
    ("apache", "Apache 反向代理", "http", Protocol::Tcp),
    ("gateway", "Gateway Service", "http", Protocol::Tcp),
    ("channel", "Channel Service", "http", Protocol::Tcp),
    ("apigw", "API Gateway Service", "http", Protocol::Tcp),
    ("common", "共通服務", "http", Protocol::Tcp),
    ("consul", "Consul", "http-api", Protocol::Tcp),
    ("redis", "Redis", "client-port", Protocol::Tcp),
    ("db", "資料庫", "jdbc", Protocol::Jdbc),
];

fn 接點(服務: &str) -> Id {
    Id::new(format!("e-{服務}"))
}

fn 契約(slug: &str) -> Id {
    Id::new(format!("r-{slug}"))
}

fn 服務欄位(服務: &str) -> (&'static str, Protocol) {
    服務清單
        .iter()
        .find(|(s, ..)| *s == 服務)
        .map(|(_, _, ep, p)| (*ep, *p))
        .expect("服務清單裡沒有這個服務")
}

fn logical() -> Logical {
    let containers = 服務清單
        .iter()
        .map(|(slug, name, ep, protocol)| Container {
            id: Id::new(format!("c-{slug}")),
            slug: (*slug).into(),
            name: (*name).into(),
            system: Id::new(系統),
            endpoints: vec![EndpointDef {
                id: 接點(slug),
                slug: (*ep).into(),
                protocol: *protocol,
            }],
        })
        .collect();

    let 連 = |slug: &str, purpose: &str, from: &str, to: &str| Relationship {
        id: 契約(slug),
        slug: slug.into(),
        purpose: purpose.into(),
        from: RelationshipEnd::Container(Id::new(format!("c-{from}"))),
        to: RelationshipEnd::Container(Id::new(format!("c-{to}"))),
        to_endpoint: 接點(to),
    };

    Logical {
        people: vec![Person {
            id: Id::new("p-使用者"),
            slug: "end-user".into(),
            name: "End User".into(),
        }],
        systems: vec![SoftwareSystem {
            id: Id::new(系統),
            slug: "通路系統".into(),
            name: "網路通路系統".into(),
            external: false,
            endpoints: vec![],
        }],
        containers,
        relationships: vec![
            // 使用者是流量的起點。這條在 C4 Context 圖上最常見，
            // 有了 `RelationshipEnd::Person` 才表達得出來。
            Relationship {
                id: 契約("使用者-連-apache"),
                slug: "使用者-連-apache".into(),
                purpose: "使用者從瀏覽器連進系統".into(),
                from: RelationshipEnd::Person(Id::new("p-使用者")),
                to: RelationshipEnd::Container(Id::new("c-apache")),
                to_endpoint: 接點("apache"),
            },
            連("apache-連-gateway", "反向代理轉送請求", "apache", "gateway"),
            連(
                "gateway-連-channel",
                "解析後把 payload 轉給通路服務",
                "gateway",
                "channel",
            ),
            連(
                "gateway-連-consul",
                "服務探索：查詢 Channel Service 位置",
                "gateway",
                "consul",
            ),
            連("channel-連-consul", "服務註冊", "channel", "consul"),
            連("channel-連-redis", "Session 存取", "channel", "redis"),
            連(
                "channel-連-apigw",
                "呼叫共通服務前先過 API Gateway",
                "channel",
                "apigw",
            ),
            連(
                "apigw-連-common",
                "API Gateway 轉送給共通服務",
                "apigw",
                "common",
            ),
            連("gateway-連-db", "讀寫資料庫", "gateway", "db"),
            連("channel-連-db", "讀寫資料庫", "channel", "db"),
            連("common-連-db", "讀寫資料庫", "common", "db"),
        ],
    }
}

// ── 環境層的組裝零件 ────────────────────────────────────

fn 落地(env: &str, slug: &str, 服務: &str, 位址: &str) -> ContainerInstance {
    let (ep, protocol) = 服務欄位(服務);
    ContainerInstance {
        id: Id::new(format!("i-{env}-{slug}")),
        slug: slug.into(),
        container: Id::new(format!("c-{服務}")),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-{env}-{slug}")),
            slug: ep.into(),
            def: Some(接點(服務)),
            protocol,
            address: Some(位址.into()),
        }],
        standalone: false,
    }
}

fn 機器(env: &str, slug: &str, instances: Vec<ContainerInstance>) -> DeploymentNode {
    DeploymentNode {
        id: Id::new(format!("n-{env}-{slug}")),
        slug: slug.into(),
        kind: NodeKind::VirtualMachine,
        children: vec![],
        instances,
    }
}

/// 機房。`NodeKind::Site` 是裝別的節點用的，本身不跑東西。
fn 站點(env: &str, slug: &str, 機器們: Vec<DeploymentNode>) -> DeploymentNode {
    DeploymentNode {
        id: Id::new(format!("n-{env}-{slug}")),
        slug: slug.into(),
        kind: NodeKind::Site,
        children: 機器們,
        instances: vec![],
    }
}

fn 群(pattern: &str, expect: u32, 服務: &str) -> Endpointing {
    Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: pattern.into(),
            expect: Some(expect),
        },
        endpoint: Some(接點(服務)),
    }
}

/// 來源端不指定接點——由作業系統分配 ephemeral port。
fn 從群(pattern: &str, expect: u32) -> Endpointing {
    Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: pattern.into(),
            expect: Some(expect),
        },
        endpoint: None,
    }
}

fn 設備(node: &str, endpoint: Option<&str>) -> Endpointing {
    Endpointing::Infra {
        node: Id::new(node),
        endpoint: endpoint.map(Id::new),
    }
}

struct 連線表 {
    env: &'static str,
    n: usize,
    out: Vec<Connection>,
}

impl 連線表 {
    fn new(env: &'static str) -> Self {
        Self {
            env,
            n: 0,
            out: vec![],
        }
    }

    fn 加(&mut self, serves: &str, purpose: &str, from: Endpointing, to: Endpointing) {
        self.加種類(serves, purpose, ConnectionKind::Primary, from, to);
    }

    fn 加種類(
        &mut self,
        serves: &str,
        purpose: &str,
        kind: ConnectionKind,
        from: Endpointing,
        to: Endpointing,
    ) {
        self.n += 1;
        self.out.push(Connection {
            id: Id::new(format!("conn-{}-{:02}", self.env, self.n)),
            serves: 契約(serves),
            purpose: purpose.into(),
            kind,
            from,
            to,
        });
    }

    /// 同站優先 + 跨站備援：一條契約拆成四條連線。
    ///
    /// 只寫同站的兩條，跨中心備援路徑的防火牆有沒有開就看不出來——
    /// 而那正是最常漏的東西。
    fn 兩站互連(&mut self, serves: &str, 來源: &str, 目標: &str, 主: u32, 異: u32) {
        self.加(
            serves,
            "同中心優先（主中心）",
            從群(&format!("{來源}-main-*"), 主),
            群(&format!("{目標}-main-*"), 主, 目標),
        );
        self.加(
            serves,
            "同中心優先（異地）",
            從群(&format!("{來源}-dr-*"), 異),
            群(&format!("{目標}-dr-*"), 異, 目標),
        );
        // 標成 Fallback：一樣要建立、防火牆一樣要開，只是畫面上會區分開來。
        self.加種類(
            serves,
            "跨中心備援：主中心 → 異地",
            ConnectionKind::Fallback,
            從群(&format!("{來源}-main-*"), 主),
            群(&format!("{目標}-dr-*"), 異, 目標),
        );
        self.加種類(
            serves,
            "跨中心備援：異地 → 主中心",
            ConnectionKind::Fallback,
            從群(&format!("{來源}-dr-*"), 異),
            群(&format!("{目標}-main-*"), 主, 目標),
        );
    }
}

fn vip(id: &str, slug: &str, 位址: &str) -> InfrastructureNode {
    InfrastructureNode {
        id: Id::new(id),
        slug: slug.into(),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-{id}")),
            slug: "vip".into(),
            def: None,
            protocol: Protocol::Tcp,
            address: Some(位址.into()),
        }],
    }
}

// ── prod：主中心 + 異地 ─────────────────────────────────

/// 站點寫進 slug（`gateway-main-01`）。
///
/// ⚠️ 限制 C 的一部分：模型沒有站點的概念，只能編進名字。
/// 不過「站點」比「角色」穩定——Redis 主從會切換，機器在哪個機房不會。
fn prod() -> Environment {
    let e = "prod";

    let 主中心 = 站點(
        e,
        "dc-主中心",
        vec![
            機器(
                e,
                "vm-b01",
                vec![落地(e, "apache-main-01", "apache", "10.1.1.11:8080")],
            ),
            機器(
                e,
                "vm-b02",
                vec![落地(e, "apache-main-02", "apache", "10.1.1.12:8080")],
            ),
            機器(
                e,
                "vm-c01",
                vec![落地(e, "gateway-main-01", "gateway", "10.1.2.11:8080")],
            ),
            機器(
                e,
                "vm-c02",
                vec![落地(e, "gateway-main-02", "gateway", "10.1.2.12:8080")],
            ),
            機器(
                e,
                "vm-d01",
                vec![落地(e, "channel-main-01", "channel", "10.1.3.11:8080")],
            ),
            機器(
                e,
                "vm-d02",
                vec![落地(e, "channel-main-02", "channel", "10.1.3.12:8080")],
            ),
            機器(
                e,
                "vm-e01",
                vec![落地(e, "apigw-main-01", "apigw", "10.1.6.11:8080")],
            ),
            機器(
                e,
                "vm-e02",
                vec![落地(e, "apigw-main-02", "apigw", "10.1.6.12:8080")],
            ),
            機器(
                e,
                "vm-f01",
                vec![落地(e, "common-main-01", "common", "10.1.4.11:8080")],
            ),
            機器(
                e,
                "vm-f02",
                vec![落地(e, "common-main-02", "common", "10.1.4.12:8080")],
            ),
            // 一台 VM 同時跑 Consul 與 Redis——模型接得住，
            // 一個 DeploymentNode 可以有多個 ContainerInstance。
            機器(
                e,
                "vm-g01",
                vec![
                    落地(e, "consul-main-01", "consul", "10.1.5.11:8500"),
                    落地(e, "redis-main-01", "redis", "10.1.5.11:6379"),
                ],
            ),
            機器(
                e,
                "vm-g02",
                vec![
                    落地(e, "consul-main-02", "consul", "10.1.5.12:8500"),
                    落地(e, "redis-main-02", "redis", "10.1.5.12:6379"),
                ],
            ),
            機器(
                e,
                "vm-g03",
                vec![
                    落地(e, "consul-main-03", "consul", "10.1.5.13:8500"),
                    落地(e, "redis-main-03", "redis", "10.1.5.13:6379"),
                ],
            ),
            機器(
                e,
                "vm-h",
                vec![落地(
                    e,
                    "db-01",
                    "db",
                    "jdbc:postgresql://10.1.9.10:5432/channel",
                )],
            ),
        ],
    );

    let 異地 = 站點(
        e,
        "dc-異地",
        vec![
            機器(
                e,
                "vm-b03",
                vec![落地(e, "apache-dr-01", "apache", "10.2.1.11:8080")],
            ),
            機器(
                e,
                "vm-c03",
                vec![落地(e, "gateway-dr-01", "gateway", "10.2.2.11:8080")],
            ),
            機器(
                e,
                "vm-d03",
                vec![落地(e, "channel-dr-01", "channel", "10.2.3.11:8080")],
            ),
            機器(
                e,
                "vm-e03",
                vec![落地(e, "apigw-dr-01", "apigw", "10.2.6.11:8080")],
            ),
            機器(
                e,
                "vm-f03",
                vec![落地(e, "common-dr-01", "common", "10.2.4.11:8080")],
            ),
            機器(
                e,
                "vm-g04",
                vec![
                    落地(e, "consul-dr-01", "consul", "10.2.5.11:8500"),
                    落地(e, "redis-dr-01", "redis", "10.2.5.11:6379"),
                ],
            ),
            機器(
                e,
                "vm-g05",
                vec![
                    落地(e, "consul-dr-02", "consul", "10.2.5.12:8500"),
                    落地(e, "redis-dr-02", "redis", "10.2.5.12:6379"),
                ],
            ),
            機器(
                e,
                "vm-g06",
                vec![
                    落地(e, "consul-dr-03", "consul", "10.2.5.13:8500"),
                    落地(e, "redis-dr-03", "redis", "10.2.5.13:6379"),
                ],
            ),
        ],
    );

    let mut c = 連線表::new(e);

    c.加(
        "使用者-連-apache",
        "使用者連上對外的 VIP",
        Endpointing::Person {
            person: Id::new("p-使用者"),
        },
        設備("f5-前台", Some("ep-f5-前台")),
    );
    c.加(
        "使用者-連-apache",
        "F5 分流到反向代理",
        設備("f5-前台", Some("ep-f5-前台")),
        群("apache-*", 3, "apache"),
    );
    c.加(
        "apache-連-gateway",
        "反向代理轉送給 Gateway",
        從群("apache-*", 3),
        群("gateway-*", 3, "gateway"),
    );

    c.加(
        "gateway-連-consul",
        "查詢 Channel Service 的位置",
        從群("gateway-*", 3),
        群("consul-*", 6, "consul"),
    );
    c.加(
        "channel-連-consul",
        "啟動時註冊自己",
        從群("channel-*", 3),
        群("consul-*", 6, "consul"),
    );

    c.兩站互連("gateway-連-channel", "gateway", "channel", 2, 1);

    c.加(
        "channel-連-redis",
        "讀寫 Session",
        從群("channel-*", 3),
        群("redis-*", 6, "redis"),
    );

    // API Gateway ＝ 一台 F5 ＋ 三台 API Gateway Service。
    c.加(
        "channel-連-apigw",
        "先送到 API Gateway 的 VIP",
        從群("channel-*", 3),
        設備("f5-apigw", Some("ep-f5-apigw")),
    );
    c.加(
        "channel-連-apigw",
        "VIP 分流到 API Gateway Service",
        設備("f5-apigw", None),
        群("apigw-*", 3, "apigw"),
    );

    c.兩站互連("apigw-連-common", "apigw", "common", 2, 1);

    c.加(
        "gateway-連-db",
        "讀寫資料庫",
        從群("gateway-*", 3),
        群("db-*", 1, "db"),
    );
    c.加(
        "channel-連-db",
        "讀寫資料庫",
        從群("channel-*", 3),
        群("db-*", 1, "db"),
    );
    c.加(
        "common-連-db",
        "讀寫資料庫",
        從群("common-*", 3),
        群("db-*", 1, "db"),
    );

    Environment {
        id: Id::new("env-prod"),
        slug: "prod".into(),
        name: "正式環境".into(),
        nodes: vec![主中心, 異地],
        infra: vec![
            vip("f5-前台", "f5-前台", "203.0.113.10:443"),
            vip("f5-apigw", "f5-api-gateway", "10.1.0.20:8443"),
        ],
        systems: vec![],
        connections: c.out,
    }
}

// ── test：單站點、每種一台 ──────────────────────────────

/// ⚠️ 這個環境是**編的**。
///
/// 使用者只描述了正式環境。加一個縮編版是為了讓覆蓋矩陣有第二欄可以比——
/// 跨環境比對正是這個工具的核心價值，只有一欄的話那張表看不出東西。
///
/// 刻意留了兩個洞：沒有 Redis（Session 那條契約整條缺）、
/// 共通服務只有 1 台但連線寫 expect 2。這是真實專案裡最常見的形狀。
fn test_env() -> Environment {
    let e = "test";

    let 站 = 站點(
        e,
        "dc-測試機房",
        vec![
            機器(
                e,
                "vm-t-b01",
                vec![落地(e, "apache-main-01", "apache", "10.9.1.11:8080")],
            ),
            機器(
                e,
                "vm-t-c01",
                vec![落地(e, "gateway-main-01", "gateway", "10.9.2.11:8080")],
            ),
            機器(
                e,
                "vm-t-d01",
                vec![落地(e, "channel-main-01", "channel", "10.9.3.11:8080")],
            ),
            機器(
                e,
                "vm-t-e01",
                vec![落地(e, "apigw-main-01", "apigw", "10.9.6.11:8080")],
            ),
            機器(
                e,
                "vm-t-f01",
                vec![落地(e, "common-main-01", "common", "10.9.4.11:8080")],
            ),
            機器(
                e,
                "vm-t-g01",
                vec![落地(e, "consul-main-01", "consul", "10.9.5.11:8500")],
            ),
            機器(
                e,
                "vm-t-h",
                vec![落地(
                    e,
                    "db-01",
                    "db",
                    "jdbc:postgresql://10.9.9.10:5432/channel",
                )],
            ),
        ],
    );

    let mut c = 連線表::new(e);
    c.加(
        "使用者-連-apache",
        "使用者直接連反向代理（測試環境沒有 F5）",
        Endpointing::Person {
            person: Id::new("p-使用者"),
        },
        群("apache-*", 1, "apache"),
    );
    c.加(
        "apache-連-gateway",
        "反向代理轉送給 Gateway",
        從群("apache-*", 1),
        群("gateway-*", 1, "gateway"),
    );
    c.加(
        "gateway-連-consul",
        "查詢 Channel Service 的位置",
        從群("gateway-*", 1),
        群("consul-*", 1, "consul"),
    );
    c.加(
        "channel-連-consul",
        "啟動時註冊自己",
        從群("channel-*", 1),
        群("consul-*", 1, "consul"),
    );
    c.加(
        "gateway-連-channel",
        "轉送解析後的 payload",
        從群("gateway-*", 1),
        群("channel-*", 1, "channel"),
    );
    c.加(
        "channel-連-apigw",
        "呼叫 API Gateway",
        從群("channel-*", 1),
        群("apigw-*", 1, "apigw"),
    );
    // 共通服務實際只有 1 台，這裡寫 2——刻意留的洞。
    c.加(
        "apigw-連-common",
        "轉送給共通服務",
        從群("apigw-*", 1),
        群("common-*", 2, "common"),
    );
    c.加(
        "gateway-連-db",
        "讀寫資料庫",
        從群("gateway-*", 1),
        群("db-*", 1, "db"),
    );
    c.加(
        "channel-連-db",
        "讀寫資料庫",
        從群("channel-*", 1),
        群("db-*", 1, "db"),
    );
    c.加(
        "common-連-db",
        "讀寫資料庫",
        從群("common-*", 1),
        群("db-*", 1, "db"),
    );

    Environment {
        id: Id::new("env-test"),
        slug: "test".into(),
        name: "測試環境".into(),
        nodes: vec![站],
        infra: vec![],
        systems: Vec::<SoftwareSystemInstance>::new(),
        connections: c.out,
    }
}

fn 專案() -> Project {
    Project {
        id: Id::new("p-通路系統"),
        slug: "通路系統".into(),
        name: "網路通路系統".into(),
        logical: logical(),
        environments: vec![prod(), test_env()],
    }
}

#[test]
#[ignore = "產生器，不是測試。用 mise run fixture:real 執行"]
fn 產生真實架構樣本() {
    let project = 專案();

    let 目的地 =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/通路系統.loom");
    if 目的地.exists() {
        std::fs::remove_dir_all(&目的地).expect("清掉舊的");
    }
    loom_core::repository::save_to_dir(&project, &目的地).expect("寫出樣本");

    let 落地數: usize = project
        .environments
        .iter()
        .map(|e| {
            e.nodes
                .iter()
                .map(|n| n.instances_recursive().len())
                .sum::<usize>()
        })
        .sum();
    let 連線數: usize = project
        .environments
        .iter()
        .map(|e| e.connections.len())
        .sum();

    println!("\n已寫出 {}", 目的地.display());
    println!(
        "{} 個服務／{} 條契約／{} 個落地／{} 條連線",
        project.logical.containers.len(),
        project.logical.relationships.len(),
        落地數,
        連線數
    );

    println!("\nlint：");
    let findings = loom_core::lint::lint(&project);
    if findings.is_empty() {
        println!("  （乾淨）");
    }
    for f in &findings {
        println!("  {} {}", f.rule.code(), f.detail);
    }
}
