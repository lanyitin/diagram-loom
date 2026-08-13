//! 產生 `fixtures/通路系統.loom`：`mise run fixture:real`。
//!
//! 這是照著一個**真實系統的架構**建的樣本，用來驗證領域模型接不接得住現實。
//! 跟 `sample.loom` 那份「刻意留破洞的教學素材」不同，這份的重點是規模與形狀。
//!
//! # 架構
//!
//! ```text
//! 使用者 → F5 ─┬→ Apache RP ×3 ─→ Gateway ×3 ─┬→ Channel ×3 ─┬→ Redis ×6（Session）
//!              │  (B1,B2 主／B3 異)  (C1,C2 主／C3 異) │  (D1,D2 主／D3 異) │
//!              │                                │              ├→ API GW → 共通服務 ×3
//!              │                                │              └→ Consul（註冊）
//!              │                                └→ Consul ×6（查詢／服務探索）
//!              └ 所有 Gateway / Channel / 共通服務 ─────────────→ 資料庫 ×1
//! ```
//!
//! Consul 分兩個 Cluster（主中心 3 台、異地 3 台），Redis 六節點一個 Cluster
//! （主中心 3 台預設 master、異地 3 台預設 slave），兩者**共用同一批 VM**（G1–G6）。
//!
//! # 建的過程中撞到的模型限制
//!
//! 都寫在對應的地方，搜尋「⚠️ 限制」看得到全部。摘要：
//!
//! 1. `RelationshipEnd` 只能是 Container 或 SoftwareSystem，**沒有 Person**——
//!    「使用者 → Apache」這條 C4 上完全合法的關係表達不出來。
//! 2. `NodeKind` 只有 physical / virtual-machine / linux-container，
//!    **沒有機房／站點**，主中心與異地只能勉強掛成 physical。
//! 3. 沒有「Cluster」與「角色（master/slave）」的概念，只能編進 slug。

use loom_core::Project;
use loom_core::environment::{
    Connection, ContainerInstance, DeploymentNode, Endpoint, Endpointing, Environment,
    InfrastructureNode, InstanceRef, NodeKind, SoftwareSystemInstance,
};
use loom_core::id::Id;
use loom_core::logical::{
    Container, EndpointDef, Logical, Person, Protocol, Relationship, RelationshipEnd,
    SoftwareSystem,
};

const 系統: &str = "s-通路";

