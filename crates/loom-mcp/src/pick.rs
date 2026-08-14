//! 「這一趟要動哪一個專案」。
//!
//! # 為什麼這件事值得一個獨立的模組
//!
//! 使用者可以同時開好幾個視窗、各看一個專案，而 Agent 看得到全部。
//! 所以每一次呼叫都得先回答「動哪一個」。
//!
//! 答錯的後果是**安靜地改到別的專案**——沒有錯誤、沒有紅字，
//! 使用者要等到打開另一扇視窗才會發現。對一個賣點是「怕漏」的工具，
//! 那是最糟的失敗方式，比整個呼叫失敗糟得多。
//!
//! 所以規則只有一條：**不確定就不做，把選項列出來讓 Agent 自己講清楚。**
//!
//! # 為什麼是純函式
//!
//! 這裡完全不知道 Tauri、視窗或鎖存在，只認得一個 `&[Open]`。
//! 於是「多個時會不會亂猜」這件事可以用 `cargo test` 直接驗——
//! 而那正是最需要被守住、卻最難用手動測試涵蓋的地方。

use std::fmt::Write as _;

/// 一個開著的專案，Agent 看得到的部分。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub slug: String,
    pub name: String,
    /// 資料夾路徑。撞名時用它區分。
    pub root: String,
    pub dirty: bool,
    pub errors: usize,
    pub warnings: usize,
}

/// 沒開專案時說的那句話。開頭那句是給人看的——Agent 會把它轉述給使用者。
pub const NOTHING_OPEN: &str =
    "使用者還沒有開啟任何專案。請他在 diagram-loom 裡開一個，或用「新專案…」建一個。";

/// 挑出要動哪一個。
///
/// | 情況 | 結果 |
/// | --- | --- |
/// | 一個都沒開 | 錯誤，請使用者先開一個 |
/// | 只開一個、沒指定 | 就是它（單專案的用法完全不變） |
/// | 開了多個、沒指定 | **錯誤並列出全部。絕不挑一個。** |
/// | 有指定 | 先比 slug，再比路徑；找不到或撞名都列候選 |
pub fn resolve(open: &[Open], want: Option<&str>) -> Result<usize, String> {
    if open.is_empty() {
        return Err(NOTHING_OPEN.into());
    }

    let Some(want) = want.map(str::trim).filter(|w| !w.is_empty()) else {
        if open.len() == 1 {
            return Ok(0);
        }
        return Err(format!(
            "使用者同時開著 {} 個專案，你沒說要動哪一個。\
             請在參數裡加上 `project`。\n\n{}",
            open.len(),
            listing(open),
        ));
    };

    // slug 完全相同的優先。名字就是使用者叫它的方式，最不容易誤會。
    let by_slug: Vec<usize> = matching(open, |p| p.slug.eq_ignore_ascii_case(want));
    if by_slug.len() == 1 {
        return Ok(by_slug[0]);
    }
    if by_slug.len() > 1 {
        return Err(ambiguous(open, &by_slug, want));
    }

    // 再來才是路徑。兩個專案可以同名（不同資料夾），這時只有路徑分得開。
    let by_path: Vec<usize> = matching(open, |p| p.root.contains(want));
    match by_path.len() {
        1 => Ok(by_path[0]),
        0 => Err(format!(
            "沒有開著叫做「{want}」的專案。\n\n{}",
            listing(open)
        )),
        _ => Err(ambiguous(open, &by_path, want)),
    }
}

fn matching(open: &[Open], f: impl Fn(&Open) -> bool) -> Vec<usize> {
    open.iter()
        .enumerate()
        .filter(|(_, p)| f(p))
        .map(|(i, _)| i)
        .collect()
}

fn ambiguous(open: &[Open], hits: &[usize], want: &str) -> String {
    let mut out = format!("「{want}」對得上好幾個專案，請用完整路徑指定：\n");
    for &i in hits {
        let _ = writeln!(out, "  {}", open[i].root);
    }
    let _ = write!(out, "\n{}", listing(open));
    out
}

