//! Lint：找出「這個環境還缺什麼」。
//!
//! 這是本工具的靈魂。痛點是數百條連線怕漏掉，lint 就是防漏機制。
//!
//! 規則代號與 `docs/lint-rules.md` 對應。目前實作 L001–L008；
//! L009–L011 與 draw.io 有關，等圖的部分做好再補。

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{ContainerInstance, Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;
use crate::logical::RelationshipEnd;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// 規則代號。序列化後就是 `"L001"` 這種字串，跟文件一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Rule {
    /// 邏輯層元素在某環境沒有任何實現。
    L001,
    /// 某條 Relationship 在某環境走不通（可達性斷裂）。
    L002,
    /// 連線指向不存在的 Instance、設備或 Endpoint。
    L003,
    /// 萬用字元的實際數量與 `expect` 不符。
    L004,
    /// 萬用字元沒有註明 `expect`。
    L005,
    /// Endpoint 缺少實際位址。
    L006,
    /// 連線沒有填用途。
    L007,
    /// 某 Instance 沒有被任何連線碰到。
    L008,
}

impl Rule {
    pub fn code(self) -> &'static str {
        match self {
            Rule::L001 => "L001",
            Rule::L002 => "L002",
            Rule::L003 => "L003",
            Rule::L004 => "L004",
            Rule::L005 => "L005",
            Rule::L006 => "L006",
            Rule::L007 => "L007",
            Rule::L008 => "L008",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            Rule::L001 | Rule::L002 | Rule::L003 | Rule::L004 | Rule::L006 => Severity::Error,
            Rule::L005 | Rule::L007 | Rule::L008 => Severity::Warning,
        }
    }
}

/// 一項發現。
///
/// 欄位順序即排序順序，讓 lint 的輸出穩定可比對。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule: Rule,
    /// 發生在哪個環境；邏輯層本身的問題為 `None`。
    pub environment: Option<Id>,
    /// 出問題的元素。
    pub subject: Id,
    pub detail: String,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.rule.severity()
    }
}

/// 連線圖上的一個點。可達性檢查就在這種點之間走。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GraphNode {
    Instance(Id),
    Infra(Id),
    System(Id),
}

/// 檢查整個專案，回傳排序後的發現清單。
///
/// 回傳結果**已排序且去重**，因此可以直接整份比對，
/// 多冒出一項非預期的發現就會被測試抓到。
pub fn lint(project: &Project) -> Vec<Finding> {
    let mut findings = Vec::new();

    for rel in &project.logical.relationships {
        if rel.purpose.trim().is_empty() {
            findings.push(Finding {
                rule: Rule::L007,
                environment: None,
                subject: rel.id.clone(),
                detail: format!("邏輯連線 {} 沒有填用途", rel.slug),
            });
        }
    }

    for env in &project.environments {
        lint_environment(project, env, &mut findings);
    }

    findings.sort();
    findings.dedup();
    findings
}

fn lint_environment(project: &Project, env: &Environment, findings: &mut Vec<Finding>) {
    let index = EnvIndex::build(project, env);

    check_logical_realized(project, env, &index.all, findings);
    check_connections(env, &index, findings);
    check_endpoint_addresses(env, &index, findings);
    check_relationships_reachable(project, env, &index, findings);
    check_orphan_instances(env, &index, findings);
}

/// L001：邏輯層的東西在每個環境都要落地。
///
/// - 每個 Container 至少要有一個 ContainerInstance
/// - 每個**外部** SoftwareSystem 至少要有一個 SoftwareSystemInstance
///   （自家系統靠自己的 Container 落地，不另外檢查）
///
/// 判定是「至少一個」而非「數量相同」：prod 12 台、test 6 台本來就正常，
/// 數量的把關交給 `expect`（L004）。
fn check_logical_realized(
    project: &Project,
    env: &Environment,
    instances: &[&ContainerInstance],
    findings: &mut Vec<Finding>,
) {
    for container in &project.logical.containers {
        if !instances.iter().any(|i| i.container == container.id) {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: container.id.clone(),
                detail: format!(
                    "服務 {} 在環境 {} 沒有任何 Instance",
                    container.slug, env.slug
                ),
            });
        }
    }

    for system in project.logical.external_systems() {
        if !env.systems.iter().any(|s| s.system == system.id) {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: system.id.clone(),
                detail: format!(
                    "外部系統 {} 在環境 {} 沒有指定落地位址",
                    system.slug, env.slug
                ),
            });
        }
    }
}

/// L003 / L004 / L005 / L007：逐條連線檢查。
fn check_connections(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    for conn in &env.connections {
        if conn.purpose.trim().is_empty() {
            findings.push(Finding {
                rule: Rule::L007,
                environment: Some(env.id.clone()),
                subject: conn.id.clone(),
                detail: "連線沒有填用途".into(),
            });
        }

        if !index.knows_relationship(&conn.serves) {
            findings.push(Finding {
                rule: Rule::L003,
                environment: Some(env.id.clone()),
                subject: conn.id.clone(),
                detail: format!("連線指向不存在的邏輯連線 {}", conn.serves),
            });
        }

        check_endpointing(env, index, conn.id.clone(), &conn.from, findings);
        check_endpointing(env, index, conn.id.clone(), &conn.to, findings);
    }
}

