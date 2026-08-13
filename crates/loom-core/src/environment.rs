//! 環境層：邏輯層母版在某個環境的落地。
//!
//! prod / test / dev 各是一個 [`Environment`]，種類開放不限這三種。
//! 同一個邏輯服務在不同環境的 IP、port、節點數都可以不同。

use serde::{Deserialize, Serialize};

use crate::id::Id;
use crate::logical::Protocol;

/// 運算載體的種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    /// 機房、園區、可用區——**裝別的節點用的**，本身不跑東西。
    ///
    /// C4 的 Deployment Node 本來就可巢狀，站點是它最常見的外層用法。
    /// 沒有這個種類的話，「主中心」只能勉強標成實體機，畫出來會變成一台機器。
    Site,
    Physical,
    VirtualMachine,
    LinuxContainer,
}

/// Endpoint 在某環境的實際樣貌：定義加上位址。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Endpoint {
    pub id: Id,
    pub slug: String,
    /// 對應邏輯層的 `EndpointDef`。
    /// [`InfrastructureNode`] 的 endpoint 沒有邏輯層對應，此處為 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def: Option<Id>,
    pub protocol: Protocol,
    /// `10.0.1.11:6379` / JDBC URL / socket 路徑 / 檔案路徑。
    /// 缺少會觸發 L006，所以必須允許 `None`。
    pub address: Option<String>,
}

/// 邏輯 Container 在此環境的一份落地。C4 的 `Container Instance`。
///
/// 叢集就是多個 Instance：12 台 VM 各跑一個 Redis process
/// 就是 12 個 `ContainerInstance`。節點數不另外存數字，避免兩份資料不一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContainerInstance {
    pub id: Id,
    pub slug: String,
    pub container: Id,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<Endpoint>,
    /// 標記「刻意獨立」，關閉 L008（沒被任何連線碰到）的警告。
    /// 冷備機是合法情境，但預設應該要叫。
    #[serde(default, skip_serializing_if = "is_false")]
    pub standalone: bool,
}

/// 機器：實體機、VM、Linux container。C4 的 `Deployment Node`，可巢狀。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeploymentNode {
    pub id: Id,
    pub slug: String,
    pub kind: NodeKind,
    /// 巢狀：機房 → 機器 → 容器。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DeploymentNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<ContainerInstance>,
}

impl DeploymentNode {
    /// 走訪自己與所有子孫節點上的 Instance。
    pub fn instances_recursive(&self) -> Vec<&ContainerInstance> {
        let mut found: Vec<&ContainerInstance> = self.instances.iter().collect();
        for child in &self.children {
            found.extend(child.instances_recursive());
        }
        found
    }
}

/// 外部系統在此環境的落地。C4 的 `Software System Instance`。
///
/// 例如金流系統：prod 用正式閘道，test 用 sandbox。
/// 它**不放在 [`DeploymentNode`] 底下**——那些機器不是我們的，我們只知道位址。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SoftwareSystemInstance {
    pub id: Id,
    pub slug: String,
    pub system: Id,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<Endpoint>,
    /// 同 [`ContainerInstance::standalone`]，關閉 L008 警告。
    #[serde(default, skip_serializing_if = "is_false")]
    pub standalone: bool,
}

/// F5 等 VIP 設備。C4 的 `Infrastructure Node`。
///
/// 它**不含 Container**，但有自己的 endpoint（VIP 位址），
/// 因此連線可以「經過」它——這也是為什麼經過 F5 的流量會拆成兩段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InfrastructureNode {
    pub id: Id,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<Endpoint>,
}

/// 連線一端指向的 Instance：可以是一個，也可以是一整群。
/// 連線的哪一端。
///
/// # 為什麼需要指名
///
/// 一條連線的兩端**都可能是萬用字元**（`apigw-* → common-*`），
/// 所以「這條連線的期望數量不對」是個不完整的說法——它沒說是哪一端。
/// Lint 的發現、以及照著發現去修的那次編輯，都必須指名，
/// 否則就得用猜的，而猜錯會安靜地改到另一端。
///
/// 名字不叫 `Side`，是為了不跟 [`crate::table::Side`]（表格上一端的完整樣貌）
/// 撞名——型別匯出到 TypeScript 之後是同一個命名空間，撞了就產不出來。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionEnd {
    From,
    To,
}

