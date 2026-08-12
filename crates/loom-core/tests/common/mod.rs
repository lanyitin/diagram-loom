//! 整合測試共用的假專案。
//!
//! 這份素材**只用公開 API** 組成——如果模型只有 crate 內部才組得起來，
//! 這裡就會編不過。這本身就是一項整合驗證。
//!
//! # 假專案的形狀
//!
//! 一間網路商店。自家有訂單 API 與 Redis 快取，外部有一個金流系統。
//!
//! | 環境 | 樣子 |
//! | --- | --- |
//! | `prod` | 訂單 API → **F5** → Redis 叢集 **3 台**；金流走正式閘道 |
//! | `test` | 訂單 API → Redis 叢集 **2 台**，無 F5；金流走 sandbox |
//! | `dev`  | 訂單 API → Redis **1 台**，直連；金流走本機 mock |
//!
//! 涵蓋了設計上最容易出錯的幾件事：經過設備的多段路徑、叢集扇出、
//! 同一條邏輯連線在不同環境展開成不同數量，以及外部系統在各環境的不同落地。

use loom_core::Project;
use loom_core::environment::{
    Connection, ContainerInstance, DeploymentNode, Endpoint, Endpointing, Environment,
    InfrastructureNode, InstanceRef, NodeKind, SoftwareSystemInstance,
};
use loom_core::id::Id;
use loom_core::logical::{
    Container, EndpointDef, Logical, Protocol, Relationship, RelationshipEnd, SoftwareSystem,
};

pub const SHOP: &str = "s-shop";
pub const PAYMENT: &str = "s-payment";
pub const API: &str = "c-order-api";
pub const REDIS: &str = "c-redis";
pub const API_EGRESS: &str = "e-api-egress";
pub const REDIS_CLIENT: &str = "e-redis-client";
pub const PAY_HTTPS: &str = "e-pay-https";
pub const REL_CACHE: &str = "r-api-快取";
pub const REL_PAY: &str = "r-api-金流";

/// 邏輯層母版：自家兩個服務、一個外部系統、兩條連線。三個環境共用。
pub fn logical() -> Logical {
    Logical {
        people: vec![],
        systems: vec![
            SoftwareSystem {
                id: Id::new(SHOP),
                slug: "shop".into(),
                name: "網路商店".into(),
                external: false,
                endpoints: vec![],
            },
            SoftwareSystem {
                id: Id::new(PAYMENT),
                slug: "payment".into(),
                name: "金流系統".into(),
                external: true,
                // 外部系統不拆成 Container，接點直接掛在系統上。
                endpoints: vec![EndpointDef {
                    id: Id::new(PAY_HTTPS),
                    slug: "https".into(),
                    protocol: Protocol::Tcp,
                }],
            },
        ],
        containers: vec![
            Container {
                id: Id::new(API),
                slug: "order-api".into(),
                name: "訂單 API".into(),
                system: Id::new(SHOP),
                endpoints: vec![EndpointDef {
                    id: Id::new(API_EGRESS),
                    slug: "egress".into(),
                    protocol: Protocol::Tcp,
                }],
            },
            Container {
                id: Id::new(REDIS),
                slug: "redis".into(),
                name: "Redis 快取".into(),
                system: Id::new(SHOP),
                endpoints: vec![EndpointDef {
                    id: Id::new(REDIS_CLIENT),
                    slug: "client-port".into(),
                    protocol: Protocol::Tcp,
                }],
            },
        ],
        relationships: vec![
            Relationship {
                id: Id::new(REL_CACHE),
                slug: "api-連-redis".into(),
                purpose: "訂單服務讀寫快取".into(),
                from: RelationshipEnd::Container(Id::new(API)),
                to: RelationshipEnd::Container(Id::new(REDIS)),
                to_endpoint: Id::new(REDIS_CLIENT),
            },
            Relationship {
                id: Id::new(REL_PAY),
                slug: "api-連-金流".into(),
                purpose: "送出付款請求".into(),
                from: RelationshipEnd::Container(Id::new(API)),
                to: RelationshipEnd::System(Id::new(PAYMENT)),
                to_endpoint: Id::new(PAY_HTTPS),
            },
        ],
    }
}