fn check_endpointing(
    env: &Environment,
    index: &EnvIndex<'_>,
    conn_id: Id,
    side: &Endpointing,
    findings: &mut Vec<Finding>,
) {
    let env_id = Some(env.id.clone());

    match side {
        Endpointing::Instance { target, endpoint } => match target {
            InstanceRef::One(id) => match index.instance(id) {
                None => findings.push(Finding {
                    rule: Rule::L003,
                    environment: env_id,
                    subject: conn_id,
                    detail: format!("連線指向不存在的 Instance {id}"),
                }),
                Some(found) => {
                    check_instance_has_endpoint(found, endpoint.as_ref(), env, conn_id, findings);
                }
            },
            InstanceRef::Pattern {
                slug_pattern,
                expect,
            } => {
                let matched = index.matching(slug_pattern);

                match expect {
                    None => findings.push(Finding {
                        rule: Rule::L005,
                        environment: env_id.clone(),
                        subject: conn_id.clone(),
                        detail: format!("萬用字元 {slug_pattern} 沒有註明 expect 期望數量"),
                    }),
                    Some(want) if *want as usize != matched.len() => findings.push(Finding {
                        rule: Rule::L004,
                        environment: env_id.clone(),
                        subject: conn_id.clone(),
                        detail: format!(
                            "萬用字元 {slug_pattern} 期望 {want} 個，實際符合 {} 個",
                            matched.len()
                        ),
                    }),
                    Some(_) => {}
                }

                for found in matched {
                    check_instance_has_endpoint(
                        found,
                        endpoint.as_ref(),
                        env,
                        conn_id.clone(),
                        findings,
                    );
                }
            }
        },
        Endpointing::Infra { node, endpoint } => match env.infra_node(node) {
            None => findings.push(Finding {
                rule: Rule::L003,
                environment: env_id,
                subject: conn_id,
                detail: format!("連線指向不存在的設備 {node}"),
            }),
            Some(found) => {
                if let Some(want) = endpoint
                    && !found.endpoints.iter().any(|e| &e.id == want)
                {
                    findings.push(Finding {
                        rule: Rule::L003,
                        environment: env_id,
                        subject: conn_id,
                        detail: format!("設備 {} 上沒有 Endpoint {want}", found.slug),
                    });
                }
            }
        },
        Endpointing::System { instance, endpoint } => match env.system_instance(instance) {
            None => findings.push(Finding {
                rule: Rule::L003,
                environment: env_id,
                subject: conn_id,
                detail: format!("連線指向不存在的外部系統落地 {instance}"),
            }),
            Some(found) => {
                if let Some(want) = endpoint
                    && !found.endpoints.iter().any(|e| e.def.as_ref() == Some(want))
                {
                    findings.push(Finding {
                        rule: Rule::L003,
                        environment: env_id,
                        subject: conn_id,
                        detail: format!(
                            "外部系統 {} 上沒有對應 EndpointDef {want} 的 Endpoint",
                            found.slug
                        ),
                    });
                }
            }
        },
    }
}

fn check_instance_has_endpoint(
    instance: &ContainerInstance,
    endpoint: Option<&Id>,
    env: &Environment,
    conn_id: Id,
    findings: &mut Vec<Finding>,
) {
    // 來源端的 endpoint 可以是 None（由作業系統分配 ephemeral port）。
    let Some(want) = endpoint else { return };

    // `want` 指的是邏輯層的 EndpointDef，不是具體 Endpoint 的 id：
    // 萬用字元會展開成多台 Instance，各有各的具體 endpoint。
    if !instance
        .endpoints
        .iter()
        .any(|e| e.def.as_ref() == Some(want))
    {
        findings.push(Finding {
            rule: Rule::L003,
            environment: Some(env.id.clone()),
            subject: conn_id,
            detail: format!(
                "Instance {} 上沒有對應 EndpointDef {want} 的 Endpoint",
                instance.slug
            ),
        });
    }
}

/// L006：每個 Endpoint 都要有實際位址。
fn check_endpoint_addresses(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    for instance in &index.all {
        for endpoint in &instance.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    detail: format!("{}／{} 缺少位址", instance.slug, endpoint.slug),
                });
            }
        }
    }

    for node in &env.infra {
        for endpoint in &node.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    detail: format!("{}／{} 缺少位址", node.slug, endpoint.slug),
                });
            }
        }
    }

    for system in &env.systems {
        for endpoint in &system.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    detail: format!("{}／{} 缺少位址", system.slug, endpoint.slug),
                });
            }
        }
    }
}

