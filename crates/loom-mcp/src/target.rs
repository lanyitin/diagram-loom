//! 指名一個**既有**的東西：`update` 與 `delete` 收的 `target`。
//!
//! # 為什麼不要求 Agent 給 UUID
//!
//! 理由跟 [`crate::refs`] 一樣（它手上的原始資料裡只有名字，而它會抄錯
//! UUID），再加上一條新的：`describe` 預設已經**不給 id 了**。
//!
//! 一列 id 是 36 個字元，乘上幾百列就是每次讀取都要付的稅。而 id 唯一的
//! 用途就是拿來當 `target`——把 `target` 改成吃名字之後，那筆稅整個消失。
//!
//! id 照樣收（`create` 回傳的、`detail: "full"` 印出來的都能直接用），
//! 但工具描述只教名字。
//!
//! # 語法
//!
//! | 寫法 | 指的是 |
//! | --- | --- |
//! | `redis-01` | 那個元素（服務實體、機器、服務、契約…都一樣） |
//! | `apache-* -> f5-01` | 那條連線。**Agent 就是這樣建它的** |
//! | `conn:apache-to-redis` | 服務那條契約的連線（這個環境只有一條時） |
//! | 一串 UUID | 那個 id |
//!
//! 連線用兩端指名，是因為它沒有 slug——而「來源 → 目標」正是 Agent 在
//! `add_connection` 裡寫過的那兩個欄位。要它換一套講法去指同一條線，
//! 等於逼它先查一次 id。
//!
//! # 撞名一律報錯，不挑一個
//!
//! `redis-01` 在 prod 與 uat 各有一台是**常態**，不是例外。挑錯的後果是
//! 安靜地改到另一個環境——沒有錯誤、沒有紅字。所以這裡列出全部候選並要求
//! `scope.environment`，跟 [`crate::pick`] 對多專案的態度是同一條。

use loom_core::id::Id;
use loom_core::resource::{self, Resource};
use loom_core::table;
use loom_core::{Project, inventory, pattern};

use crate::query::{Scope, kind_key};

/// `target` 指到的東西。
#[derive(Debug, Clone)]
pub enum Target {
    Resource(Box<Resource>),
    /// 連線不是 [`Resource`]，所以它走另一條路。帶著整列是因為錯誤訊息與
    /// 回報都要講得出「哪一條」，而那需要兩端已經解析好的名字。
    Connection(Box<table::Row>),
}

impl Target {
    /// 給回報與預覽用的一句話。
    pub fn what(&self, project: &Project) -> String {
        match self {
            Target::Resource(r) => format!("{} {}", r.kind_name(), r.slug()),
            Target::Connection(row) => {
                let env = project
                    .environment(&row.environment)
                    .map(|e| e.slug.as_str())
                    .unwrap_or("？");
                format!("{env} 的連線 {} → {}", row.from.label, row.to.label)
            }
        }
    }

    pub fn id(&self) -> &Id {
        match self {
            Target::Resource(r) => r.id(),
            Target::Connection(row) => &row.id,
        }
    }
}

/// `scope.environment` 選中了哪幾個環境。空的代表「全部」。
///
/// 一次算好交給 [`resolve`]，因為批次裡每一項都要用。
pub fn environments(project: &Project, scope: &Scope) -> Result<Vec<Id>, String> {
    match &scope.environment {
        None => Ok(vec![]),
        Some(slug) => project
            .environments
            .iter()
            .find(|e| &e.slug == slug)
            .map(|e| vec![e.id.clone()])
            .ok_or_else(|| {
                format!(
                    "找不到環境 {slug}。有的是：{}",
                    list(project.environments.iter().map(|e| e.slug.clone()))
                )
            }),
    }
}

