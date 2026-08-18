//! Agent 看得到什麼，以及看一次要付多少。
//!
//! # 這份測試在驗什麼
//!
//! 讀取是 Agent 做得最頻繁的事，而它每讀一次就付一次 token。以前它只有
//! 「把整份專案倒出來」一種問法，而那一份**還是不完整的**。
//!
//! 三件事：
//!
//! 1. **看得到全部**——不是只有第一個環境，而且連線也在裡面
//! 2. **問得出小的**——`select` 篩、`detail` 挑詳細度
//! 3. **篩到零筆說得出是篩掉的**，不然 Agent 會以為專案是空的，
//!    然後把已經存在的東西再建一次

mod common;

use serde_json::json;

use common::*;

// ── 看得到全部 ──────────────────────────────────────────────────

#[test]
fn describe_covers_every_environment_not_just_the_first() {
    // 這裡原本寫死 `environments.first()`。第二個環境的機器與服務實體
    // Agent 一個都看不到——而它建得出來、也改得動。
    let mut ws = a_two_environment_project();
    let out = ok(&mut ws, "describe", json!({}));

    assert!(out.contains("# 環境 prod"), "{out}");
    assert!(out.contains("# 環境 uat"), "{out}");
    // uat 的服務實體只在 uat 那一段裡，prod 沒有它。
    assert!(out.contains("## 服務實體 @ uat"), "{out}");
}

#[test]
fn describe_lists_the_connections_that_already_exist() {
    // 連線是整份專案裡數量最多的東西，也是這個工具的賣點。
    // 以前它一條都不在 describe 裡——Agent 想改一條既有連線，
    // 唯一的入口是 lint 剛好報出來的那幾條。
    let mut ws = a_two_environment_project();
    let out = ok(&mut ws, "describe", json!({}));

    assert!(out.contains("## 連線 @ prod"), "{out}");
    assert!(out.contains("apache-to-redis"), "{out}");
    // 萬用字元那一端要看得出展開成幾台、期望幾台——那正是 L004 在看的。
    assert!(out.contains("redis-*"), "{out}");
    assert!(out.contains("3 台"), "{out}");
}

#[test]
fn scoping_to_one_environment_leaves_the_other_one_out() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "uat"}}),
    );

    assert!(out.contains("# 環境 uat"), "{out}");
    assert!(
        !out.contains("# 環境 prod"),
        "指定了 uat 卻還是把 prod 倒出來：{out}"
    );
}

#[test]
fn an_unknown_environment_lists_the_real_ones() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "stg"}}),
    );

    assert!(out.contains("找不到環境 stg"), "{out}");
    assert!(out.contains("prod"), "沒告訴它有哪些：{out}");
}

// ── 問得出小的 ──────────────────────────────────────────────────

#[test]
fn brief_leaves_the_ids_out_and_full_puts_them_back() {
    // 一列 id 是 36 個字元。`target` 吃名字之後它幾乎沒人要用了，
    // 所以預設不印——這是這一層省最多的一筆。
    let mut ws = a_two_environment_project();

    let brief = ok(&mut ws, "describe", json!({}));
    let full = ok(&mut ws, "describe", json!({"detail": "full"}));

    assert!(!brief.contains("| id |"), "brief 不該印 id：{brief}");
    assert!(full.contains("| id |"), "full 應該要有 id：{full}");
    assert!(full.len() > brief.len(), "full 竟然沒有比較長");
}

#[test]
fn count_is_just_the_numbers() {
    let mut ws = a_two_environment_project();
    let out = ok(&mut ws, "describe", json!({"detail": "count"}));

    assert!(out.contains("## 服務（2）"), "{out}");
    // 只有標題與數字，一列資料都不印。
    assert!(!out.contains("反向代理"), "count 還是把資料印出來了：{out}");
}

#[test]
fn selecting_a_kind_leaves_the_rest_out() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "describe",
        json!({"select": {"kinds": ["connection"]}}),
    );

    assert!(out.contains("## 連線 @ prod"), "{out}");
    assert!(!out.contains("## 服務（"), "沒篩掉服務那張表：{out}");
}