/// L001 / L002：每條邏輯連線在每個環境都要有實現，而且要走得通。
///
/// 走迷宮：把貼同一個 `serves` 標籤的連線攤開成一張圖，
/// 從來源服務的任一 Instance 出發，看能不能走到目標服務的任一 Instance。
/// 這順便會抓到「F5 設了但後面忘了接」。
fn check_relationships_reachable(
    project: &Project,
    env: &Environment,
    index: &EnvIndex<'_>,
    findings: &mut Vec<Finding>,
) {
    for rel in &project.logical.relationships {
        let serving = index.serving(&rel.id);

        if serving.is_empty() {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: rel.id.clone(),
                detail: format!("邏輯連線 {} 在環境 {} 沒有任何實際連線", rel.slug, env.slug),
            });
            continue;
        }

        let starts = ends_to_nodes(env, &index.all, &rel.from);
        let goals: HashSet<GraphNode> = ends_to_nodes(env, &index.all, &rel.to)
            .into_iter()
            .collect();

        // 兩端的服務本身就沒實現時，L001 已經報過了，不再重複報 L002。
        if starts.is_empty() || goals.is_empty() {
            continue;
        }

        let mut edges: HashMap<GraphNode, Vec<GraphNode>> = HashMap::new();
        for conn in serving {
            let to = resolve(index, &conn.to);
            for from in resolve(index, &conn.from) {
                edges.entry(from).or_default().extend(to.iter().cloned());
            }
        }

        if !reachable(&edges, &starts, &goals) {
            findings.push(Finding {
                rule: Rule::L002,
                environment: Some(env.id.clone()),
                subject: rel.id.clone(),
                detail: format!(
                    "邏輯連線 {} 在環境 {} 走不通：從來源出發到不了目標",
                    rel.slug, env.slug
                ),
            });
        }
    }
}

/// 把**邏輯連線的一端**展開成圖上的點：該服務／外部系統在此環境的所有落地。
fn ends_to_nodes(
    env: &Environment,
    instances: &[&ContainerInstance],
    end: &RelationshipEnd,
) -> Vec<GraphNode> {
    match end {
        RelationshipEnd::Container(cid) => instances
            .iter()
            .filter(|i| &i.container == cid)
            .map(|i| GraphNode::Instance(i.id.clone()))
            .collect(),
        RelationshipEnd::System(sid) => env
            .systems
            .iter()
            .filter(|s| &s.system == sid)
            .map(|s| GraphNode::System(s.id.clone()))
            .collect(),
    }
}

/// 把**實際連線的一端**展開成圖上的點。萬用字元會展開成多個點。
fn resolve(index: &EnvIndex<'_>, side: &Endpointing) -> Vec<GraphNode> {
    match side {
        Endpointing::Instance { target, .. } => match target {
            InstanceRef::One(id) => vec![GraphNode::Instance(id.clone())],
            InstanceRef::Pattern { slug_pattern, .. } => index
                .matching(slug_pattern)
                .into_iter()
                .map(|i| GraphNode::Instance(i.id.clone()))
                .collect(),
        },
        Endpointing::Infra { node, .. } => vec![GraphNode::Infra(node.clone())],
        Endpointing::System { instance, .. } => vec![GraphNode::System(instance.clone())],
    }
}

/// 廣度優先搜尋：從任一起點出發，能否抵達任一終點。
fn reachable(
    edges: &HashMap<GraphNode, Vec<GraphNode>>,
    starts: &[GraphNode],
    goals: &HashSet<GraphNode>,
) -> bool {
    let mut seen: HashSet<&GraphNode> = HashSet::new();
    let mut queue: VecDeque<&GraphNode> = VecDeque::new();

    for start in starts {
        if goals.contains(start) {
            return true;
        }
        if seen.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(next_nodes) = edges.get(current) else {
            continue;
        };
        for next in next_nodes {
            if goals.contains(next) {
                return true;
            }
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }

    false
}

/// L008：沒有被任何連線碰到的 Instance。
///
/// 冷備機是合法情境，所以可以在該 Instance 標記 `standalone` 關掉這個警告。
fn check_orphan_instances(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    let mut touched: HashSet<Id> = HashSet::new();
    for conn in &env.connections {
        for side in [&conn.from, &conn.to] {
            for node in resolve(index, side) {
                match node {
                    GraphNode::Instance(id) | GraphNode::System(id) => {
                        touched.insert(id);
                    }
                    GraphNode::Infra(_) => {}
                }
            }
        }
    }

    for instance in &index.all {
        if instance.standalone || touched.contains(&instance.id) {
            continue;
        }
        findings.push(Finding {
            rule: Rule::L008,
            environment: Some(env.id.clone()),
            subject: instance.id.clone(),
            detail: format!("Instance {} 沒有被任何連線碰到", instance.slug),
        });
    }

    for system in &env.systems {
        if system.standalone || touched.contains(&system.id) {
            continue;
        }
        findings.push(Finding {
            rule: Rule::L008,
            environment: Some(env.id.clone()),
            subject: system.id.clone(),
            detail: format!("外部系統 {} 沒有被任何連線碰到", system.slug),
        });
    }
}
