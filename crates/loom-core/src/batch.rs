//! 批次建立 Instance。
//!
//! 12 台 VM 各跑一個 Redis，手打 12 次很痛。這裡用「命名樣板 + 起始序號」
//! 一次生出來。
//!
//! # 樣板
//!
//! 樣板裡可以放兩種佔位符，兩者都能用在名稱、機器名與位址上：
//!
//! | 佔位符 | 意思 | 起始值 | 補零 |
//! | --- | --- | --- | --- |
//! | `{n}` | 序號 | [`BatchSpec::start`] | 依 [`BatchSpec::pad`] |
//! | `{ip}` | 位址序號 | [`BatchSpec::ip_start`] | 不補零 |
//!
//! 兩個計數器分開，是因為現實中兩者常常對不齊：
//! `redis-01`～`redis-12` 可能對應 `10.0.1.11`～`10.0.1.22`。
//!
//! ```
//! use loom_core::batch::{BatchSpec, EndpointPlan, expand};
//! use loom_core::environment::NodeKind;
//! use loom_core::id::Id;
//! use loom_core::logical::Protocol;
//!
//! let spec = BatchSpec {
//!     count: 12,
//!     name_template: "redis-{n}".into(),
//!     node_template: "vm-redis-{n}".into(),
//!     start: 1,
//!     pad: 2,
//!     address_template: "10.0.1.{ip}:6379".into(),
//!     ip_start: 11,
//!     node_kind: NodeKind::VirtualMachine,
//!     container: Id::new("c-redis"),
//!     endpoint: EndpointPlan {
//!         def: Id::new("e-redis-client"),
//!         slug: "client-port".into(),
//!         protocol: Protocol::Tcp,
//!     },
//! };
//!
//! let nodes = expand(&spec, |hint| Id::new(format!("id-{hint}"))).unwrap();
//! assert_eq!(nodes.len(), 12);
//! assert_eq!(nodes[0].instances[0].slug, "redis-01");
//! assert_eq!(
//!     nodes[11].instances[0].endpoints[0].address.as_deref(),
//!     Some("10.0.1.22:6379")
//! );
//! ```

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::environment::{ContainerInstance, DeploymentNode, Endpoint, NodeKind};
use crate::id::Id;
use crate::logical::Protocol;
use crate::slug;

/// 要在每個 Instance 上建立的 Endpoint。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct EndpointPlan {
    /// 對應邏輯層的 `EndpointDef`。
    pub def: Id,
    pub slug: String,
    pub protocol: Protocol,
}

/// 一次批次建立的規格。
///
/// 數字用 `u32` 而不是 `usize`：它們是「要建幾台」「從幾號開始」，
/// 不是記憶體索引。而且 `usize` 匯不出 TypeScript（specta 怕 BigInt 精度）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchSpec {
    pub count: u32,
    /// Instance 的名稱樣板，例如 `redis-{n}`。
    pub name_template: String,
    /// 承載機器的名稱樣板，例如 `vm-redis-{n}`。
    pub node_template: String,
    /// `{n}` 的起始值。
    pub start: u32,
    /// `{n}` 的補零寬度。`2` 會產生 `01`、`02`。
    pub pad: u32,
    /// 位址樣板，例如 `10.0.1.{ip}:6379`。
    pub address_template: String,
    /// `{ip}` 的起始值。
    pub ip_start: u32,
    pub node_kind: NodeKind,
    pub container: Id,
    pub endpoint: EndpointPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchError {
    /// 數量為零，沒有東西可建。
    EmptyCount,
    /// 樣板裡有不認識的佔位符。多半是打錯字（例如 `{num}`）。
    UnknownPlaceholder(String),
    /// 樣板裡的 `{` 沒有對應的 `}`。
    UnclosedPlaceholder,
    /// 產生出重複的名稱。通常是名稱樣板忘了放 `{n}`。
    DuplicateName(String),
    /// 這個環境裡已經有同名的機器或服務實體了。
    ///
    /// 同一個環境有兩台 `redis-01` 會讓萬用字元數到 2，
    /// 而使用者以為那是兩台不同的機器——正好是這個工具要防的誤會。
    Taken(String),
    /// 指定的服務不在邏輯層裡。
    NoSuchContainer(Id),
    /// 這個接點定義不屬於那個服務。
    EndpointNotOnContainer { container: String, endpoint: Id },
    /// 產生出的名稱不是正規的 slug。附上建議寫法。
    NotNormalized {
        produced: String,
        suggestion: String,
    },
}

