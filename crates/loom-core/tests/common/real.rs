//! `fixtures/通路系統.loom` 的內容：照一個**真實系統的架構**建的樣本。
//!
//! # 為什麼建構器住在這裡，不住在產生器裡
//!
//! 這份素材有兩個身分，而它們會打架：
//!
//! 1. **磁碟上的 `fixtures/通路系統.loom`**——開來玩的範例專案。
//!    使用者會在 App 裡改它、存檔，那正是它存在的用途。
//! 2. **測試素材**——用來釘住「模型撐不撐得住現實的形狀」。
//!
//! 原本身分 2 是直接讀身分 1 的檔案，於是每次有人玩過那份範例，
//! `mise run check` 就紅一次，然後只能把他的操作洗掉。
//!
//! 所以改成：測試從**這裡**現建一份，存進暫存區再讀回來。
//! 磁碟上那份就只剩身分 1，隨便玩。
//!
//! `mise run fixture:real` 仍然用同一個建構器重新產生它。

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

const SYSTEM: &str = "s-通路";
/// 站點的節點 id。`within` 用它把萬用字元限定在某個機房底下。
const MAIN_SITE: &str = "n-prod-dc-主中心";
const DR_SITE: &str = "n-prod-dc-異地";

/// 服務、顯示名、對外接點、協定。
const CONTAINER_SLUGS: &[(&str, &str, &str, Protocol)] = &[
    ("apache", "Apache 反向代理", "http", Protocol::Tcp),
    ("gateway", "Gateway Service", "http", Protocol::Tcp),
    ("channel", "Channel Service", "http", Protocol::Tcp),
    ("apigw", "API Gateway Service", "http", Protocol::Tcp),
    ("common", "共通服務", "http", Protocol::Tcp),
    ("consul", "Consul", "http-api", Protocol::Tcp),
    ("redis", "Redis", "client-port", Protocol::Tcp),
    ("db", "資料庫", "jdbc", Protocol::Jdbc),
];

fn endpoint(container: &str) -> Id {
    Id::new(format!("e-{container}"))
}

fn relationship_id(slug: &str) -> Id {
    Id::new(format!("r-{slug}"))
}

fn service_columns(container: &str) -> (&'static str, Protocol) {
    CONTAINER_SLUGS
        .iter()
        .find(|(s, ..)| *s == container)
        .map(|(_, _, ep, p)| (*ep, *p))
        .expect("服務清單裡沒有這個服務")
}

fn logical() -> Logical {
    let containers = CONTAINER_SLUGS
        .iter()
        .map(|(slug, name, ep, protocol)| Container {
            id: Id::new(format!("c-{slug}")),
            slug: (*slug).into(),
            name: (*name).into(),
            system: Id::new(SYSTEM),
            endpoints: vec![EndpointDef {
                id: endpoint(slug),
                slug: (*ep).into(),
                protocol: *protocol,
                memo: String::new(),
            }],
            memo: String::new(),
        })
        .collect();

    let link = |slug: &str, purpose: &str, from: &str, to: &str| Relationship {
        id: relationship_id(slug),
        slug: slug.into(),
        purpose: purpose.into(),
        from: RelationshipEnd::Container(Id::new(format!("c-{from}"))),
        to: RelationshipEnd::Container(Id::new(format!("c-{to}"))),
        to_endpoint: endpoint(to),
        memo: String::new(),
    };

    Logical {
        people: vec![Person {
            id: Id::new("p-使用者"),
            slug: "end-user".into(),
            name: "End User".into(),
            memo: String::new(),
        }],
        systems: vec![SoftwareSystem {
            id: Id::new(SYSTEM),
            slug: "通路系統".into(),
            name: "網路通路系統".into(),
            external: false,
            endpoints: vec![],
            memo: String::new(),
        }],
        containers,
        relationships: vec![
            // 使用者是流量的起點。這條在 C4 Context 圖上最常見，
            // 有了 `RelationshipEnd::Person` 才表達得出來。
            Relationship {
                id: relationship_id("使用者-連-apache"),
                slug: "使用者-連-apache".into(),
                purpose: "使用者從瀏覽器連進系統".into(),
                from: RelationshipEnd::Person(Id::new("p-使用者")),
                to: RelationshipEnd::Container(Id::new("c-apache")),
                to_endpoint: endpoint("apache"),
                memo: String::new(),
            },
            link("apache-連-gateway", "反向代理轉送請求", "apache", "gateway"),
            link(
                "gateway-連-channel",
                "解析後把 payload 轉給通路服務",
                "gateway",
                "channel",
            ),
            link(
                "gateway-連-consul",
                "服務探索：查詢 Channel Service 位置",
                "gateway",
                "consul",
            ),
            link("channel-連-consul", "服務註冊", "channel", "consul"),
            link("channel-連-redis", "Session 存取", "channel", "redis"),
            link(
                "channel-連-apigw",
                "呼叫共通服務前先過 API Gateway",
                "channel",
                "apigw",
            ),
            link(
                "apigw-連-common",
                "API Gateway 轉送給共通服務",
                "apigw",
                "common",
            ),
            link("gateway-連-db", "讀寫資料庫", "gateway", "db"),
            link("channel-連-db", "讀寫資料庫", "channel", "db"),
            link("common-連-db", "讀寫資料庫", "common", "db"),
        ],
    }
}