/// 服務與它對外的接點。
const 服務清單: &[(&str, &str, &str, Protocol)] = &[
    ("apache", "Apache 反向代理", "http", Protocol::Tcp),
    ("gateway", "Gateway Service", "http", Protocol::Tcp),
    ("channel", "Channel Service", "http", Protocol::Tcp),
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
        // ⚠️ 限制 1：Person 宣告得出來，但接不上任何連線。
        //
        // `RelationshipEnd` 只有 `Container` 與 `System` 兩種，所以
        // 「使用者 → Apache 反向代理」這條在 C4 裡完全正常的關係，
        // 這個模型表達不出來。整條進入點的流量（使用者 → F5 → Apache）
        // 因此少了最前面那一段，只能從 F5 開始畫。
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
                "channel-連-common",
                "呼叫共通服務（經 API Gateway）",
                "channel",
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
    ContainerInstance {
        id: Id::new(format!("i-{env}-{slug}")),
        slug: slug.into(),
        container: Id::new(format!("c-{服務}")),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-{env}-{slug}")),
            slug: 服務清單
                .iter()
                .find(|(s, ..)| *s == 服務)
                .map(|(_, _, ep, _)| (*ep).to_string())
                .expect("服務清單裡沒有這個服務"),
            def: Some(接點(服務)),
            protocol: 服務清單
                .iter()
                .find(|(s, ..)| *s == 服務)
                .map(|(.., p)| *p)
                .unwrap(),
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

/// ⚠️ 限制 2：`NodeKind` 沒有「機房／站點」。
///
/// 主中心與異地是站點，不是實體機、不是 VM、也不是容器。這裡只能勉強
/// 標成 `Physical`。畫成 Deployment 圖時它會被當成一台實體機。
fn 站點(env: &str, slug: &str, 機器們: Vec<DeploymentNode>) -> DeploymentNode {
    DeploymentNode {
        id: Id::new(format!("n-{env}-{slug}")),
        slug: slug.into(),
        kind: NodeKind::Physical,
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

fn 連線(
    env: &str,
    n: usize,
    serves: &str,
    purpose: &str,
    from: Endpointing,
    to: Endpointing,
) -> Connection {
    Connection {
        id: Id::new(format!("conn-{env}-{n:02}")),
        serves: 契約(serves),
        purpose: purpose.into(),
        from,
        to,
    }
}

// ── prod：主中心 + 異地 ─────────────────────────────────

fn prod() -> Environment {
    let e = "prod";

    // B1,B2 主中心／B3 異地；C、D、F 依同樣的 2+1 分佈。
    //
    // ⚠️ 需要確認：使用者只明確說了 B 與 C 的分佈，D（Channel）與
    // F（共通服務）沒說。這裡照 B、C 的模式假設 3 號機在異地。
    let 主中心 = 站點(
        e,
        "dc-主中心",
        vec![
            機器(
                e,
                "vm-b01",
                vec![落地(e, "apache-01", "apache", "10.1.1.11:8080")],
            ),
            機器(
                e,
                "vm-b02",
                vec![落地(e, "apache-02", "apache", "10.1.1.12:8080")],
            ),
            機器(
                e,
                "vm-c01",
                vec![落地(e, "gateway-01", "gateway", "10.1.2.11:8080")],
            ),
            機器(
                e,
                "vm-c02",
                vec![落地(e, "gateway-02", "gateway", "10.1.2.12:8080")],
            ),
            機器(
                e,
                "vm-d01",
                vec![落地(e, "channel-01", "channel", "10.1.3.11:8080")],
            ),
            機器(
                e,
                "vm-d02",
                vec![落地(e, "channel-02", "channel", "10.1.3.12:8080")],
            ),
            機器(
                e,
                "vm-f01",
                vec![落地(e, "common-01", "common", "10.1.4.11:8080")],
            ),
            機器(
                e,
                "vm-f02",
                vec![落地(e, "common-02", "common", "10.1.4.12:8080")],
            ),
            // ⚠️ 限制 3：一台 VM 跑兩個服務——這個模型接得住（一個
            // DeploymentNode 可以有多個 ContainerInstance），很好。
            //
            // 但「Consul 分兩個 Cluster」與「Redis 節點是 master 還是 slave」
            // 都沒有地方放，只能編進 slug。改天主從切換，slug 就說謊了。
            機器(
                e,
                "vm-g01",
                vec![
                    落地(e, "consul-main-01", "consul", "10.1.5.11:8500"),
                    落地(e, "redis-master-01", "redis", "10.1.5.11:6379"),
                ],
            ),
            機器(
                e,
                "vm-g02",
                vec![
                    落地(e, "consul-main-02", "consul", "10.1.5.12:8500"),
                    落地(e, "redis-master-02", "redis", "10.1.5.12:6379"),
                ],
            ),
            機器(
                e,
                "vm-g03",
                vec![
                    落地(e, "consul-main-03", "consul", "10.1.5.13:8500"),
                    落地(e, "redis-master-03", "redis", "10.1.5.13:6379"),
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
                vec![落地(e, "apache-03", "apache", "10.2.1.11:8080")],
            ),
            機器(
                e,
                "vm-c03",
                vec![落地(e, "gateway-03", "gateway", "10.2.2.11:8080")],
            ),
            機器(
                e,
                "vm-d03",
                vec![落地(e, "channel-03", "channel", "10.2.3.11:8080")],
            ),
            機器(
                e,
                "vm-f03",
                vec![落地(e, "common-03", "common", "10.2.4.11:8080")],
            ),
            機器(
                e,
                "vm-g04",
                vec![
                    落地(e, "consul-dr-01", "consul", "10.2.5.11:8500"),
                    落地(e, "redis-slave-01", "redis", "10.2.5.11:6379"),
                ],
            ),
            機器(
                e,
                "vm-g05",
                vec![
                    落地(e, "consul-dr-02", "consul", "10.2.5.12:8500"),
                    落地(e, "redis-slave-02", "redis", "10.2.5.12:6379"),
                ],
            ),
            機器(
                e,
                "vm-g06",
                vec![
                    落地(e, "consul-dr-03", "consul", "10.2.5.13:8500"),
                    落地(e, "redis-slave-03", "redis", "10.2.5.13:6379"),
                ],
            ),
        ],
    );

    // ⚠️ 需要確認：API Gateway（E）被描述成「一個 API Gateway」，
    // 沒有說部署在幾台機器上——跟 F5 的描述方式一樣，所以這裡當成設備。
    // 如果它其實是跑在 VM 上的軟體，應該改成 Container。
    let infra = vec![
        InfrastructureNode {
            id: Id::new("f5-prod"),
            slug: "f5-vip".into(),
            endpoints: vec![Endpoint {
                id: Id::new("ep-f5-prod"),
                slug: "vip-https".into(),
                def: None,
                protocol: Protocol::Tcp,
                address: Some("203.0.113.10:443".into()),
            }],
        },
        InfrastructureNode {
            id: Id::new("apigw-prod"),
            slug: "api-gateway".into(),
            endpoints: vec![Endpoint {
                id: Id::new("ep-apigw-prod"),
                slug: "vip-https".into(),
                def: None,
                protocol: Protocol::Tcp,
                address: Some("10.1.0.20:8443".into()),
            }],
        },
    ];

    let connections = vec![
        // 使用者 → F5 → Apache。前半段（使用者 → F5）表達不出來，見限制 1。
        連線(
            e,
            1,
            "apache-連-gateway",
            "F5 分流到反向代理",
            設備("f5-prod", Some("ep-f5-prod")),
            群("apache-*", 3, "apache"),
        ),
        連線(
            e,
            2,
            "apache-連-gateway",
            "反向代理轉送給 Gateway",
            從群("apache-*", 3),
            群("gateway-*", 3, "gateway"),
        ),
        連線(
            e,
            3,
            "gateway-連-consul",
            "查詢 Channel Service 的位置",
            從群("gateway-*", 3),
            群("consul-*", 6, "consul"),
        ),
        連線(
            e,
            4,
            "channel-連-consul",
            "啟動時註冊自己",
            從群("channel-*", 3),
            群("consul-*", 6, "consul"),
        ),
        連線(
            e,
            5,
            "gateway-連-channel",
            "轉送解析後的 payload",
            從群("gateway-*", 3),
            群("channel-*", 3, "channel"),
        ),
        連線(
            e,
            6,
            "channel-連-redis",
            "讀寫 Session",
            從群("channel-*", 3),
            群("redis-*", 6, "redis"),
        ),
        連線(
            e,
            7,
            "channel-連-common",
            "呼叫共通服務（第一段：到 API Gateway）",
            從群("channel-*", 3),
            設備("apigw-prod", Some("ep-apigw-prod")),
        ),
        連線(
            e,
            8,
            "channel-連-common",
            "API Gateway 分流到共通服務",
            設備("apigw-prod", None),
            群("common-*", 3, "common"),
        ),
        連線(
            e,
            9,
            "gateway-連-db",
            "讀寫資料庫",
            從群("gateway-*", 3),
            群("db-*", 1, "db"),
        ),
        連線(
            e,
            10,
            "channel-連-db",
            "讀寫資料庫",
            從群("channel-*", 3),
            群("db-*", 1, "db"),
        ),
        連線(
            e,
            11,
            "common-連-db",
            "讀寫資料庫",
            從群("common-*", 3),
            群("db-*", 1, "db"),
        ),
    ];

    Environment {
        id: Id::new("env-prod"),
        slug: "prod".into(),
        name: "正式環境".into(),
        nodes: vec![主中心, 異地],
        infra,
        systems: vec![],
        connections,
    }
}

// ── test：單站點、每種一台 ──────────────────────────────

/// ⚠️ 這個環境是**編的**。
///
/// 使用者只描述了正式環境的架構。加一個縮編版的測試環境，是為了讓
/// 覆蓋矩陣有第二欄可以比——這個工具的核心價值就是跨環境比對，
/// 只有一個環境的話那張表看不出東西。
///
/// 它刻意少了兩樣：沒有 Redis（所以 Session 那條契約整條缺）、
/// 共通服務只有 1 台但連線寫 expect 2。這兩個洞是真實專案裡最常見的形狀。
fn test_env() -> Environment {
    let e = "test";

    let 站 = 站點(
        e,
        "dc-測試機房",
        vec![
            機器(
                e,
                "vm-t-b01",
                vec![落地(e, "apache-01", "apache", "10.9.1.11:8080")],
            ),
            機器(
                e,
                "vm-t-c01",
                vec![落地(e, "gateway-01", "gateway", "10.9.2.11:8080")],
            ),
            機器(
                e,
                "vm-t-d01",
                vec![落地(e, "channel-01", "channel", "10.9.3.11:8080")],
            ),
            機器(
                e,
                "vm-t-f01",
                vec![落地(e, "common-01", "common", "10.9.4.11:8080")],
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

    let connections = vec![
        連線(
            e,
            1,
            "apache-連-gateway",
            "反向代理轉送給 Gateway",
            從群("apache-*", 1),
            群("gateway-*", 1, "gateway"),
        ),
        連線(
            e,
            2,
            "gateway-連-consul",
            "查詢 Channel Service 的位置",
            從群("gateway-*", 1),
            群("consul-*", 1, "consul"),
        ),
        連線(
            e,
            3,
            "channel-連-consul",
            "啟動時註冊自己",
            從群("channel-*", 1),
            群("consul-*", 1, "consul"),
        ),
        連線(
            e,
            4,
            "gateway-連-channel",
            "轉送解析後的 payload",
            從群("gateway-*", 1),
            群("channel-*", 1, "channel"),
        ),
        // 共通服務實際只有 1 台，但這裡寫 2——刻意留的洞。
        連線(
            e,
            5,
            "channel-連-common",
            "呼叫共通服務",
            從群("channel-*", 1),
            群("common-*", 2, "common"),
        ),
        連線(
            e,
            6,
            "gateway-連-db",
            "讀寫資料庫",
            從群("gateway-*", 1),
            群("db-*", 1, "db"),
        ),
        連線(
            e,
            7,
            "channel-連-db",
            "讀寫資料庫",
            從群("channel-*", 1),
            群("db-*", 1, "db"),
        ),
        連線(
            e,
            8,
            "common-連-db",
            "讀寫資料庫",
            從群("common-*", 1),
            群("db-*", 1, "db"),
        ),
    ];

    Environment {
        id: Id::new("env-test"),
        slug: "test".into(),
        name: "測試環境".into(),
        nodes: vec![站],
        infra: vec![],
        systems: Vec::<SoftwareSystemInstance>::new(),
        connections,
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
    for f in loom_core::lint::lint(&project) {
        println!("  {} {}", f.rule.code(), f.detail);
    }
}