/// 把一段文字解析成一個既有的東西。
///
/// `envs` 空的代表不限環境。
pub fn resolve(project: &Project, envs: &[Id], text: &str) -> Result<Target, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("target 是空的".into());
    }

    // 1. 先當 id 試。查得到就結束，查不到不報錯——多半只是個名字。
    let as_id = Id::new(text);
    if let Some(r) = resource::find(project, &as_id) {
        return Ok(Target::Resource(Box::new(r)));
    }
    if let Some(row) = table::rows(project).into_iter().find(|r| r.id == as_id) {
        return Ok(Target::Connection(Box::new(row)));
    }

    // 2. `來源 -> 目標`：一條連線。
    if let Some((from, to)) = text.split_once("->") {
        return connection_by_ends(project, envs, from.trim(), to.trim());
    }

    // 3. `conn:契約名`：服務那條契約的連線。
    if let Some(rel) = text.strip_prefix("conn:") {
        return connection_by_relationship(project, envs, rel.trim());
    }

    // 4. 一個名字。
    resource_by_slug(project, envs, text)
}

// ── 用名字找元素 ────────────────────────────────────────────────

/// 這個名字在哪裡出現過。
struct Hit {
    /// 在哪個環境。邏輯層是 `None`。
    env: Option<String>,
    kind: &'static str,
    resource: Resource,
}

fn resource_by_slug(project: &Project, envs: &[Id], slug: &str) -> Result<Target, String> {
    let mut hits: Vec<Hit> = Vec::new();
    let mut all: Vec<String> = Vec::new();

    // 邏輯層與專案層的表。它們不屬於任何環境，所以永遠都看。
    for table in inventory::tables(project, None) {
        for row in &table.rows {
            all.push(row.resource.slug().to_string());
            if row.resource.slug() == slug {
                hits.push(Hit {
                    env: None,
                    kind: kind_key(table.kind),
                    resource: row.resource.clone(),
                });
            }
        }
    }

    for env in &project.environments {
        if !envs.is_empty() && !envs.contains(&env.id) {
            continue;
        }
        for table in inventory::tables(project, Some(&env.id)) {
            if table.environment.is_none() {
                continue; // 上面看過了。
            }
            for row in &table.rows {
                all.push(row.resource.slug().to_string());
                if row.resource.slug() == slug {
                    hits.push(Hit {
                        env: Some(env.slug.clone()),
                        kind: kind_key(table.kind),
                        resource: row.resource.clone(),
                    });
                }
            }
        }
    }

    match hits.len() {
        1 => Ok(Target::Resource(Box::new(hits.pop().unwrap().resource))),
        // 候選會被截斷（三百個名字是三百個名字的 token），所以還是要留一條
        // 「自己去查」的路——不然清單一被截，Agent 就只剩下重猜一次。
        0 => Err(format!(
            "找不到 {slug}{}。有的是：{}\n查不到就用 describe 看目前有什麼。",
            in_which(project, envs),
            list(all.into_iter())
        )),
        _ => Err(ambiguous(slug, &hits)),
    }
}

/// 撞名時列出每一個候選是哪一種、在哪個環境。
///
/// 只說「有好幾個」的話 Agent 只能重試同一個字。它需要知道**下一步寫什麼**。
fn ambiguous(slug: &str, hits: &[Hit]) -> String {
    let mut lines: Vec<String> = hits
        .iter()
        .map(|h| match &h.env {
            Some(env) => format!("  {} {slug}（環境 {env}）", h.kind),
            None => format!("  {} {slug}（邏輯層）", h.kind),
        })
        .collect();
    lines.sort();

    let same_kind = hits.iter().all(|h| h.kind == hits[0].kind);
    let hint = if same_kind && hits.iter().all(|h| h.env.is_some()) {
        "加上 `scope`: {\"environment\": \"…\"} 指定是哪一個環境。"
    } else {
        "用 describe（detail: \"full\"）拿到 id，再拿 id 當 target。"
    };
    format!(
        "{slug} 有 {} 個，不會替你挑：\n{}\n{hint}",
        hits.len(),
        lines.join("\n")
    )
}

fn in_which(project: &Project, envs: &[Id]) -> String {
    if envs.is_empty() {
        return String::new();
    }
    let names: Vec<String> = project
        .environments
        .iter()
        .filter(|e| envs.contains(&e.id))
        .map(|e| e.slug.clone())
        .collect();
    format!("（只找了環境 {}）", names.join("、"))
}

// ── 用兩端找連線 ────────────────────────────────────────────────