impl std::fmt::Display for ConnectionEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionEnd::From => write!(f, "來源"),
            ConnectionEnd::To => write!(f, "目標"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum InstanceRef {
    /// 指名一個 Instance。
    One(Id),
    /// 萬用字元，例如 `redis-*`。
    ///
    /// `expect` 是期望數量。**它必須是 `Option`**：萬用字元表示「現在有幾台就
    /// 連幾台」，少建一台 VM 會看起來完全正常，正好吃掉本工具的核心價值。
    /// 因此沒填 `expect` 要能被 lint 抓出來（L005），型別就不能強制它存在。
    Pattern {
        slug_pattern: String,
        /// 只算這個 [`DeploymentNode`] 底下的（含所有子孫）。
        ///
        /// # 為什麼需要它
        ///
        /// `redis-* expect 6` 只數總量。有人把一台機器從主中心搬到異地，
        /// 總數還是 6，**lint 不會叫**——而那正是這個工具存在的理由。
        ///
        /// 拆成兩條「主中心 3 台」「異地 3 台」就抓得到。而站點資訊
        /// **本來就在模型裡**（Instance 住在 VM 裡，VM 住在站點裡），
        /// 所以不需要新的 Cluster 概念，只需要能限定範圍。
        ///
        /// 順帶好處：站點不必再編進 slug（`redis-main-01`）。
        /// 名字編了資訊就會說謊——機器搬家之後 slug 不會自己改。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<Id>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// 用 `u32` 而不是 `usize`：這是「期望有幾台機器」，不是記憶體索引。
        /// 而且 `usize` 不能匯出成 TypeScript（specta 怕 BigInt 精度問題）。
        expect: Option<u32>,
    },
}

/// 連線的一端。
///
/// YAML 上長這樣（`serves` 相同的連線串起來就是一條路徑）：
///
/// ```yaml
/// from:
///   instance:
///     target: { one: i-prod-api-01 }   # 來源不填 endpoint = OS 分配
/// to:
///   infra:
///     node: f5-prod
///     endpoint: ep-f5-redis
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum Endpointing {
    Instance {
        /// 欄位叫 `target` 而不是 `instance`，否則 YAML 會出現
        /// `instance: { instance: ... }` 這種讀不下去的巢狀。
        target: InstanceRef,
        /// 指向**邏輯層的 [`EndpointDef`](crate::logical::EndpointDef)**，
        /// 不是某一台的具體 [`Endpoint`]。
        ///
        /// 因為萬用字元會展開成多台 Instance，各有各的具體 endpoint；
        /// 只有邏輯層的定義才是它們共通的東西。實際位址由各 Instance
        /// 上 `def` 對應的那個 endpoint 提供。
        ///
        /// 來源端可為 `None`，表示由作業系統分配（ephemeral port）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<Id>,
    },
    Infra {
        node: Id,
        /// 指向該設備上的具體 [`Endpoint`] id。
        ///
        /// 與 Instance 端不對稱是刻意的：設備不對應任何邏輯層元素，
        /// 它的 endpoint（VIP 位址）沒有 `EndpointDef` 可以指。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<Id>,
    },
    /// 外部系統。不支援萬用字元——一個外部系統在一個環境就是一個落地。
    System {
        instance: Id,
        /// 同 Instance 端，指向邏輯層的 `EndpointDef`（掛在
        /// [`SoftwareSystem`](crate::logical::SoftwareSystem) 上）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<Id>,
    },
    /// 真人。只會出現在**來源端**——使用者是流量的起點。
    ///
    /// 人沒有落地也沒有位址，所以這一端只指向邏輯層的
    /// [`Person`](crate::logical::Person)，沒有 endpoint。
    /// 「使用者從哪裡連過來」不是我們配置得到的東西。
    Person { person: Id },
}

/// 環境層的一條實際連線。
///
/// `serves` 指向它所服務的邏輯 [`Relationship`](crate::logical::Relationship)。
/// Lint 把貼同一個 `serves` 的連線攤開成一張圖，檢查從來源走不走得到目標。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Connection {
    pub id: Id,
    pub serves: Id,
    /// 用途說明。空白會觸發 L007。
    pub purpose: String,
    /// 這是平常走的路，還是只在故障時走的。
    ///
    /// 「同中心優先、某中心出問題才交叉」這種安排會讓一條契約長出四條連線，
    /// 其中兩條是備援。沒有這個欄位的話四條看起來一樣重，圖上會很吵，
    /// 讀的人也分不出平常的資料流是哪幾條。
    #[serde(default, skip_serializing_if = "ConnectionKind::is_primary")]
    pub kind: ConnectionKind,
    pub from: Endpointing,
    pub to: Endpointing,
}

