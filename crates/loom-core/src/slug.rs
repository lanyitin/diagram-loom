//! 元素的顯示名（slug）。
//!
//! 每個元素同時有 UUID 與 slug：UUID 是身分證，永不改變，draw.io 的綁定與
//! YAML 的互相參照都用它；slug 只是給人看的名字，改名不會弄斷任何東西。
//!
//! slug 仍需正規化，因為它會出現在檔名、萬用字元樣式與匯入比對中。

/// 把任意字串正規化成 slug。
///
/// - 英文字母轉小寫
/// - 非文數字（空白、標點）視為分隔符，收合成單一 `-`
/// - 去掉頭尾的 `-`
/// - 保留中日韓文字：`char::is_alphanumeric` 是 Unicode 感知的
///
/// ```
/// use loom_core::slug::slugify;
/// assert_eq!(slugify("Redis Cluster"), "redis-cluster");
/// ```
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_separator = false;

    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_separator = true;
        }
    }

    out
}

/// slug 是否已是正規形式。匯入與 UI 用它判斷要不要提示使用者。
pub fn is_normalized(candidate: &str) -> bool {
    !candidate.is_empty() && slugify(candidate) == candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_become_hyphens_and_case_is_lowered() {
        assert_eq!(slugify("Redis Cluster"), "redis-cluster");
    }

    #[test]
    fn runs_of_separators_collapse_to_one() {
        assert_eq!(slugify("order   api"), "order-api");
        assert_eq!(slugify("f5 -- vip"), "f5-vip");
    }

    #[test]
    fn trims_leading_and_trailing_separators() {
        assert_eq!(slugify("  app-vm-01  "), "app-vm-01");
        assert_eq!(slugify("///redis///"), "redis");
    }

    #[test]
    fn punctuation_becomes_a_separator() {
        assert_eq!(slugify("jdbc:oracle:thin"), "jdbc-oracle-thin");
        assert_eq!(slugify("10.0.1.11:6379"), "10-0-1-11-6379");
    }

    #[test]
    fn keeps_cjk_characters() {
        assert_eq!(slugify("訂單服務"), "訂單服務");
        assert_eq!(slugify("訂單 服務"), "訂單-服務");
    }

    #[test]
    fn all_separators_gives_empty_string() {
        assert_eq!(slugify("---"), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_is_idempotent() {
        for input in ["Redis Cluster", "  f5 -- vip ", "訂單 服務", "10.0.1.11"] {
            let once = slugify(input);
            assert_eq!(slugify(&once), once, "slugify 對 {input:?} 不是冪等的");
        }
    }

    #[test]
    fn is_normalised() {
        assert!(is_normalized("redis-cluster"));
        assert!(!is_normalized("Redis Cluster"));
        assert!(!is_normalized(""));
    }
}