impl fmt::Display for BatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatchError::EmptyCount => write!(f, "數量必須大於 0"),
            BatchError::UnknownPlaceholder(name) => {
                write!(
                    f,
                    "樣板裡有不認識的佔位符 {{{name}}}，只能用 {{n}} 或 {{ip}}"
                )
            }
            BatchError::UnclosedPlaceholder => write!(f, "樣板裡的 {{ 沒有對應的 }}"),
            BatchError::Taken(name) => {
                write!(f, "{name} 在這個環境已經有了")
            }
            BatchError::NoSuchContainer(id) => write!(f, "找不到服務 {id}"),
            BatchError::EndpointNotOnContainer {
                container,
                endpoint,
            } => write!(f, "服務 {container} 上沒有接點定義 {endpoint}"),
            BatchError::DuplicateName(name) => {
                write!(f, "產生出重複的名稱 {name}，名稱樣板需要包含 {{n}}")
            }
            BatchError::NotNormalized {
                produced,
                suggestion,
            } => write!(f, "名稱 {produced} 不是正規寫法，建議改成 {suggestion}"),
        }
    }
}

impl std::error::Error for BatchError {}

/// 依規格展開成一批機器，每台機器上放一個 Instance。
///
/// `new_id` 是識別碼工廠：傳入的是名稱提示，回傳該元素的 [`Id`]。
/// 目前還沒接真正的 UUID（見 [`crate::id`]），把產生方式留給呼叫端，
/// 之後換成 UUID 產生器時這裡不用改。
pub fn expand(
    spec: &BatchSpec,
    mut new_id: impl FnMut(&str) -> Id,
) -> Result<Vec<DeploymentNode>, BatchError> {
    if spec.count == 0 {
        return Err(BatchError::EmptyCount);
    }

    let mut nodes = Vec::with_capacity(spec.count as usize);
    let mut seen_names = HashSet::new();

    for offset in 0..spec.count {
        let n = spec.start + offset;
        let ip = spec.ip_start + offset;

        let instance_slug = render(&spec.name_template, n, ip, spec.pad)?;
        check_normalized(&instance_slug)?;
        if !seen_names.insert(instance_slug.clone()) {
            return Err(BatchError::DuplicateName(instance_slug));
        }

        let node_slug = render(&spec.node_template, n, ip, spec.pad)?;
        check_normalized(&node_slug)?;

        let address = render(&spec.address_template, n, ip, spec.pad)?;

        let endpoint = Endpoint {
            id: new_id(&format!("ep-{instance_slug}")),
            slug: spec.endpoint.slug.clone(),
            def: Some(spec.endpoint.def.clone()),
            protocol: spec.endpoint.protocol,
            address: Some(address),
        };

        let instance = ContainerInstance {
            id: new_id(&format!("i-{instance_slug}")),
            slug: instance_slug,
            container: spec.container.clone(),
            endpoints: vec![endpoint],
            standalone: false,
        };

        nodes.push(DeploymentNode {
            id: new_id(&format!("n-{node_slug}")),
            slug: node_slug,
            kind: spec.node_kind,
            children: vec![],
            instances: vec![instance],
        });
    }

    Ok(nodes)
}

/// 把樣板裡的 `{n}` 與 `{ip}` 換成實際數字。
fn render(template: &str, n: u32, ip: u32, pad: u32) -> Result<String, BatchError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];

        let close = after.find('}').ok_or(BatchError::UnclosedPlaceholder)?;
        match &after[..close] {
            "n" => {
                // 補零寬度是格式參數，只吃 usize。
                let pad = pad as usize;
                out.push_str(&format!("{n:0pad$}"))
            }
            "ip" => out.push_str(&ip.to_string()),
            other => return Err(BatchError::UnknownPlaceholder(other.to_string())),
        }

        rest = &after[close + 1..];
    }

    out.push_str(rest);
    Ok(out)
}

