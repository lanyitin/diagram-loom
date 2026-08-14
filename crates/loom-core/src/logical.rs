//! 邏輯層：母版，整個專案定義一次。
//!
//! 這一層只說「有哪些東西、誰要連誰」，不含任何 IP、port 或機器。
//! 具體的服務實體在 [`crate::environment`]。
//!
//! 命名對齊 C4 Model：這裡的 `Container` 是**服務**（Redis、Consul、App），
//! 不是機器。機器叫 `DeploymentNode`，在環境層。

use serde::{Deserialize, Serialize};

use crate::id::Id;

/// Endpoint 的協定種類。
///
/// 「Port」這個詞不夠準確——Unix socket、JDBC URL 與檔案都不是 port，
/// 它們的共通點是「服務對外的一個接點」，因此統一叫 Endpoint。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Tcp,
    Udp,
    UnixSocket,
    Jdbc,
    File,
}

impl std::fmt::Display for Protocol {
    /// 給畫面看的名字。序列化用的是 serde 的 kebab-case，兩者刻意分開——
    /// 檔案格式改了會壞掉，顯示文字改了不會。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Protocol::Tcp => "TCP",
            Protocol::Udp => "UDP",
            Protocol::UnixSocket => "Unix socket",
            Protocol::Jdbc => "JDBC",
            Protocol::File => "檔案",
        };
        f.write_str(s)
    }
}

/// 真人使用者。C4 的 `Person`，只出現在 Context 圖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Person {
    pub id: Id,
    pub slug: String,
    pub name: String,
    /// 使用者的備註。見 [`crate::memo_is_empty`]。
    #[serde(default, skip_serializing_if = "crate::memo_is_empty")]
    pub memo: String,
}

/// 軟體系統。可能是自家系統，也可能是外部系統（金流、簡訊商）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SoftwareSystem {
    pub id: Id,
    pub slug: String,
    pub name: String,
    /// 外部系統不由我們部署，但仍需在每個環境指定它的位址
    /// （例如測試環境用金流 sandbox）。
    pub external: bool,
    /// 外部系統對外的接點定義。
    ///
    /// C4 不把外部系統拆成 Container——我們看不到人家內部長怎樣，
    /// 只知道「有一個 API 可以打」。因此接點直接掛在系統上。
    /// 自家系統留空，它的接點由各個 [`Container`] 自己定義。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<EndpointDef>,
    /// 使用者的備註。見 [`crate::memo_is_empty`]。
    #[serde(default, skip_serializing_if = "crate::memo_is_empty")]
    pub memo: String,
}

impl SoftwareSystem {
    pub fn endpoint(&self, id: &Id) -> Option<&EndpointDef> {
        self.endpoints.iter().find(|e| &e.id == id)
    }
}

/// Endpoint 的定義：只有名字與協定，沒有位址。
///
/// 位址屬於環境層的 [`Endpoint`](crate::environment::Endpoint)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EndpointDef {
    pub id: Id,
    pub slug: String,
    pub protocol: Protocol,
    /// 使用者的備註。見 [`crate::memo_is_empty`]。
    #[serde(default, skip_serializing_if = "crate::memo_is_empty")]
    pub memo: String,
}

/// 一個會跑的服務：Redis、Consul、訂單 API。C4 的 `Container`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Container {
    pub id: Id,
    pub slug: String,
    pub name: String,
    pub system: Id,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<EndpointDef>,
    /// 使用者的備註。見 [`crate::memo_is_empty`]。
    #[serde(default, skip_serializing_if = "crate::memo_is_empty")]
    pub memo: String,
}

impl Container {
    pub fn endpoint(&self, id: &Id) -> Option<&EndpointDef> {
        self.endpoints.iter().find(|e| &e.id == id)
    }
}

