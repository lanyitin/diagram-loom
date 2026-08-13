//! 連線表：把一個環境的實際連線攤成一列一列。
//!
//! # 為什麼在 Rust 算
//!
//! 表格裡的「來源」「目標」欄位不是直接印 id——`redis-*` 要展開成實際幾台、
//! 位址要從對應的 Endpoint 查出來。這跟 lint 做萬用字元比對是同一件事，
//! 放前端做就是把同一套邏輯寫第二遍，然後慢慢漂移。
//!
//! 前端只負責排版：太多位址時顯示第一個加「+N」之類的。
//!
//! # 狀態同樣不自己判斷
//!
//! 跟 [`crate::coverage`] 一樣，每一列的狀態由 lint 的結果決定。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;
use crate::lint::{Rule, Severity, lint};

/// 連線的一端指向什麼。畫面上用不同的圖示區分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SideKind {
    /// 一台或一群服務落地。
    Instance,
    /// F5 這類設備。
    Infra,
    /// 外部系統的落地。
    System,
}

/// 連線的一端，已經解析成看得懂的東西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Side {
    pub kind: SideKind,
    /// 顯示名。萬用字元保留原樣（`redis-*`），使用者才看得出這是一群。
    pub label: String,
    /// 接點的顯示名。來源端可以沒有（由 OS 分配 ephemeral port）。
    pub endpoint: Option<String>,
    /// 實際位址。萬用字元會有多個。
    pub addresses: Vec<String>,
    /// 萬用字元展開成幾個。不是萬用字元時是 1。
    pub matched: u32,
    /// 註明的期望數量。
    pub expect: Option<u32>,
}

/// 表格的一列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: Id,
    pub environment: Id,
    /// 它服務哪條契約。
    pub serves: Id,
    /// 契約的顯示名。找不到對應契約時是 `None`（那本身就是 L003）。
    pub serves_slug: Option<String>,
    pub purpose: String,
    pub from: Side,
    pub to: Side,
    /// 這一列自己的嚴重度。沒問題時是 `None`。
    pub severity: Option<Severity>,
    pub rules: Vec<Rule>,
}

/// 算出所有環境的所有連線列，依環境、再依契約排好。
pub fn rows(project: &Project) -> Vec<Row> {
    let findings = lint(project);
    let mut out = Vec::new();

    for env in &project.environments {
        let index = EnvIndex::build(project, env);

        // 這個環境裡，每條連線各自有哪些問題。
        let mut 每條的問題: HashMap<&Id, Vec<Rule>> = HashMap::new();
        for f in &findings {
            if f.environment.as_ref() != Some(&env.id) {
                continue;
            }
            let bucket = 每條的問題.entry(&f.subject).or_default();
            if !bucket.contains(&f.rule) {
                bucket.push(f.rule);
            }
        }

        for conn in &env.connections {
            let mut rules = 每條的問題.get(&conn.id).cloned().unwrap_or_default();
            rules.sort();

            out.push(Row {
                id: conn.id.clone(),
                environment: env.id.clone(),
                serves: conn.serves.clone(),
                serves_slug: project
                    .logical
                    .relationships
                    .iter()
                    .find(|r| r.id == conn.serves)
                    .map(|r| r.slug.clone()),
                purpose: conn.purpose.clone(),
                from: 解析(&index, env, &conn.from),
                to: 解析(&index, env, &conn.to),
                severity: rules.iter().map(|r| r.severity()).max(),
                rules,
            });
        }
    }

    out.sort_by(|a, b| {
        let 環境 = |r: &Row| {
            project
                .environments
                .iter()
                .position(|e| e.id == r.environment)
        };
        (環境(a), a.serves.to_string(), a.id.to_string()).cmp(&(
            環境(b),
            b.serves.to_string(),
            b.id.to_string(),
        ))
    });
    out
}

fn 解析(index: &EnvIndex<'_>, env: &Environment, side: &Endpointing) -> Side {
    match side {
        Endpointing::Instance { target, endpoint } => match target {
            InstanceRef::One(id) => {
                let found = index.instance(id);
                let ep = found.and_then(|i| {
                    i.endpoints
                        .iter()
                        .find(|e| endpoint.as_ref().is_some_and(|d| e.def.as_ref() == Some(d)))
                });
                Side {
                    kind: SideKind::Instance,
                    // 查不到就把 id 印出來——那是 L003，使用者需要看到是哪個 id 壞了。
                    label: found
                        .map(|i| i.slug.clone())
                        .unwrap_or_else(|| id.to_string()),
                    endpoint: ep.map(|e| e.slug.clone()),
                    addresses: ep.and_then(|e| e.address.clone()).into_iter().collect(),
                    matched: found.is_some() as u32,
                    expect: None,
                }
            }
            InstanceRef::Pattern {
                slug_pattern,
                expect,
            } => {
                let matched = index.matching(slug_pattern);
                let addresses = matched
                    .iter()
                    .filter_map(|i| {
                        i.endpoints
                            .iter()
                            .find(|e| endpoint.as_ref().is_some_and(|d| e.def.as_ref() == Some(d)))
                            .and_then(|e| e.address.clone())
                    })
                    .collect();
                let ep = matched.first().and_then(|i| {
                    i.endpoints
                        .iter()
                        .find(|e| endpoint.as_ref().is_some_and(|d| e.def.as_ref() == Some(d)))
                });
                Side {
                    kind: SideKind::Instance,
                    label: slug_pattern.clone(),
                    endpoint: ep.map(|e| e.slug.clone()),
                    addresses,
                    matched: matched.len() as u32,
                    expect: *expect,
                }
            }
        },
        Endpointing::Infra { node, endpoint } => {
            let found = env.infra_node(node);
            // 設備這端指的是**具體的 Endpoint**，不是邏輯層的 EndpointDef——
            // 設備不對應任何邏輯層元素，沒有 def 可指。這個不對稱是刻意的。
            let ep = found.and_then(|n| {
                n.endpoints
                    .iter()
                    .find(|e| endpoint.as_ref().is_some_and(|want| &e.id == want))
            });
            Side {
                kind: SideKind::Infra,
                label: found
                    .map(|n| n.slug.clone())
                    .unwrap_or_else(|| node.to_string()),
                endpoint: ep.map(|e| e.slug.clone()),
                addresses: ep.and_then(|e| e.address.clone()).into_iter().collect(),
                matched: found.is_some() as u32,
                expect: None,
            }
        }
        Endpointing::System { instance, endpoint } => {
            let found = env.system_instance(instance);
            let ep = found.and_then(|s| {
                s.endpoints
                    .iter()
                    .find(|e| endpoint.as_ref().is_some_and(|d| e.def.as_ref() == Some(d)))
            });
            Side {
                kind: SideKind::System,
                label: found
                    .map(|s| s.slug.clone())
                    .unwrap_or_else(|| instance.to_string()),
                endpoint: ep.map(|e| e.slug.clone()),
                addresses: ep.and_then(|e| e.address.clone()).into_iter().collect(),
                matched: found.is_some() as u32,
                expect: None,
            }
        }
    }
}
