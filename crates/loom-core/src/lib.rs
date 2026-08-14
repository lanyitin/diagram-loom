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

// rustdoc 在這裡的用途是**檢查註解裡的連結指不指得到東西**（`mise run
// check:docs`，跑的是 `--document-private-items`），不是產一份要發佈的手冊。
// 註解要講清楚一件事，常常得指到私有的函式——那正是讀的人接下來要看的地方。
#![allow(rustdoc::private_intra_doc_links)]

pub mod annotate;
pub mod batch;
pub mod cascade;
pub mod connect;
pub mod coverage;
pub mod diagrams;
pub mod edit;
pub mod environment;
pub mod highlight;
pub mod history;
pub mod id;
pub mod importer;
mod index;
pub mod inventory;
pub mod lint;
pub mod logical;
pub mod pattern;
pub mod plan;
pub mod reconcile;
pub mod repository;
pub mod resource;
pub mod slug;
pub mod store;
pub mod table;
pub mod wiring;

use crate::environment::Environment;
use crate::id::Id;
use crate::logical::Logical;

/// 備註是空的。`skip_serializing_if` 用：沒寫備註就不要在 YAML 留下空字串。
///
/// # 為什麼每個模型元素都有 `memo`
///
/// 模型欄位是**規則要用的**：`purpose` 會被 L007 檢查、`expect` 會被 L005
/// 檢查、`standalone` 會關掉 L008 的警告。但真實世界的資訊不會剛好只有這些——
/// 「這台等年底汰換」「這條是舊系統遺留、問過 XX 說不能拆」這種話沒有欄位可放，
/// 人就會塞進 `slug` 或 `purpose` 裡，把有規則在讀的欄位弄髒。
///
/// `memo` 是那些話的去處，而且**刻意不參與任何 lint 判斷**。
/// 沒有規則讀它，就沒有人需要為了讓 lint 閉嘴而修改它。
///
/// 型別是 `String` 不是 `Option<String>`：空字串就是沒寫，
/// 這裡沒有「沒填」與「填了空的」之分，多一層 `Option` 只是讓每個讀的人多解一次。
pub fn memo_is_empty(memo: &str) -> bool {
    memo.is_empty()
}

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
    /// 使用者的備註。見 [`memo_is_empty`]。
    #[serde(default, skip_serializing_if = "memo_is_empty")]
    pub memo: String,
    pub logical: Logical,
    pub environments: Vec<Environment>,
}

impl Project {
    pub fn environment(&self, id: &Id) -> Option<&Environment> {
        self.environments.iter().find(|e| &e.id == id)
    }
}