/// 名稱必須是正規的 slug——它會進到檔名、萬用字元樣式與匯入比對。
///
/// 這裡選擇報錯而不是自動修正：悄悄改掉使用者輸入的名字，
/// 之後對不上時會很難查。
fn check_normalized(name: &str) -> Result<(), BatchError> {
    if slug::is_normalized(name) {
        return Ok(());
    }
    Err(BatchError::NotNormalized {
        produced: name.to_string(),
        suggestion: slug::slugify(name),
    })
}

/// 展開之後的樣子，加上它會放在哪。
///
/// # 為什麼 id 在這裡就發好
///
/// 跟 [`crate::connect::propose`] 同一個理由：`apply` 必須是決定性的。
/// 若 id 等到套用時才產生，預覽給使用者看的就不是他真正會拿到的東西，
/// 而復原之後重做也會得到一批不同 id 的機器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BatchPlan {
    pub nodes: Vec<DeploymentNode>,
    /// 給預覽看的一行行摘要：`vm-redis-01 / redis-01 @ 10.0.1.11:6379`。
    pub preview: Vec<String>,
}

/// 算出「這批會建出什麼」，並且擋掉會撞名的。**不改動任何東西。**
///
/// 檢查放在這裡而不是 [`expand`]：`expand` 只認得樣板，
/// 「這個環境已經有一台 redis-01 了」要看環境才知道。
pub fn plan(
    project: &crate::Project,
    env: &crate::environment::Environment,
    spec: &BatchSpec,
) -> Result<BatchPlan, BatchError> {
    let container = project
        .logical
        .container(&spec.container)
        .ok_or_else(|| BatchError::NoSuchContainer(spec.container.clone()))?;

    if container.endpoint(&spec.endpoint.def).is_none() {
        return Err(BatchError::EndpointNotOnContainer {
            container: container.slug.clone(),
            endpoint: spec.endpoint.def.clone(),
        });
    }

    let nodes = expand(spec, |_hint| Id::generate())?;

    // 撞名一律擋下來。同一個環境有兩台 redis-01 會讓萬用字元數到 2，
    // 而使用者以為那是兩台不同的機器——正好是這個工具要防的誤會。
    let existing_nodes: HashSet<&str> = all_node_slugs(&env.nodes);
    let existing_instances: HashSet<&str> = env
        .instances()
        .into_iter()
        .map(|i| i.slug.as_str())
        .collect();
    for node in &nodes {
        if existing_nodes.contains(node.slug.as_str()) {
            return Err(BatchError::Taken(node.slug.clone()));
        }
        for i in &node.instances {
            if existing_instances.contains(i.slug.as_str()) {
                return Err(BatchError::Taken(i.slug.clone()));
            }
        }
    }

    let preview = nodes
        .iter()
        .flat_map(|n| {
            n.instances.iter().map(move |i| {
                let address_of = i
                    .endpoints
                    .first()
                    .and_then(|e| e.address.as_deref())
                    .unwrap_or("（沒有位址）");
                format!("{} / {} @ {}", n.slug, i.slug, address_of)
            })
        })
        .collect();

    Ok(BatchPlan { nodes, preview })
}