// ── 環境層的組裝零件 ────────────────────────────────────

fn instance(env: &str, slug: &str, container: &str, address: &str) -> ContainerInstance {
    let (ep, protocol) = service_columns(container);
    ContainerInstance {
        id: Id::new(format!("i-{env}-{slug}")),
        slug: slug.into(),
        container: Id::new(format!("c-{container}")),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-{env}-{slug}")),
            slug: ep.into(),
            def: Some(endpoint(container)),
            protocol,
            address: Some(address.into()),
            memo: String::new(),
        }],
        standalone: false,
        memo: String::new(),
    }
}

fn node(env: &str, slug: &str, instances: Vec<ContainerInstance>) -> DeploymentNode {
    DeploymentNode {
        id: Id::new(format!("n-{env}-{slug}")),
        slug: slug.into(),
        kind: NodeKind::VirtualMachine,
        children: vec![],
        instances,
        memo: String::new(),
    }
}

/// 機房。`NodeKind::Site` 是裝別的節點用的，本身不跑東西。
fn site(env: &str, slug: &str, nodes: Vec<DeploymentNode>) -> DeploymentNode {
    DeploymentNode {
        id: Id::new(format!("n-{env}-{slug}")),
        slug: slug.into(),
        kind: NodeKind::Site,
        children: nodes,
        instances: vec![],
        memo: String::new(),
    }
}

fn group_to(pattern: &str, expect: u32, container: &str) -> Endpointing {
    group_within(pattern, None, expect, container)
}

/// 限定在某個部署節點底下的一群。站點就用這個表達，不必編進 slug。
fn group_within(pattern: &str, within: Option<&str>, expect: u32, container: &str) -> Endpointing {
    Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: pattern.into(),
            within: within.map(Id::new),
            expect: Some(expect),
        },
        endpoint: Some(endpoint(container)),
    }
}

/// 來源端不指定接點——由作業系統分配 ephemeral port。
fn group_from(pattern: &str, expect: u32) -> Endpointing {
    group_from_within(pattern, None, expect)
}

fn group_from_within(pattern: &str, within: Option<&str>, expect: u32) -> Endpointing {
    Endpointing::Instance {
        target: InstanceRef::Pattern {
            slug_pattern: pattern.into(),
            within: within.map(Id::new),
            expect: Some(expect),
        },
        endpoint: None,
    }
}

fn infra(node: &str, endpoint: Option<&str>) -> Endpointing {
    Endpointing::Infra {
        node: Id::new(node),
        endpoint: endpoint.map(Id::new),
    }
}

struct ConnectionRows {
    env: &'static str,
    n: usize,
    out: Vec<Connection>,
}

impl ConnectionRows {
    fn new(env: &'static str) -> Self {
        Self {
            env,
            n: 0,
            out: vec![],
        }
    }

    fn push_row(&mut self, serves: &str, purpose: &str, from: Endpointing, to: Endpointing) {
        self.push_row_with_kind(serves, purpose, ConnectionKind::Primary, from, to);
    }