/// 建一個有位址的 endpoint。
pub fn endpoint(id: &str, slug: &str, def: &str, address: &str) -> Endpoint {
    Endpoint {
        id: Id::new(id),
        slug: slug.into(),
        def: Some(Id::new(def)),
        protocol: Protocol::Tcp,
        address: Some(address.into()),
    }
}

/// 建一個 Instance，帶一個 endpoint。
pub fn instance(
    env: &str,
    slug: &str,
    container: &str,
    def: &str,
    address: &str,
) -> ContainerInstance {
    ContainerInstance {
        id: Id::new(format!("i-{env}-{slug}")),
        slug: slug.into(),
        container: Id::new(container),
        endpoints: vec![endpoint(
            &format!("ep-{env}-{slug}"),
            "client-port",
            def,
            address,
        )],
        standalone: false,
    }
}

/// 金流系統在某環境的落地。
pub fn payment(env: &str, slug: &str, address: &str) -> SoftwareSystemInstance {
    SoftwareSystemInstance {
        id: Id::new(format!("sys-{env}-payment")),
        slug: slug.into(),
        system: Id::new(PAYMENT),
        endpoints: vec![endpoint(
            &format!("ep-{env}-payment"),
            "https",
            PAY_HTTPS,
            address,
        )],
        standalone: false,
    }
}

/// 把一批 Instance 各自放進自己的 VM。
pub fn vms(env: &str, instances: Vec<ContainerInstance>) -> Vec<DeploymentNode> {
    instances
        .into_iter()
        .map(|i| DeploymentNode {
            id: Id::new(format!("n-{env}-{}", i.slug)),
            slug: format!("vm-{}", i.slug),
            kind: NodeKind::VirtualMachine,
            children: vec![],
            instances: vec![i],
        })
        .collect()
}

/// 來源端：指名一個 Instance，不指定 endpoint（由 OS 分配 ephemeral port）。
pub fn from_instance(id: &str) -> Endpointing {
    Endpointing::Instance {
        instance: InstanceRef::One(Id::new(id)),
        endpoint: None,
    }
}

/// 目標端：一整群 Instance，附期望數量。
pub fn to_cluster(pattern: &str, expect: Option<usize>, endpoint_def: &str) -> Endpointing {
    Endpointing::Instance {
        instance: InstanceRef::Pattern {
            slug_pattern: pattern.into(),
            expect,
        },
        endpoint: Some(Id::new(endpoint_def)),
    }
}

pub fn via_infra(node: &str, endpoint_id: &str) -> Endpointing {
    Endpointing::Infra {
        node: Id::new(node),
        endpoint: Some(Id::new(endpoint_id)),
    }
}

pub fn to_system(instance: &str, endpoint_def: &str) -> Endpointing {
    Endpointing::System {
        instance: Id::new(instance),
        endpoint: Some(Id::new(endpoint_def)),
    }
}

pub fn connection(
    id: &str,
    serves: &str,
    purpose: &str,
    from: Endpointing,
    to: Endpointing,
) -> Connection {
    Connection {
        id: Id::new(id),
        serves: Id::new(serves),
        purpose: purpose.into(),
        from,
        to,
    }
}

/// 每個環境都有的金流連線。
fn pay_connection(env: &str) -> Connection {
    connection(
        &format!("conn-{env}-pay"),
        REL_PAY,
        "送出付款請求",
        from_instance(&format!("i-{env}-api-01")),
        to_system(&format!("sys-{env}-payment"), PAY_HTTPS),
    )
}