fn all_node_slugs(nodes: &[DeploymentNode]) -> HashSet<&str> {
    let mut out = HashSet::new();
    for n in nodes {
        out.insert(n.slug.as_str());
        out.extend(all_node_slugs(&n.children));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(count: u32) -> BatchSpec {
        BatchSpec {
            count,
            name_template: "redis-{n}".into(),
            node_template: "vm-redis-{n}".into(),
            start: 1,
            pad: 2,
            address_template: "10.0.1.{ip}:6379".into(),
            ip_start: 11,
            node_kind: NodeKind::VirtualMachine,
            container: Id::new("c-redis"),
            endpoint: EndpointPlan {
                def: Id::new("e-redis-client"),
                slug: "client-port".into(),
                protocol: Protocol::Tcp,
            },
        }
    }

    fn expanded(spec: &BatchSpec) -> Result<Vec<DeploymentNode>, BatchError> {
        expand(spec, |hint| Id::new(hint))
    }

    #[test]
    fn creates_the_requested_number_of_nodes_one_instance_each() {
        let nodes = expanded(&spec(12)).unwrap();
        assert_eq!(nodes.len(), 12);
        assert!(nodes.iter().all(|n| n.instances.len() == 1));
    }

    #[test]
    fn sequence_number_is_padded() {
        let nodes = expanded(&spec(12)).unwrap();
        assert_eq!(nodes[0].instances[0].slug, "redis-01");
        assert_eq!(nodes[8].instances[0].slug, "redis-09");
        assert_eq!(nodes[9].instances[0].slug, "redis-10");
    }

    #[test]
    fn address_counter_is_separate_and_unpadded() {
        let nodes = expanded(&spec(12)).unwrap();
        let address_of = |i: usize| nodes[i].instances[0].endpoints[0].address.clone();
        assert_eq!(address_of(0).as_deref(), Some("10.0.1.11:6379"));
        assert_eq!(address_of(11).as_deref(), Some("10.0.1.22:6379"));
    }

    #[test]
    fn node_slug_and_instance_slug_are_separate() {
        let nodes = expanded(&spec(2)).unwrap();
        assert_eq!(nodes[0].slug, "vm-redis-01");
        assert_eq!(nodes[0].instances[0].slug, "redis-01");
    }

    #[test]
    fn both_placeholders_work_in_every_template() {
        let mut spec = spec(2);
        spec.name_template = "redis-{n}-{ip}".into();
        let nodes = expanded(&spec).unwrap();
        assert_eq!(nodes[0].instances[0].slug, "redis-01-11");
    }

    #[test]
    fn zero_count_is_an_error() {
        assert_eq!(expanded(&spec(0)), Err(BatchError::EmptyCount));
    }

    #[test]
    fn template_without_sequence_number_duplicates() {
        let mut spec = spec(3);
        spec.name_template = "redis".into();
        assert_eq!(
            expanded(&spec),
            Err(BatchError::DuplicateName("redis".into()))
        );
    }

    #[test]
    fn single_item_needs_no_sequence_number() {
        let mut spec = spec(1);
        spec.name_template = "redis".into();
        spec.node_template = "vm-redis".into();
        assert_eq!(expanded(&spec).unwrap()[0].instances[0].slug, "redis");
    }

    #[test]
    fn unknown_placeholder_is_rejected() {
        let mut spec = spec(1);
        spec.name_template = "redis-{num}".into();
        assert_eq!(
            expanded(&spec),
            Err(BatchError::UnknownPlaceholder("num".into()))
        );
    }

    #[test]
    fn unclosed_brace_is_rejected() {
        let mut spec = spec(1);
        spec.name_template = "redis-{n".into();
        assert_eq!(expanded(&spec), Err(BatchError::UnclosedPlaceholder));
    }

    #[test]
    fn rejects_unnormalised_names_with_a_suggestion() {
        let mut spec = spec(1);
        spec.name_template = "Redis Node {n}".into();
        assert_eq!(
            expanded(&spec),
            Err(BatchError::NotNormalized {
                produced: "Redis Node 01".into(),
                suggestion: "redis-node-01".into(),
            })
        );
    }

    #[test]
    fn address_template_is_not_slug_normalised() {
        // 位址可以是 JDBC URL、socket 路徑、檔案路徑，不該被當成 slug 檢查。
        let mut spec = spec(1);
        spec.address_template = "jdbc:oracle:thin:@10.0.2.{ip}:1521/ORCL".into();
        let nodes = expanded(&spec).unwrap();
        assert_eq!(
            nodes[0].instances[0].endpoints[0].address.as_deref(),
            Some("jdbc:oracle:thin:@10.0.2.11:1521/ORCL")
        );
    }

    #[test]
    fn the_caller_decides_the_id() {
        let mut count = 0;
        let nodes = expand(&spec(1), |hint| {
            count += 1;
            Id::new(format!("uuid-{hint}"))
        })
        .unwrap();

        // 每個實例會要三個 id：endpoint、instance、node
        assert_eq!(count, 3);
        assert_eq!(nodes[0].id, Id::new("uuid-n-vm-redis-01"));
        assert_eq!(nodes[0].instances[0].id, Id::new("uuid-i-redis-01"));
    }
}