fn connection_by_ends(
    project: &Project,
    envs: &[Id],
    from: &str,
    to: &str,
) -> Result<Target, String> {
    let rows = rows_in(project, envs);
    let mut hits: Vec<table::Row> = rows
        .iter()
        .filter(|r| side_matches(&r.from.label, from) && side_matches(&r.to.label, to))
        .cloned()
        .collect();

    match hits.len() {
        1 => Ok(Target::Connection(Box::new(hits.pop().unwrap()))),
        0 => Err(format!(
            "找不到 {from} → {to} 這條連線{}。這裡有的是：\n{}",
            in_which(project, envs),
            listing(project, &rows)
        )),
        _ => Err(format!(
            "{from} → {to} 對到 {} 條連線，不會替你挑：\n{}\n\
             加上 `scope`: {{\"environment\": \"…\"}}，或把兩端寫完整。",
            hits.len(),
            listing(project, &hits)
        )),
    }
}

fn connection_by_relationship(project: &Project, envs: &[Id], rel: &str) -> Result<Target, String> {
    let rows = rows_in(project, envs);
    let mut hits: Vec<table::Row> = rows
        .iter()
        .filter(|r| r.serves_slug.as_deref() == Some(rel))
        .cloned()
        .collect();

    match hits.len() {
        1 => Ok(Target::Connection(Box::new(hits.pop().unwrap()))),
        0 => Err(format!(
            "契約 {rel} 在這裡沒有任何連線{}。有連線的契約是：{}",
            in_which(project, envs),
            list(rows.iter().filter_map(|r| r.serves_slug.clone()))
        )),
        // 經過 F5 的流量會拆成兩段，兩段都服務同一條契約——所以這不是例外，
        // 是常態。要它用 `來源 -> 目標` 講清楚是哪一段。
        _ => Err(format!(
            "契約 {rel} 有 {} 條連線（經過設備的流量本來就會拆成好幾段），\
             請改用 `來源 -> 目標` 指名：\n{}",
            hits.len(),
            listing(project, &hits)
        )),
    }
}

fn rows_in(project: &Project, envs: &[Id]) -> Vec<table::Row> {
    table::rows(project)
        .into_iter()
        .filter(|r| envs.is_empty() || envs.contains(&r.environment))
        .collect()
}

/// 兩端的名字比對。`*` 可比對零到多個字元，跟連線兩端用的是同一套語法。
///
/// 樣式連線的 label 會帶上限定的站點（`redis-* @ dc-main`），所以 `@` 前面
/// 那一段也要比對得到——不然 Agent 得把它在 `add_connection` 裡寫過的東西
/// 再抄一次，而它多半只記得前半段。
fn side_matches(label: &str, given: &str) -> bool {
    if pattern::matches(given, label) {
        return true;
    }
    match label.split_once('@') {
        Some((head, _)) => pattern::matches(given, head.trim()),
        None => false,
    }
}

fn listing(project: &Project, rows: &[table::Row]) -> String {
    if rows.is_empty() {
        return "  （一條都沒有）".into();
    }
    let mut lines: Vec<String> = rows
        .iter()
        .take(30)
        .map(|r| {
            let env = project
                .environment(&r.environment)
                .map(|e| e.slug.as_str())
                .unwrap_or("？");
            format!("  [{env}] {} -> {}", r.from.label, r.to.label)
        })
        .collect();
    lines.sort();
    lines.dedup();
    if rows.len() > 30 {
        lines.push(format!(
            "  （還有 {} 條，加 scope 或寫完整一點）",
            rows.len() - 30
        ));
    }
    lines.join("\n")
}

/// 候選清單。**會截斷**——三百個名字是三百個名字的 token，
/// 而 Agent 看前面幾十個就足以發現自己拼錯了。
fn list(items: impl Iterator<Item = String>) -> String {
    let mut v: Vec<String> = items.collect();
    v.sort();
    v.dedup();
    if v.is_empty() {
        return "（一個都沒有）".into();
    }
    if v.len() > 30 {
        let rest = v.len() - 30;
        v.truncate(30);
        return format!("{}…（還有 {rest} 個）", v.join("、"));
    }
    v.join("、")
}
