//! 萬用字元比對。
//!
//! 只支援 `*`（比對零到多個字元），刻意不做完整的 glob 或正規表示式——
//! 使用者要寫的是 `redis-*` 這種東西，多餘的威力只會製造誤解。

/// `pattern` 是否符合 `text`。`*` 可比對零到多個字元。
///
/// ```
/// use loom_core::pattern::matches;
/// assert!(matches("redis-*", "redis-01"));
/// assert!(!matches("redis-*", "consul-01"));
/// ```
pub fn matches(pattern: &str, text: &str) -> bool {
    let segments: Vec<&str> = pattern.split('*').collect();

    // 沒有 `*`：必須完全相同。
    if segments.len() == 1 {
        return pattern == text;
    }

    let mut rest = text;

    // 第一段必須是開頭（`*foo` 時第一段為空字串，自然通過）。
    let first = segments[0];
    match rest.strip_prefix(first) {
        Some(remaining) => rest = remaining,
        None => return false,
    }

    // 最後一段必須是結尾。
    let last = segments[segments.len() - 1];
    if !last.is_empty() {
        match rest.strip_suffix(last) {
            // 借用結束後才縮短，避免中間段與結尾段搶同一批字元。
            Some(remaining) => rest = remaining,
            None => return false,
        }
    }

    // 中間各段依序出現即可，貪婪往前推進。
    for segment in &segments[1..segments.len() - 1] {
        if segment.is_empty() {
            continue;
        }
        match rest.find(segment) {
            Some(at) => rest = &rest[at + segment.len()..],
            None => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_wildcard() {
        assert!(matches("redis-*", "redis-01"));
        assert!(matches("redis-*", "redis-"));
        assert!(!matches("redis-*", "redis"));
        assert!(!matches("redis-*", "consul-01"));
    }

    #[test]
    fn without_a_wildcard_it_must_match_exactly() {
        assert!(matches("redis-01", "redis-01"));
        assert!(!matches("redis-01", "redis-02"));
        assert!(!matches("redis", "redis-01"));
    }

    #[test]
    fn leading_wildcard() {
        assert!(matches("*-01", "redis-01"));
        assert!(!matches("*-01", "redis-02"));
    }

    #[test]
    fn wildcard_in_the_middle() {
        assert!(matches("redis-*-01", "redis-prod-01"));
        assert!(matches("redis-*-01", "redis--01"));
        assert!(!matches("redis-*-01", "redis-prod-02"));
    }

    #[test]
    fn lone_star_matches_anything() {
        assert!(matches("*", "redis-01"));
        assert!(matches("*", ""));
    }

    #[test]
    fn prefix_and_suffix_do_not_share_characters() {
        // "ab" 不該同時當開頭又當結尾去滿足 "ab*ab"
        assert!(!matches("ab*ab", "ab"));
        assert!(matches("ab*ab", "abab"));
    }

    #[test]
    fn empty_pattern_matches_only_empty() {
        assert!(matches("", ""));
        assert!(!matches("", "redis"));
    }
}