/// 開著的專案一覽。錯誤訊息與 `projects` 工具共用同一份，
/// 這樣 Agent 讀到的格式永遠一致。
pub fn listing(open: &[Open]) -> String {
    if open.is_empty() {
        return NOTHING_OPEN.into();
    }
    let mut out = String::from("目前開著的專案：\n");
    for p in open {
        let _ = write!(out, "- {} （{}）  {}", p.slug, p.name, p.root);
        if p.dirty {
            out.push_str("  ⚠️未儲存");
        }
        match (p.errors, p.warnings) {
            (0, 0) => out.push_str("  lint 乾淨"),
            (e, w) => {
                let _ = write!(out, "  lint {e} 錯誤 / {w} 警告");
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(slug: &str, root: &str) -> Open {
        Open {
            slug: slug.into(),
            name: slug.into(),
            root: root.into(),
            dirty: false,
            errors: 0,
            warnings: 0,
        }
    }

    #[test]
    fn nothing_open_says_so() {
        assert_eq!(resolve(&[], None).unwrap_err(), NOTHING_OPEN);
        assert_eq!(resolve(&[], Some("payments")).unwrap_err(), NOTHING_OPEN);
    }

    #[test]
    fn a_single_project_needs_no_naming() {
        // 只開一個的時候用法要跟以前完全一樣，不然每個 Agent 都得改。
        let one = [open("payments", "/w/payments.loom")];
        assert_eq!(resolve(&one, None).unwrap(), 0);
    }

    #[test]
    fn several_open_and_unnamed_never_guesses() {
        // 這是這個模組存在的理由。猜錯＝安靜地改到別的專案。
        let two = [
            open("payments", "/w/payments.loom"),
            open("billing", "/w/billing.loom"),
        ];
        let err = resolve(&two, None).unwrap_err();
        assert!(err.contains("你沒說要動哪一個"), "{err}");
        assert!(err.contains("payments") && err.contains("billing"), "{err}");
    }

    #[test]
    fn a_name_picks_one() {
        let two = [
            open("payments", "/w/payments.loom"),
            open("billing", "/w/billing.loom"),
        ];
        assert_eq!(resolve(&two, Some("billing")).unwrap(), 1);
        assert_eq!(resolve(&two, Some("PAYMENTS")).unwrap(), 0);
    }

    #[test]
    fn a_path_picks_one_when_the_names_collide() {
        // 兩個資料夾可以放同名專案。這時只有路徑分得開。
        let two = [
            open("payments", "/w/prod/payments.loom"),
            open("payments", "/w/lab/payments.loom"),
        ];
        assert_eq!(resolve(&two, Some("/w/lab/")).unwrap(), 1);

        let err = resolve(&two, Some("payments")).unwrap_err();
        assert!(err.contains("好幾個"), "{err}");
        assert!(err.contains("/w/prod/payments.loom"), "{err}");
    }

    #[test]
    fn an_unknown_name_lists_what_is_open() {
        // 只說「找不到」的話 Agent 只能猜。列出來它才修得掉。
        let two = [
            open("payments", "/w/payments.loom"),
            open("billing", "/w/billing.loom"),
        ];
        let err = resolve(&two, Some("crm")).unwrap_err();
        assert!(err.contains("沒有開著叫做「crm」的專案"), "{err}");
        assert!(err.contains("billing"), "{err}");
    }

    #[test]
    fn an_empty_name_counts_as_not_given() {
        // Agent 送 `"project": ""` 是常見的手滑。當成沒給，走同一套規則。
        let two = [
            open("payments", "/w/payments.loom"),
            open("billing", "/w/billing.loom"),
        ];
        assert!(resolve(&two, Some("  ")).unwrap_err().contains("你沒說"));
        assert_eq!(resolve(&two[..1], Some("")).unwrap(), 0);
    }

    #[test]
    fn the_listing_shows_what_needs_attention() {
        // Agent 一趟就要知道該先修哪一個——這個專案剛學到的教訓是貴的是趟數。
        let two = [
            Open {
                dirty: true,
                errors: 3,
                warnings: 1,
                ..open("payments", "/w/payments.loom")
            },
            open("billing", "/w/billing.loom"),
        ];
        let text = listing(&two);
        assert!(text.contains("⚠️未儲存"), "{text}");
        assert!(text.contains("lint 3 錯誤 / 1 警告"), "{text}");
        assert!(text.contains("lint 乾淨"), "{text}");
    }
}