/// 連線是正常路徑還是備援路徑。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionKind {
    /// 平常就在走的路。預設。
    #[default]
    Primary,
    /// 只在故障時才走。**它一樣要被建立、防火牆一樣要開**——
    /// 所以 lint 對它的要求跟正常路徑完全相同，差別只在畫面上的呈現。
    Fallback,
}

impl ConnectionKind {
    /// `skip_serializing_if` 用：預設值不必寫進 YAML。
    fn is_primary(&self) -> bool {
        matches!(self, ConnectionKind::Primary)
    }
}

/// 一個部署環境。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Environment {
    pub id: Id,
    pub slug: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<DeploymentNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub infra: Vec<InfrastructureNode>,
    /// 外部系統在此環境的落地。不在 `nodes` 底下，因為那些機器不是我們的。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<SoftwareSystemInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<Connection>,
}

impl Environment {
    /// 這個環境裡所有的 Instance，含巢狀節點底下的。
    pub fn instances(&self) -> Vec<&ContainerInstance> {
        self.nodes
            .iter()
            .flat_map(|n| n.instances_recursive())
            .collect()
    }

    pub fn instance(&self, id: &Id) -> Option<&ContainerInstance> {
        self.instances().into_iter().find(|i| &i.id == id)
    }

    pub fn infra_node(&self, id: &Id) -> Option<&InfrastructureNode> {
        self.infra.iter().find(|n| &n.id == id)
    }

    pub fn system_instance(&self, id: &Id) -> Option<&SoftwareSystemInstance> {
        self.systems.iter().find(|s| &s.id == id)
    }

    /// 符合萬用字元樣式的所有 Instance。
    pub fn instances_matching(&self, pattern: &str) -> Vec<&ContainerInstance> {
        self.instances()
            .into_iter()
            .filter(|i| crate::pattern::matches(pattern, &i.slug))
            .collect()
    }
}

/// `skip_serializing_if` 用：`false` 是預設值，不必寫進 YAML。
fn is_false(value: &bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 實例(slug: &str) -> ContainerInstance {
        ContainerInstance {
            id: Id::new(format!("i-{slug}")),
            slug: slug.into(),
            container: Id::new("c-redis"),
            endpoints: vec![],
            standalone: false,
        }
    }

    fn 機器(slug: &str, instances: Vec<ContainerInstance>) -> DeploymentNode {
        DeploymentNode {
            id: Id::new(format!("n-{slug}")),
            slug: slug.into(),
            kind: NodeKind::VirtualMachine,
            children: vec![],
            instances,
        }
    }

    fn 環境(nodes: Vec<DeploymentNode>) -> Environment {
        Environment {
            id: Id::new("env-prod"),
            slug: "prod".into(),
            name: "正式環境".into(),
            nodes,
            infra: vec![],
            systems: vec![],
            connections: vec![],
        }
    }

    #[test]
    fn 走訪巢狀節點底下的所有實例() {
        let 內層 = 機器("docker-host", vec![實例("redis-01")]);
        let mut 外層 = 機器("rack-a", vec![實例("consul-01")]);
        外層.children.push(內層);

        let env = 環境(vec![外層]);
        let mut slugs: Vec<_> = env.instances().iter().map(|i| i.slug.clone()).collect();
        slugs.sort();
        assert_eq!(slugs, vec!["consul-01", "redis-01"]);
    }

    #[test]
    fn 萬用字元選出整群實例() {
        let env = 環境(vec![機器(
            "vm",
            vec![實例("redis-01"), 實例("redis-02"), 實例("consul-01")],
        )]);
        assert_eq!(env.instances_matching("redis-*").len(), 2);
        assert_eq!(env.instances_matching("consul-*").len(), 1);
        assert_eq!(env.instances_matching("oracle-*").len(), 0);
    }

    #[test]
    fn 可以用_id_找到實例與設備() {
        let env = 環境(vec![機器("vm", vec![實例("redis-01")])]);
        assert!(env.instance(&Id::new("i-redis-01")).is_some());
        assert!(env.instance(&Id::new("i-沒有這台")).is_none());
    }
}
