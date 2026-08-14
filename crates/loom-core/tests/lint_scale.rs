//! Lint 在真實規模下要跑多久。
//!
//! 這件事會影響 UI 的設計，不只是效能好奇心：
//! 如果 lint 是毫秒級，就可以在使用者每改一下就重跑，不需要進度條；
//! 如果是秒級，UI 就得做非同步 + 進度回報，複雜度差很多。
//!
//! 素材是程式產生的，形狀跟真的專案一樣——每條邏輯連線在環境層都拆成
//! 兩段（經過 F5），目標端是萬用字元展開成一整群 Instance。
//! 這會同時壓到 lint 最貴的兩件事：**萬用字元比對**與**可達性 BFS**。

use std::time::{Duration, Instant};

use loom_core::Project;
use loom_core::environment::{
    Connection, ContainerInstance, DeploymentNode, Endpoint, Endpointing, Environment,
    InfrastructureNode, InstanceRef, NodeKind,
};
use loom_core::id::Id;
use loom_core::lint::lint;
use loom_core::logical::{
    Container, EndpointDef, Logical, Protocol, Relationship, RelationshipEnd, SoftwareSystem,
};

/// 一個規模。`nodes` 是每個服務在每個環境有幾台。
#[derive(Clone, Copy)]
struct Scale {
    services: usize,
    envs: usize,
    nodes: usize,
}

impl Scale {
    /// 邏輯連線的條數。每個服務連下一個服務，繞成一圈。
    fn relationships(self) -> usize {
        self.services
    }

    /// 環境層實際連線的條數：每條邏輯連線拆兩段 × 每個環境。
    fn connections(self) -> usize {
        self.services * 2 * self.envs
    }

    fn instances(self) -> usize {
        self.services * self.nodes * self.envs
    }
}

fn svc(i: usize) -> String {
    format!("c-svc{i:03}")
}

fn def(i: usize) -> String {
    format!("e-svc{i:03}")
}

fn logical(scale: Scale) -> Logical {
    let containers = (0..scale.services)
        .map(|i| Container {
            id: Id::new(svc(i)),
            slug: format!("svc{i:03}"),
            name: format!("服務 {i}"),
            system: Id::new("s-main"),
            endpoints: vec![EndpointDef {
                id: Id::new(def(i)),
                slug: "client-port".into(),
                protocol: Protocol::Tcp,
                memo: String::new(),
            }],
            memo: String::new(),
        })
        .collect();

    // 每個服務連下一個，繞成一圈——保證每個服務都同時是來源也是目標，
    // 可達性 BFS 不會因為圖太稀疏而變得太便宜。
    let relationships = (0..scale.services)
        .map(|i| {
            let next = (i + 1) % scale.services;
            Relationship {
                id: Id::new(format!("r-{i:03}")),
                slug: format!("svc{i:03}-連-svc{next:03}"),
                purpose: "壓力測試用的連線".into(),
                from: RelationshipEnd::Container(Id::new(svc(i))),
                to: RelationshipEnd::Container(Id::new(svc(next))),
                to_endpoint: Id::new(def(next)),
                memo: String::new(),
            }
        })
        .collect();

    Logical {
        people: vec![],
        systems: vec![SoftwareSystem {
            id: Id::new("s-main"),
            slug: "main".into(),
            name: "主系統".into(),
            external: false,
            endpoints: vec![],
            memo: String::new(),
        }],
        containers,
        relationships,
    }
}

