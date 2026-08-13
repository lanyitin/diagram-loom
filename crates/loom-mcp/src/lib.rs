//! 給 AI Agent 用的 MCP 介面。
//!
//! # 這是「另一個前端」，不是另一個程式
//!
//! 桌面 App 給人用，這裡給 Agent 用，**兩者共用同一份 `loom-core`**。
//! Agent 建東西一樣要過 slug 唯一、參照完整那些檢查——規則只有一套。
//!
//! 而且 Agent 操作的就是**使用者當下開著的那份專案**，不是另外開一份：
//!
//! - Agent 改的東西，使用者按 ⌘Z 退得掉（走同一個 [`History`](loom_core::history::History)）
//! - 畫面即時更新，使用者看得到它在做什麼
//! - **不自動存檔**——標題列會亮起「未儲存」，由人決定要不要留
//!
//! 最後一條是刻意的。Agent 會弄錯，而這個工具的整個賣點就是「怕漏」；
//! 讓它直接寫進磁碟等於把最後一道人工檢查拿掉。
//!
//! # 為什麼 lint 是這裡最重要的工具
//!
//! Agent 從一段文字建模型一定會漏東西——漏接點、漏某個環境、漏一段路徑。
//! `lint` 讓它**自己發現自己漏了什麼**，然後補。
//!
//! 這正是這個工具本來就在做的事，只是使用者換成了 Agent。

pub mod protocol;
pub mod refs;
pub mod tools;

use loom_core::Project;
use loom_core::edit::Edit;

/// MCP 這一層需要對專案做的事。
///
/// # 為什麼是一個 port
///
/// 這個 crate 不知道 Tauri 存在，也不知道專案存在哪裡。它只知道
/// 「有一份專案可以看，而且可以對它下 `Edit`」。
///
/// 桌面 App 用它自己的 [`History`](loom_core::history::History) 實作，
/// 所以 Agent 的每一次修改都自動進了同一條復原鏈。測試用一個最小的
/// 假實作，於是整組工具都能用 `cargo test` 驗證，不必開視窗也不必跑 server。
pub trait Workspace {
    /// 目前開著的專案。沒開專案時是 `None`——那時每個工具都該說一句人話。
    fn project(&self) -> Option<&Project>;

    /// 套用一次修改。**必須走 `History`**，否則使用者退不回來。
    fn edit(&mut self, edit: &Edit) -> Result<(), String>;
}