    fn push_row_with_kind(
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
            serves: relationship_id(serves),
            purpose: purpose.into(),
            kind,
            from,
            to,
            memo: String::new(),
        });
    }

    /// 同站優先 + 跨站備援：一條契約拆成四條連線。
    ///
    /// 只寫同站的兩條，跨中心備援路徑的防火牆有沒有開就看不出來——
    /// 而那正是最常漏的東西。
    ///
    /// 站點用 `within` 限定，不編進 slug。編進名字的話機器搬家之後
    /// 名字就說謊了，而且 `expect` 只數總量，搬家根本抓不到。
    fn cross_site_pair(
        &mut self,
        serves: &str,
        source: &str,
        target: &str,
        main_site_of: u32,
        dr_site_of: u32,
    ) {
        let pattern = |s: &str| format!("{s}-*");
        self.push_row(
            serves,
            "同中心優先（主中心）",
            group_from_within(&pattern(source), Some(MAIN_SITE), main_site_of),
            group_within(&pattern(target), Some(MAIN_SITE), main_site_of, target),
        );
        self.push_row(
            serves,
            "同中心優先（異地）",
            group_from_within(&pattern(source), Some(DR_SITE), dr_site_of),
            group_within(&pattern(target), Some(DR_SITE), dr_site_of, target),
        );
        // 標成 Fallback：一樣要建立、防火牆一樣要開，只是畫面上會區分開來。
        self.push_row_with_kind(
            serves,
            "跨中心備援：主中心 → 異地",
            ConnectionKind::Fallback,
            group_from_within(&pattern(source), Some(MAIN_SITE), main_site_of),
            group_within(&pattern(target), Some(DR_SITE), dr_site_of, target),
        );
        self.push_row_with_kind(
            serves,
            "跨中心備援：異地 → 主中心",
            ConnectionKind::Fallback,
            group_from_within(&pattern(source), Some(DR_SITE), dr_site_of),
            group_within(&pattern(target), Some(MAIN_SITE), main_site_of, target),
        );
    }
}

fn vip(id: &str, slug: &str, address: &str) -> InfrastructureNode {
    InfrastructureNode {
        id: Id::new(id),
        slug: slug.into(),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-{id}")),
            slug: "vip".into(),
            def: None,
            protocol: Protocol::Tcp,
            address: Some(address.into()),
            memo: String::new(),
        }],
        memo: String::new(),
    }
}

// ── prod：主中心 + 異地 ─────────────────────────────────

