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

use crate::environment::{ContainerInstance, DeploymentNode, Endpoint, NodeKind};
use crate::id::Id;
use crate::logical::Protocol;
use crate::slug;

/// 要在每個 Instance 上建立的 Endpoint。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointPlan {
    /// 對應邏輯層的 `EndpointDef`。
    pub def: Id,
    pub slug: String,
    pub protocol: Protocol,
}

/// 一次批次建立的規格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSpec {
    pub count: usize,
    /// Instance 的名稱樣板，例如 `redis-{n}`。
    pub name_template: String,
    /// 承載機器的名稱樣板，例如 `vm-redis-{n}`。
    pub node_template: String,
    /// `{n}` 的起始值。
    pub start: usize,
    /// `{n}` 的補零寬度。`2` 會產生 `01`、`02`。
    pub pad: usize,
    /// 位址樣板，例如 `10.0.1.{ip}:6379`。
    pub address_template: String,
    /// `{ip}` 的起始值。
    pub ip_start: usize,
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

    let mut nodes = Vec::with_capacity(spec.count);
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
fn render(template: &str, n: usize, ip: usize, pad: usize) -> Result<String, BatchError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];

        let close = after.find('}').ok_or(BatchError::UnclosedPlaceholder)?;
        match &after[..close] {
            "n" => out.push_str(&format!("{n:0pad$}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn 規格(count: usize) -> BatchSpec {
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

    fn 展開(spec: &BatchSpec) -> Result<Vec<DeploymentNode>, BatchError> {
        expand(spec, |hint| Id::new(hint))
    }

    #[test]
    fn 產生指定數量的機器每台一個實例() {
        let nodes = 展開(&規格(12)).unwrap();
        assert_eq!(nodes.len(), 12);
        assert!(nodes.iter().all(|n| n.instances.len() == 1));
    }

    #[test]
    fn 序號補零() {
        let nodes = 展開(&規格(12)).unwrap();
        assert_eq!(nodes[0].instances[0].slug, "redis-01");
        assert_eq!(nodes[8].instances[0].slug, "redis-09");
        assert_eq!(nodes[9].instances[0].slug, "redis-10");
    }

    #[test]
    fn 位址用自己的計數器不補零() {
        let nodes = 展開(&規格(12)).unwrap();
        let 位址 = |i: usize| nodes[i].instances[0].endpoints[0].address.clone();
        assert_eq!(位址(0).as_deref(), Some("10.0.1.11:6379"));
        assert_eq!(位址(11).as_deref(), Some("10.0.1.22:6379"));
    }

    #[test]
    fn 機器名稱與實例名稱分開() {
        let nodes = 展開(&規格(2)).unwrap();
        assert_eq!(nodes[0].slug, "vm-redis-01");
        assert_eq!(nodes[0].instances[0].slug, "redis-01");
    }

    #[test]
    fn 兩種佔位符都能用在任何樣板上() {
        let mut spec = 規格(2);
        spec.name_template = "redis-{n}-{ip}".into();
        let nodes = 展開(&spec).unwrap();
        assert_eq!(nodes[0].instances[0].slug, "redis-01-11");
    }

    #[test]
    fn 數量為零視為錯誤() {
        assert_eq!(展開(&規格(0)), Err(BatchError::EmptyCount));
    }

    #[test]
    fn 名稱樣板忘了放序號會產生重複() {
        let mut spec = 規格(3);
        spec.name_template = "redis".into();
        assert_eq!(展開(&spec), Err(BatchError::DuplicateName("redis".into())));
    }

    #[test]
    fn 只建一個時沒有序號也可以() {
        let mut spec = 規格(1);
        spec.name_template = "redis".into();
        spec.node_template = "vm-redis".into();
        assert_eq!(展開(&spec).unwrap()[0].instances[0].slug, "redis");
    }

    #[test]
    fn 打錯佔位符會被擋下() {
        let mut spec = 規格(1);
        spec.name_template = "redis-{num}".into();
        assert_eq!(
            展開(&spec),
            Err(BatchError::UnknownPlaceholder("num".into()))
        );
    }

    #[test]
    fn 大括號沒關會被擋下() {
        let mut spec = 規格(1);
        spec.name_template = "redis-{n".into();
        assert_eq!(展開(&spec), Err(BatchError::UnclosedPlaceholder));
    }

    #[test]
    fn 名稱不是正規寫法時報錯並給建議() {
        let mut spec = 規格(1);
        spec.name_template = "Redis Node {n}".into();
        assert_eq!(
            展開(&spec),
            Err(BatchError::NotNormalized {
                produced: "Redis Node 01".into(),
                suggestion: "redis-node-01".into(),
            })
        );
    }

    #[test]
    fn 位址樣板不受slug正規化限制() {
        // 位址可以是 JDBC URL、socket 路徑、檔案路徑，不該被當成 slug 檢查。
        let mut spec = 規格(1);
        spec.address_template = "jdbc:oracle:thin:@10.0.2.{ip}:1521/ORCL".into();
        let nodes = 展開(&spec).unwrap();
        assert_eq!(
            nodes[0].instances[0].endpoints[0].address.as_deref(),
            Some("jdbc:oracle:thin:@10.0.2.11:1521/ORCL")
        );
    }

    #[test]
    fn 識別碼由呼叫端決定() {
        let mut 次數 = 0;
        let nodes = expand(&規格(1), |hint| {
            次數 += 1;
            Id::new(format!("uuid-{hint}"))
        })
        .unwrap();

        // 每個實例會要三個 id：endpoint、instance、node
        assert_eq!(次數, 3);
        assert_eq!(nodes[0].id, Id::new("uuid-n-vm-redis-01"));
        assert_eq!(nodes[0].instances[0].id, Id::new("uuid-i-redis-01"));
    }
}