/// 邏輯層的連線：「A 要連 B 的某個 endpoint」，並說明用途。
///
/// **這就是原本討論中的「連線契約」。** 我們刻意不另設 placeholder 概念——
/// 邏輯層宣告了一條連線，每個環境就必須實現它，沒實現就是 lint 錯誤。
///
/// 一條 Relationship 在不同環境會展開成**不同數量**的實際連線：
/// dev 直連是 1 段，prod 走 F5 是 2 段；Redis 叢集 prod 12 個節點、test 6 個。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Relationship {
    pub id: Id,
    pub slug: String,
    /// 用途說明。空白會觸發 L007。
    pub purpose: String,
    pub from: RelationshipEnd,
    pub to: RelationshipEnd,
    /// 連到目標的哪一個 [`EndpointDef`]。
    /// 目標是 Container 就查它的 endpoints，是外部系統就查系統的 endpoints。
    pub to_endpoint: Id,
    /// 使用者的備註。見 [`crate::memo_is_empty`]。
    ///
    /// 跟 `purpose` 分開：`purpose` 是 L007 在檢查的欄位，屬於「這條連線為什麼存在」；
    /// 備註是規則不管的雜項，寫進 `purpose` 會讓那個欄位變成雜物櫃。
    #[serde(default, skip_serializing_if = "crate::memo_is_empty")]
    pub memo: String,
}

/// 邏輯連線的一端：自家的服務、一整個外部系統，或一個真人。
///
/// 外部系統不拆成 Container，所以它整個就是一端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum RelationshipEnd {
    Container(Id),
    System(Id),
    /// 真人。**只該出現在來源端**——「使用者連上系統」是 C4 Context 圖
    /// 最常見的關係，沒有它整條進入點的流量就少了最前面那一段。
    ///
    /// 人不需要被部署，所以 L001 不會要求它在每個環境都有實體。
    Person(Id),
}

/// 邏輯層的全部內容。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Logical {
    pub people: Vec<Person>,
    pub systems: Vec<SoftwareSystem>,
    pub containers: Vec<Container>,
    pub relationships: Vec<Relationship>,
}

impl Logical {
    pub fn container(&self, id: &Id) -> Option<&Container> {
        self.containers.iter().find(|c| &c.id == id)
    }

    pub fn relationship(&self, id: &Id) -> Option<&Relationship> {
        self.relationships.iter().find(|r| &r.id == id)
    }

    pub fn system(&self, id: &Id) -> Option<&SoftwareSystem> {
        self.systems.iter().find(|s| &s.id == id)
    }

    /// 需要在每個環境都有實體的外部系統。
    ///
    /// 自家系統不列入：它是靠自己的 [`Container`] 部署的，
    /// 沒有獨立的「系統實例」。
    pub fn external_systems(&self) -> impl Iterator<Item = &SoftwareSystem> {
        self.systems.iter().filter(|s| s.external)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_container() -> Container {
        Container {
            id: Id::new("c-redis"),
            slug: "redis".into(),
            name: "Redis 快取".into(),
            system: Id::new("s-shop"),
            endpoints: vec![EndpointDef {
                id: Id::new("e-redis-client"),
                slug: "client-port".into(),
                protocol: Protocol::Tcp,
                memo: String::new(),
            }],
            memo: String::new(),
        }
    }

    #[test]
    fn finds_container_endpoints_by_id() {
        let redis = sample_container();
        let found = redis.endpoint(&Id::new("e-redis-client"));
        assert_eq!(found.map(|e| e.slug.as_str()), Some("client-port"));
    }

    #[test]
    fn missing_endpoint_returns_none() {
        let redis = sample_container();
        assert!(redis.endpoint(&Id::new("e-不存在")).is_none());
    }

    #[test]
    fn logical_layer_looks_elements_up_by_id() {
        let logical = Logical {
            containers: vec![sample_container()],
            ..Default::default()
        };
        assert!(logical.container(&Id::new("c-redis")).is_some());
        assert!(logical.container(&Id::new("c-consul")).is_none());
    }
}