/// 站點寫進 slug（`gateway-main-01`）。
///
/// ⚠️ 限制 C 的一部分：模型沒有站點的概念，只能編進名字。
/// 不過「站點」比「角色」穩定——Redis 主從會切換，機器在哪個機房不會。
fn prod() -> Environment {
    let e = "prod";

    let main_site = site(
        e,
        "dc-主中心",
        vec![
            node(
                e,
                "vm-b01",
                vec![instance(e, "apache-01", "apache", "10.1.1.11:8080")],
            ),
            node(
                e,
                "vm-b02",
                vec![instance(e, "apache-02", "apache", "10.1.1.12:8080")],
            ),
            node(
                e,
                "vm-c01",
                vec![instance(e, "gateway-01", "gateway", "10.1.2.11:8080")],
            ),
            node(
                e,
                "vm-c02",
                vec![instance(e, "gateway-02", "gateway", "10.1.2.12:8080")],
            ),
            node(
                e,
                "vm-d01",
                vec![instance(e, "channel-01", "channel", "10.1.3.11:8080")],
            ),
            node(
                e,
                "vm-d02",
                vec![instance(e, "channel-02", "channel", "10.1.3.12:8080")],
            ),
            node(
                e,
                "vm-e01",
                vec![instance(e, "apigw-01", "apigw", "10.1.6.11:8080")],
            ),
            node(
                e,
                "vm-e02",
                vec![instance(e, "apigw-02", "apigw", "10.1.6.12:8080")],
            ),
            node(
                e,
                "vm-f01",
                vec![instance(e, "common-01", "common", "10.1.4.11:8080")],
            ),
            node(
                e,
                "vm-f02",
                vec![instance(e, "common-02", "common", "10.1.4.12:8080")],
            ),
            // 一台 VM 同時跑 Consul 與 Redis——模型接得住，
            // 一個 DeploymentNode 可以有多個 ContainerInstance。
            node(
                e,
                "vm-g01",
                vec![
                    instance(e, "consul-01", "consul", "10.1.5.11:8500"),
                    instance(e, "redis-01", "redis", "10.1.5.11:6379"),
                ],
            ),
            node(
                e,
                "vm-g02",
                vec![
                    instance(e, "consul-02", "consul", "10.1.5.12:8500"),
                    instance(e, "redis-02", "redis", "10.1.5.12:6379"),
                ],
            ),
            node(
                e,
                "vm-g03",
                vec![
                    instance(e, "consul-03", "consul", "10.1.5.13:8500"),
                    instance(e, "redis-03", "redis", "10.1.5.13:6379"),
                ],
            ),
            node(
                e,
                "vm-h",
                vec![instance(
                    e,
                    "db-01",
                    "db",
                    "jdbc:postgresql://10.1.9.10:5432/channel",
                )],
            ),
        ],
    );

    let dr_site = site(
        e,
        "dc-異地",
        vec![
            node(
                e,
                "vm-b03",
                vec![instance(e, "apache-03", "apache", "10.2.1.11:8080")],
            ),
            node(
                e,
                "vm-c03",
                vec![instance(e, "gateway-03", "gateway", "10.2.2.11:8080")],
            ),
            node(
                e,
                "vm-d03",
                vec![instance(e, "channel-03", "channel", "10.2.3.11:8080")],
            ),
            node(
                e,
                "vm-e03",
                vec![instance(e, "apigw-03", "apigw", "10.2.6.11:8080")],
            ),
            node(
                e,
                "vm-f03",
                vec![instance(e, "common-03", "common", "10.2.4.11:8080")],
            ),
            node(
                e,
                "vm-g04",
                vec![
                    instance(e, "consul-04", "consul", "10.2.5.11:8500"),
                    instance(e, "redis-04", "redis", "10.2.5.11:6379"),
                ],
            ),
            node(
                e,
                "vm-g05",
                vec![
                    instance(e, "consul-05", "consul", "10.2.5.12:8500"),
                    instance(e, "redis-05", "redis", "10.2.5.12:6379"),
                ],
            ),
            node(
                e,
                "vm-g06",
                vec![
                    instance(e, "consul-06", "consul", "10.2.5.13:8500"),
                    instance(e, "redis-06", "redis", "10.2.5.13:6379"),
                ],
            ),
        ],
    );

    let mut c = ConnectionRows::new(e);

    c.push_row(
        "使用者-連-apache",
        "使用者連上對外的 VIP",
        Endpointing::Person {
            person: Id::new("p-使用者"),
        },
        infra("f5-前台", Some("ep-f5-前台")),
    );
    c.push_row(
        "使用者-連-apache",
        "F5 分流到反向代理",
        infra("f5-前台", Some("ep-f5-前台")),
        group_to("apache-*", 3, "apache"),
    );
    c.push_row(
        "apache-連-gateway",
        "反向代理轉送給 Gateway",
        group_from("apache-*", 3),
        group_to("gateway-*", 3, "gateway"),
    );

    c.push_row(
        "gateway-連-consul",
        "查詢 Channel Service 的位置",
        group_from("gateway-*", 3),
        group_to("consul-*", 6, "consul"),
    );
    c.push_row(
        "channel-連-consul",
        "啟動時註冊自己",
        group_from("channel-*", 3),
        group_to("consul-*", 6, "consul"),
    );

    c.cross_site_pair("gateway-連-channel", "gateway", "channel", 2, 1);

    c.push_row(
        "channel-連-redis",
        "讀寫 Session",
        group_from("channel-*", 3),
        group_to("redis-*", 6, "redis"),
    );

    // API Gateway ＝ 一台 F5 ＋ 三台 API Gateway Service。
    c.push_row(
        "channel-連-apigw",
        "先送到 API Gateway 的 VIP",
        group_from("channel-*", 3),
        infra("f5-apigw", Some("ep-f5-apigw")),
    );
    c.push_row(
        "channel-連-apigw",
        "VIP 分流到 API Gateway Service",
        infra("f5-apigw", None),
        group_to("apigw-*", 3, "apigw"),
    );

    c.cross_site_pair("apigw-連-common", "apigw", "common", 2, 1);

    c.push_row(
        "gateway-連-db",
        "讀寫資料庫",
        group_from("gateway-*", 3),
        group_to("db-*", 1, "db"),
    );
    c.push_row(
        "channel-連-db",
        "讀寫資料庫",
        group_from("channel-*", 3),
        group_to("db-*", 1, "db"),
    );
    c.push_row(
        "common-連-db",
        "讀寫資料庫",
        group_from("common-*", 3),
        group_to("db-*", 1, "db"),
    );

    Environment {
        id: Id::new("env-prod"),
        slug: "prod".into(),
        name: "正式環境".into(),
        nodes: vec![main_site, dr_site],
        infra: vec![
            vip("f5-前台", "f5-前台", "203.0.113.10:443"),
            vip("f5-apigw", "f5-api-gateway", "10.1.0.20:8443"),
        ],
        systems: vec![],
        connections: c.out,
        memo: String::new(),
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

    let site = site(
        e,
        "dc-測試機房",
        vec![
            node(
                e,
                "vm-t-b01",
                vec![instance(e, "apache-01", "apache", "10.9.1.11:8080")],
            ),
            node(
                e,
                "vm-t-c01",
                vec![instance(e, "gateway-01", "gateway", "10.9.2.11:8080")],
            ),
            node(
                e,
                "vm-t-d01",
                vec![instance(e, "channel-01", "channel", "10.9.3.11:8080")],
            ),
            node(
                e,
                "vm-t-e01",
                vec![instance(e, "apigw-01", "apigw", "10.9.6.11:8080")],
            ),
            node(
                e,
                "vm-t-f01",
                vec![instance(e, "common-01", "common", "10.9.4.11:8080")],
            ),
            node(
                e,
                "vm-t-g01",
                vec![instance(e, "consul-01", "consul", "10.9.5.11:8500")],
            ),
            node(
                e,
                "vm-t-h",
                vec![instance(
                    e,
                    "db-01",
                    "db",
                    "jdbc:postgresql://10.9.9.10:5432/channel",
                )],
            ),
        ],
    );

    let mut c = ConnectionRows::new(e);
    c.push_row(
        "使用者-連-apache",
        "使用者直接連反向代理（測試環境沒有 F5）",
        Endpointing::Person {
            person: Id::new("p-使用者"),
        },
        group_to("apache-*", 1, "apache"),
    );
    c.push_row(
        "apache-連-gateway",
        "反向代理轉送給 Gateway",
        group_from("apache-*", 1),
        group_to("gateway-*", 1, "gateway"),
    );
    c.push_row(
        "gateway-連-consul",
        "查詢 Channel Service 的位置",
        group_from("gateway-*", 1),
        group_to("consul-*", 1, "consul"),
    );
    c.push_row(
        "channel-連-consul",
        "啟動時註冊自己",
        group_from("channel-*", 1),
        group_to("consul-*", 1, "consul"),
    );
    c.push_row(
        "gateway-連-channel",
        "轉送解析後的 payload",
        group_from("gateway-*", 1),
        group_to("channel-*", 1, "channel"),
    );
    c.push_row(
        "channel-連-apigw",
        "呼叫 API Gateway",
        group_from("channel-*", 1),
        group_to("apigw-*", 1, "apigw"),
    );
    // 共通服務實際只有 1 台，這裡寫 2——刻意留的洞。
    c.push_row(
        "apigw-連-common",
        "轉送給共通服務",
        group_from("apigw-*", 1),
        group_to("common-*", 2, "common"),
    );
    c.push_row(
        "gateway-連-db",
        "讀寫資料庫",
        group_from("gateway-*", 1),
        group_to("db-*", 1, "db"),
    );
    c.push_row(
        "channel-連-db",
        "讀寫資料庫",
        group_from("channel-*", 1),
        group_to("db-*", 1, "db"),
    );
    c.push_row(
        "common-連-db",
        "讀寫資料庫",
        group_from("common-*", 1),
        group_to("db-*", 1, "db"),
    );

    Environment {
        id: Id::new("env-test"),
        slug: "test".into(),
        name: "測試環境".into(),
        nodes: vec![site],
        infra: vec![],
        systems: Vec::<SoftwareSystemInstance>::new(),
        connections: c.out,
        memo: String::new(),
    }
}

pub fn project() -> Project {
    Project {
        id: Id::new("p-通路系統"),
        slug: "通路系統".into(),
        name: "網路通路系統".into(),
        logical: logical(),
        environments: vec![prod(), test_env()],
        memo: String::new(),
    }
}