/// prod：訂單 API → F5 → Redis 叢集 3 台。連線因此拆成兩段。
pub fn prod() -> Environment {
    let mut nodes = vms(
        "prod",
        vec![instance("prod", "api-01", API, API_EGRESS, "10.0.0.5:8080")],
    );
    nodes.extend(vms(
        "prod",
        (1..=3)
            .map(|n| {
                instance(
                    "prod",
                    &format!("redis-0{n}"),
                    REDIS,
                    REDIS_CLIENT,
                    &format!("10.0.1.1{n}:6379"),
                )
            })
            .collect(),
    ));

    Environment {
        id: Id::new("env-prod"),
        slug: "prod".into(),
        name: "正式環境".into(),
        nodes,
        infra: vec![InfrastructureNode {
            id: Id::new("f5-prod"),
            slug: "f5-vip".into(),
            endpoints: vec![Endpoint {
                id: Id::new("ep-f5-redis"),
                slug: "vip-redis".into(),
                def: None,
                protocol: Protocol::Tcp,
                address: Some("10.0.0.100:6379".into()),
            }],
        }],
        systems: vec![payment(
            "prod",
            "payment-gateway",
            "https://pay.example.com",
        )],
        connections: vec![
            connection(
                "conn-prod-1",
                REL_CACHE,
                "訂單服務讀寫快取（第一段：到 F5）",
                from_instance("i-prod-api-01"),
                via_infra("f5-prod", "ep-f5-redis"),
            ),
            connection(
                "conn-prod-2",
                REL_CACHE,
                "訂單服務讀寫快取（第二段：F5 到後端）",
                via_infra("f5-prod", "ep-f5-redis"),
                to_cluster("redis-*", Some(3), REDIS_CLIENT),
            ),
            pay_connection("prod"),
        ],
    }
}

/// test：規模較小，2 台 Redis，沒有 F5，直連；金流走 sandbox。
pub fn test_env() -> Environment {
    let mut nodes = vms(
        "test",
        vec![instance("test", "api-01", API, API_EGRESS, "10.1.0.5:8080")],
    );
    nodes.extend(vms(
        "test",
        (1..=2)
            .map(|n| {
                instance(
                    "test",
                    &format!("redis-0{n}"),
                    REDIS,
                    REDIS_CLIENT,
                    &format!("10.1.1.1{n}:6379"),
                )
            })
            .collect(),
    ));

    Environment {
        id: Id::new("env-test"),
        slug: "test".into(),
        name: "測試環境".into(),
        nodes,
        infra: vec![],
        systems: vec![payment(
            "test",
            "payment-sandbox",
            "https://sandbox.pay.example.com",
        )],
        connections: vec![
            connection(
                "conn-test-1",
                REL_CACHE,
                "訂單服務讀寫快取",
                from_instance("i-test-api-01"),
                to_cluster("redis-*", Some(2), REDIS_CLIENT),
            ),
            pay_connection("test"),
        ],
    }
}

/// dev：最小規模，1 台 Redis，直連；金流走本機 mock。
pub fn dev() -> Environment {
    let mut nodes = vms(
        "dev",
        vec![instance("dev", "api-01", API, API_EGRESS, "127.0.0.1:8080")],
    );
    nodes.extend(vms(
        "dev",
        vec![instance(
            "dev",
            "redis-01",
            REDIS,
            REDIS_CLIENT,
            "127.0.0.1:6379",
        )],
    ));

    Environment {
        id: Id::new("env-dev"),
        slug: "dev".into(),
        name: "開發環境".into(),
        nodes,
        infra: vec![],
        systems: vec![payment("dev", "payment-mock", "http://localhost:9000")],
        connections: vec![
            connection(
                "conn-dev-1",
                REL_CACHE,
                "訂單服務讀寫快取",
                from_instance("i-dev-api-01"),
                Endpointing::Instance {
                    instance: InstanceRef::One(Id::new("i-dev-redis-01")),
                    endpoint: Some(Id::new(REDIS_CLIENT)),
                },
            ),
            pay_connection("dev"),
        ],
    }
}

/// 三個環境都健康的完整專案。
pub fn healthy_project() -> Project {
    Project {
        id: Id::new("p-shop"),
        slug: "shop".into(),
        name: "網路商店".into(),
        logical: logical(),
        environments: vec![prod(), test_env(), dev()],
    }
}