fn environment(scale: Scale, e: usize) -> Environment {
    let env = format!("env{e}");

    let mut nodes = Vec::with_capacity(scale.services * scale.nodes);
    for i in 0..scale.services {
        for k in 0..scale.nodes {
            let slug = format!("svc{i:03}-{k:02}");
            nodes.push(DeploymentNode {
                id: Id::new(format!("n-{env}-{slug}")),
                slug: format!("vm-{slug}"),
                kind: NodeKind::VirtualMachine,
                children: vec![],
                instances: vec![ContainerInstance {
                    id: Id::new(format!("i-{env}-{slug}")),
                    slug: slug.clone(),
                    container: Id::new(svc(i)),
                    endpoints: vec![Endpoint {
                        id: Id::new(format!("ep-{env}-{slug}")),
                        slug: "client-port".into(),
                        def: Some(Id::new(def(i))),
                        protocol: Protocol::Tcp,
                        address: Some(format!("10.{e}.{i}.{k}:6379")),
                        memo: String::new(),
                    }],
                    standalone: false,
                    memo: String::new(),
                }],
                memo: String::new(),
            });
        }
    }

    let infra = vec![InfrastructureNode {
        id: Id::new(format!("f5-{env}")),
        slug: "f5-vip".into(),
        endpoints: vec![Endpoint {
            id: Id::new(format!("ep-f5-{env}")),
            slug: "vip".into(),
            def: None,
            protocol: Protocol::Tcp,
            address: Some(format!("10.{e}.0.100:6379")),
            memo: String::new(),
        }],
        memo: String::new(),
    }];

    // 每條邏輯連線拆兩段：來源 → F5 → 目標那一整群。
    let mut connections = Vec::with_capacity(scale.services * 2);
    for i in 0..scale.services {
        let next = (i + 1) % scale.services;
        connections.extend([
            Connection {
                id: Id::new(format!("conn-{env}-{i:03}-a")),
                serves: Id::new(format!("r-{i:03}")),
                purpose: "第一段：到 F5".into(),
                kind: Default::default(),
                from: Endpointing::Instance {
                    target: InstanceRef::Pattern {
                        slug_pattern: format!("svc{i:03}-*"),
                        within: None,
                        expect: Some(scale.nodes as u32),
                    },
                    endpoint: None,
                },
                to: Endpointing::Infra {
                    node: Id::new(format!("f5-{env}")),
                    endpoint: Some(Id::new(format!("ep-f5-{env}"))),
                },
                memo: String::new(),
            },
            Connection {
                id: Id::new(format!("conn-{env}-{i:03}-b")),
                serves: Id::new(format!("r-{i:03}")),
                purpose: "第二段：F5 分流到叢集".into(),
                kind: Default::default(),
                from: Endpointing::Infra {
                    node: Id::new(format!("f5-{env}")),
                    endpoint: None,
                },
                to: Endpointing::Instance {
                    target: InstanceRef::Pattern {
                        slug_pattern: format!("svc{next:03}-*"),
                        within: None,
                        expect: Some(scale.nodes as u32),
                    },
                    endpoint: Some(Id::new(def(next))),
                },
                memo: String::new(),
            },
        ]);
    }

    Environment {
        id: Id::new(format!("id-{env}")),
        slug: env.clone(),
        name: format!("環境 {e}"),
        nodes,
        infra,
        systems: vec![],
        connections,
        memo: String::new(),
    }
}

fn project(scale: Scale) -> Project {
    Project {
        id: Id::new("p-scale"),
        slug: "scale".into(),
        name: "規模測試".into(),
        logical: logical(scale),
        environments: (0..scale.envs).map(|e| environment(scale, e)).collect(),
        memo: String::new(),
    }
}

/// 跑幾次取最快的一次。最快的那次最接近「沒有被別的行程干擾」的真實成本。
fn amount(project: &Project) -> (Duration, usize) {
    let mut best = Duration::MAX;
    let mut findings = 0;
    for _ in 0..5 {
        let t = Instant::now();
        let f = lint(project);
        best = best.min(t.elapsed());
        findings = f.len();
    }
    (best, findings)
}

#[test]
fn the_generated_fixture_is_itself_clean() {
    // 如果素材自己就一堆錯，量到的就不是「正常專案」的成本，
    // 而是「一直在配置錯誤字串」的成本。
    let findings = lint(&project(Scale {
        services: 20,
        envs: 2,
        nodes: 3,
    }));
    assert!(
        findings.is_empty(),
        "壓力測試的素材應該完全乾淨，但有 {} 個問題：{:?}",
        findings.len(),
        &findings[..findings.len().min(5)]
    );
}

#[test]
fn lint_stays_in_milliseconds_at_hundreds_of_connections() {
    let step = [
        Scale {
            services: 25,
            envs: 3,
            nodes: 4,
        },
        Scale {
            services: 50,
            envs: 3,
            nodes: 6,
        },
        Scale {
            services: 100,
            envs: 3,
            nodes: 8,
        },
        Scale {
            services: 200,
            envs: 3,
            nodes: 12,
        },
    ];

    println!("\n 邏輯連線 │ 實際連線 │  Instance │      耗時 │ 問題數");
    println!("──────────┼──────────┼───────────┼───────────┼────────");

    let mut slowest = Duration::ZERO;
    for scale in step {
        let (elapsed, findings) = amount(&project(scale));
        slowest = slowest.max(elapsed);
        println!(
            "{:>9} │{:>9} │{:>10} │{:>10?} │{:>7}",
            scale.relationships(),
            scale.connections(),
            scale.instances(),
            elapsed,
            findings
        );
    }
    println!();

    // 這個上限刻意放得很寬——它不是效能目標，是「有沒有退化成平方級」的警報器。
    // 真實數字比這個小兩個數量級以上；訂太緊只會在別人的機器上隨機紅燈。
    assert!(
        slowest < Duration::from_secs(3),
        "lint 最慢跑了 {slowest:?}——退化成這樣的話，UI 就不能每次改動都重跑了"
    );
}
