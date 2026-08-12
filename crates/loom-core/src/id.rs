//! 元素的身分證。
//!
//! 每個元素同時具備 **UUID（身分證）** 與 **slug（顯示名）**：
//! 改名只動 slug，draw.io 圖形裡的綁定與 YAML 的互相參照都用 UUID，永不斷線。
//!
//! # 目前的狀態
//!
//! 這裡刻意還沒相依 `uuid` crate。`Id` 是一個不透明的字串包裝，
//! 之後換成真正的 UUID 時只會動到 [`Id::new`] 與內部表示，
//! 使用端（領域模型、lint）完全不受影響。

use std::fmt;

/// 元素的永久識別碼。建立後永不改變。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(String);

impl Id {
    /// 從既有字串建立（讀檔、測試素材用）。
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
    fn 可以當成雜湊表的鍵() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(Id::new("redis"), 6379);
        assert_eq!(map.get(&Id::new("redis")), Some(&6379));
    }
}
