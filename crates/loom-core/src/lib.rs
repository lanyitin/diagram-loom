//! diagram-loom 的領域模型、驗證與 lint。
//!
//! # 紀律
//!
//! 這個 crate **不得相依 `tauri`**。所有領域邏輯都必須能用 `cargo test`
//! 驗證，不需要開視窗、不需要畫面。Tauri 那層只負責把請求轉進來。

pub mod slug;