#[test]
fn matching_a_pattern_narrows_it_to_one_group() {
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "describe",
        json!({"scope": {"environment": "prod"},
               "select": {"kinds": ["instance"], "match": "redis-*"}}),
    );

    assert!(out.contains("redis-01"), "{out}");
    assert!(!out.contains("apache-01"), "樣式沒把 apache 篩掉：{out}");
}

#[test]
fn an_unknown_detail_or_kind_is_refused_with_the_real_ones() {
    // 認不得的值不能安靜地當成預設值——那會讓 Agent 以為它篩到了。
    let mut ws = a_two_environment_project();

    let e = err(&mut ws, "describe", json!({"detail": "verbose"}));
    assert!(e.contains("brief"), "{e}");

    let e = err(&mut ws, "describe", json!({"select": {"kinds": ["連線"]}}));
    assert!(e.contains("connection"), "{e}");
}

// ── 篩到零筆要說得出來 ──────────────────────────────────────────

#[test]
fn filtering_everything_away_says_it_was_filtered() {
    // 「沒有東西」與「都被篩掉了」對 Agent 是完全不同的兩件事：
    // 前者它會開始重建，後者它會把 select 拿掉。
    let mut ws = a_two_environment_project();
    let out = ok(
        &mut ws,
        "describe",
        json!({"select": {"match": "根本沒這個-*"}}),
    );

    assert!(out.contains("**這不代表專案是空的**"), "{out}");
}

// ── lint ────────────────────────────────────────────────────────

#[test]
fn lint_count_is_a_single_line() {
    // 「建完一批之後還有問題嗎」是 Agent 每一輪都要問的。
    // 那個問題的答案是一個數字，不該是一份完整的報告。
    let mut ws = a_two_environment_project();
    create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "服務發現", "system": "shop"}),
    );

    let out = ok(&mut ws, "lint", json!({"detail": "count"}));
    assert_eq!(out.lines().count(), 1, "count 不該多行：{out}");
    assert!(out.contains("錯誤"), "{out}");
}

#[test]
fn lint_can_be_narrowed_to_one_rule() {
    let mut ws = a_two_environment_project();
    create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "服務發現", "system": "shop"}),
    );

    let out = ok(&mut ws, "lint", json!({"select": {"rules": ["L001"]}}));
    assert!(out.contains("L001"), "{out}");
    for line in out.lines().filter(|l| l.starts_with("- ")) {
        assert!(line.contains("L001"), "篩了 L001 卻混進別的：{line}");
    }
}

#[test]
fn every_finding_still_says_how_to_fix_it_even_in_brief() {
    // 省掉「怎麼修」省得下幾個字，但換來的是 Agent 亂試一輪——
    // 而它最常見的亂試是把契約刪掉，那完全是反效果。
    let mut ws = a_two_environment_project();
    create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "服務發現", "system": "shop"}),
    );

    let out = ok(&mut ws, "lint", json!({}));
    for line in out.lines().filter(|l| l.starts_with("- ")) {
        assert!(line.contains(" → "), "這一項沒說怎麼修：{line}");
    }
}

#[test]
fn narrowing_lint_to_one_environment_keeps_the_logical_layer() {
    // 邏輯層的問題對每個環境都成立。藏起來的話，Agent 會在 uat 裡
    // 一直找不到病因——因為病根本不在 uat。
    let mut ws = a_two_environment_project();
    // 契約兩端指到不存在的東西：這是邏輯層的問題，不屬於任何環境。
    create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "服務發現", "system": "shop"}),
    );

    let scoped = ok(&mut ws, "lint", json!({"scope": {"environment": "uat"}}));
    assert!(scoped.contains("uat"), "{scoped}");
    // consul 在兩個環境都沒實現，uat 那一項要留著。
    assert!(scoped.contains("consul"), "{scoped}");
}

#[test]
fn a_clean_project_that_was_filtered_does_not_claim_to_be_clean() {
    // 「沒有符合的發現」與「整份專案沒問題」差很多。
    let mut ws = a_two_environment_project();
    create(
        &mut ws,
        "container",
        json!({"slug": "consul", "name": "服務發現", "system": "shop"}),
    );

    let out = ok(&mut ws, "lint", json!({"select": {"rules": ["L014"]}}));
    assert!(out.contains("有篩選條件"), "{out}");
}
