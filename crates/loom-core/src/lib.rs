//! diagram-loom 的領域模型、驗證與 lint。
//!
//! # 紀律
//!
//! 這個 crate **不得相依 `tauri`**。所有領域邏輯都必須能用 `cargo test`
//! 驗證，不需要開視窗、不需要畫面。Tauri 那層只負責把請求轉進來。
//!
//! # 兩層模型
//!
//! - [`logical`] 是母版：有哪些服務、誰要連誰。定義一次。
//! - [`environment`] 是分身：實際跑在哪台機器、IP 是什麼、幾個節點。每個環境一份。
//!
//! **通則：邏輯層有的東西，每個環境都必須實現，沒實現就是 lint 錯誤。**
//!
//! # 為什麼型別允許「錯誤的狀態」
//!
//! 一般會希望用型別讓非法狀態無法表示。但本工具的核心價值就是**指出哪裡缺漏**，
//! 所以缺漏必須是可以表示的：`Endpoint::address` 是 `Option`、萬用字元的
//! `expect` 是 `Option`。型別若強制它們存在，對應的 lint 規則就永遠不會觸發。

pub mod batch;
pub mod connect;
pub mod coverage;
pub mod edit;
pub mod environment;
pub mod history;
pub mod id;
pub mod importer;
mod index;
pub mod lint;
pub mod logical;
pub mod pattern;
pub mod plan;
pub mod reconcile;
pub mod repository;
pub mod slug;
pub mod store;
pub mod table;

use crate::environment::Environment;
use crate::id::Id;
use crate::logical::Logical;

/// 一個專案：一份邏輯層母版，加上多個環境。
/// 一個完整的專案。
///
/// 存檔時會被 [`repository`] 拆成多個 YAML 檔，所以磁碟上沒有「一個 Project 檔」。
/// 但它整份會跨過 Tauri 邊界送到前端當唯讀鏡像，因此需要序列化。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Project {
    pub id: Id,
    pub slug: String,
    pub name: String,
    pub logical: Logical,
    pub environments: Vec<Environment>,
}

impl Project {
    pub fn environment(&self, id: &Id) -> Option<&Environment> {
        self.environments.iter().find(|e| &e.id == id)
    }
}
