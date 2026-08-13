//! 元素的身分證。
//!
//! 每個元素同時具備 **UUID（身分證）** 與 **slug（顯示名）**：
//! 改名只動 slug，draw.io 圖形裡的綁定與 YAML 的互相參照都用 UUID，永不斷線。
//!
//! # 為什麼包成不透明型別
//!
//! `Id` 內部就是字串，但不直接用 `String`：這樣新建元素一律走
//! [`Id::generate`]（真正的 UUID v4），而讀檔與測試素材走 [`Id::new`]。
//! 兩條路分開，就不會有人隨手拿一個名字當識別碼用。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 元素的永久識別碼。建立後永不改變。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// 產生一個新的識別碼（UUID v4）。新建元素時使用。
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 從既有字串建立。讀檔與測試素材用。
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Id {
    fn from(raw: &str) -> Self {
        Self::new(raw)
    }
}

impl From<String> for Id {
    fn from(raw: String) -> Self {
        Self::new(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 相同字串視為同一個_id() {
        assert_eq!(Id::new("redis"), Id::from("redis"));
    }

    #[test]
    fn 不同字串是不同的_id() {
        assert_ne!(Id::new("redis"), Id::new("consul"));
    }

    #[test]
    fn 每次產生的識別碼都不同() {
        let a = Id::generate();
        let b = Id::generate();
        assert_ne!(a, b);
        // UUID v4 的標準字串長度
        assert_eq!(a.as_str().len(), 36);
    }

    #[test]
    fn 序列化成純字串不加包裝() {
        let yaml = yaml_serde::to_string(&Id::new("c-redis")).unwrap();
        assert_eq!(yaml.trim(), "c-redis");

        let back: Id = yaml_serde::from_str("c-redis").unwrap();
        assert_eq!(back, Id::new("c-redis"));
    }

    #[test]
    fn 可以當成雜湊表的鍵() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(Id::new("redis"), 6379);
        assert_eq!(map.get(&Id::new("redis")), Some(&6379));
    }
}
