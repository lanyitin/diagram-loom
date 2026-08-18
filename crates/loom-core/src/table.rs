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
use crate::environment::{ConnectionKind, Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;
use crate::lint::{Rule, Severity, lint};
use crate::logical::Logical;

/// 連線的一端指向什麼。畫面上用不同的圖示區分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SideKind {
    /// 一台或一群服務實體。
    Instance,
    /// F5 這類設備。
    Infra,
    /// 外部系統的實體。
    System,
    /// 真人。只會出現在來源端。
    Person,
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
    /// 使用者的備註。跟 `purpose` 分開，理由見
    /// [`Connection::memo`](crate::environment::Connection::memo)。
    ///
    /// # 為什麼表格一定要帶著它
    ///
    /// 備註是這個模型裡**唯一沒有規則會回報**的欄位：寫錯了 lint 不會叫。
    /// 所以它的回報路徑只有表格。讀不回來的話，寫的人（包含 Agent）
    /// 會以為沒寫成功，然後再寫一次。
    ///
    /// 資源那幾張表由 [`crate::inventory::tables`] 統一補上這一欄，
    /// 連線表是自己一份，所以要在這裡帶。
    pub memo: String,
    /// 正常路徑還是備援路徑。畫面上用來區分——備援跟正常長得一樣的話，
    /// 讀的人分不出平常的資料流是哪幾條。
    pub kind: ConnectionKind,
    pub from: Side,
    pub to: Side,
    /// 這一列自己的嚴重度。沒問題時是 `None`。
    pub severity: Option<Severity>,
    pub rules: Vec<Rule>,
    /// 這一列牽涉到的所有模型元素 id：連線自己、它服務的契約、
    /// 兩端解析出來的 Instance／設備／外部系統／人，以及用到的 Endpoint。
    ///
    /// # 為什麼需要它
    ///
    /// Lint 面板上點一項發現，要能跳到**那一列**。但發現的 `subject`
    /// 不一定是連線 id——L006 指的是 Endpoint、L008 指的是 Instance。
    /// 「這個 id 對應到哪幾列」得走一次萬用字元展開才知道，
    /// 那跟 lint 是同一套邏輯，放前端就是寫第二遍。
    pub subjects: Vec<Id>,
}

/// 算出所有環境的所有連線列，依環境、再依契約排好。
pub fn rows(project: &Project) -> Vec<Row> {
    let findings = lint(project);
    let mut out = Vec::new();

    for env in &project.environments {
        let index = EnvIndex::build(project, env);

        // 這個環境裡，每條連線各自有哪些問題。
        let mut issues_by_connection: HashMap<&Id, Vec<Rule>> = HashMap::new();
        for f in &findings {
            if f.environment.as_ref() != Some(&env.id) {
                continue;
            }
            let bucket = issues_by_connection.entry(&f.subject).or_default();
            if !bucket.contains(&f.rule) {
                bucket.push(f.rule);
            }
        }

        for conn in &env.connections {
            let mut rules = issues_by_connection
                .get(&conn.id)
                .cloned()
                .unwrap_or_default();
            rules.sort();

            let mut subjects = vec![conn.id.clone(), conn.serves.clone()];
            for side in [&conn.from, &conn.to] {
                collect_elements(&index, env, side, &mut subjects);
            }
            subjects.sort();
            subjects.dedup();

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
                memo: conn.memo.clone(),
                kind: conn.kind,
                from: resolve_side(&index, env, &project.logical, &conn.from),
                to: resolve_side(&index, env, &project.logical, &conn.to),
                severity: rules.iter().map(|r| r.severity()).max(),
                rules,
                subjects,
            });
        }
    }

    out.sort_by(|a, b| {
        let env_of = |r: &Row| {
            project
                .environments
                .iter()
                .position(|e| e.id == r.environment)
        };
        (env_of(a), a.serves.to_string(), a.id.to_string()).cmp(&(
            env_of(b),
            b.serves.to_string(),
            b.id.to_string(),
        ))
    });
    out
}

/// 這一端牽涉到哪些模型元素。給 [`Row::subjects`] 用。
///
/// 找不到的 id **也要收**——那正是 L003 的情況，使用者點那項發現時
/// 需要跳到這一列看是哪個 id 壞了。
fn collect_elements(
    index: &EnvIndex<'_>,
    env: &Environment,
    side: &Endpointing,
    out: &mut Vec<Id>,
) {
    /// 這個 Instance／服務實體上，對應某個 `EndpointDef` 的那個具體 Endpoint。
    fn endpoint_id(endpoints: &[crate::environment::Endpoint], def: Option<&Id>) -> Option<Id> {
        endpoints
            .iter()
            .find(|e| def.is_some_and(|d| e.def.as_ref() == Some(d)))
            .map(|e| e.id.clone())
    }

    match side {
        Endpointing::Instance { target, endpoint } => match target {
            InstanceRef::One(id) => {
                out.push(id.clone());
                if let Some(i) = index.instance(id) {
                    out.extend(endpoint_id(&i.endpoints, endpoint.as_ref()));
                }
            }
            InstanceRef::Pattern {
                slug_pattern,
                within,
                ..
            } => {
                for i in index.matching_within(slug_pattern, within.as_ref()) {
                    out.push(i.id.clone());
                    out.extend(endpoint_id(&i.endpoints, endpoint.as_ref()));
                }
                out.extend(within.clone());
            }
        },
        Endpointing::Infra { node, endpoint } => {
            out.push(node.clone());
            // 設備這端指的是具體 Endpoint，不是 EndpointDef，所以直接收。
            out.extend(endpoint.clone());
        }
        Endpointing::System { instance, endpoint } => {
            out.push(instance.clone());
            if let Some(s) = env.system_instance(instance) {
                out.extend(endpoint_id(&s.endpoints, endpoint.as_ref()));
            }
        }
        Endpointing::Person { person } => out.push(person.clone()),
    }
}

fn resolve_side(
    index: &EnvIndex<'_>,
    env: &Environment,
    logical: &Logical,
    side: &Endpointing,
) -> Side {
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
                within,
                expect,
            } => {
                let matched = index.matching_within(slug_pattern, within.as_ref());
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
                    // 限定了範圍就寫出來，否則「3 台」看起來像全部只有 3 台。
                    label: match within {
                        Some(node) => format!("{slug_pattern} @ {}", node_name(env, node)),
                        None => slug_pattern.clone(),
                    },
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
        // 人沒有實體、沒有位址、也沒有數量——只有一個名字。
        Endpointing::Person { person } => Side {
            kind: SideKind::Person,
            label: logical
                .people
                .iter()
                .find(|p| &p.id == person)
                .map(|p| p.slug.clone())
                // 查不到就印 id：那是 L003，使用者需要知道是哪個 id 壞了。
                .unwrap_or_else(|| person.to_string()),
            endpoint: None,
            addresses: vec![],
            matched: 1,
            expect: None,
        },
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

/// 部署節點的顯示名。找不到就印 id——那是 L003，使用者要知道是哪個壞了。
fn node_name(env: &Environment, id: &Id) -> String {
    fn search<'a>(nodes: &'a [crate::environment::DeploymentNode], id: &Id) -> Option<&'a str> {
        for n in nodes {
            if &n.id == id {
                return Some(&n.slug);
            }
            if let Some(found) = search(&n.children, id) {
                return Some(found);
            }
        }
        None
    }
    search(&env.nodes, id)
        .map(str::to_string)
        .unwrap_or_else(|| id.to_string())
}
