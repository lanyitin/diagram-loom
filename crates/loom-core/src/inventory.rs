//! 資源清單：每種資源一張表。
//!
//! # 為什麼欄位是 Rust 決定的
//!
//! 「服務那張表要顯示幾個接點、在各環境幾台」不是排版選擇，是**模型知識**——
//! 那些數字要走一次落地比對才算得出來。放前端就是把 `coverage` 的邏輯
//! 再寫一遍，然後慢慢漂移。
//!
//! 所以這裡吐的是**已經算好的字串格子**，前端只負責畫成表格。
//! 代價是欄位不能在前端排序或篩選——那等真的需要再說。
//!
//! # 每一列都帶著它的 `Resource`
//!
//! 使用者按「編輯」時，表單要有一份現成的值當起點。讓前端從
//! `project` 裡自己撈會需要知道每種資源住在哪一層（接點定義掛在服務或
//! 系統身上、機器可能巢狀好幾層），那又是模型知識。

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::DeploymentNode;
use crate::id::Id;
use crate::lint::{Severity, lint};
use crate::resource::{Kind, Resource};

/// 表格的一列。
///
/// 名字不叫 `Row`，是為了不跟 [`crate::table::Row`]（連線表的一列）撞名——
/// 型別匯出到 TypeScript 之後是同一個命名空間，撞了就產不出來。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ResourceRow {
    pub id: Id,
    /// 已經算好的格子，順序對應 [`Table::columns`]。
    pub cells: Vec<String>,
    /// 縮排層級。只有機器會用到（站點 → 機器 → 機器）。
    pub depth: u32,
    /// 這一列身上有沒有 lint 問題。有的話畫面上點一下就能跳過去。
    pub severity: Option<Severity>,
    /// 編輯表單的起點。
    pub resource: Resource,
}

/// 一種資源的一張表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub kind: Kind,
    /// 分頁上的字，例如「服務」。
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<ResourceRow>,
    /// 這張表屬於哪個環境。邏輯層的表是 `None`。
    pub environment: Option<Id>,
    /// 沒有半列時要說什麼。空白的表格看起來像壞掉。
    pub empty_hint: String,
}

/// 邏輯層的所有表，加上指定環境的所有表。
///
/// `environment` 為 `None` 時只給邏輯層——剛開一個空專案就是這樣。
pub fn tables(project: &Project, environment: Option<&Id>) -> Vec<Table> {
    let findings = lint(project);
    let 嚴重度 = |id: &Id| {
        findings
            .iter()
            .filter(|f| &f.subject == id)
            .map(|f| f.severity())
            .max()
    };

    let mut out = vec![
        系統表(project, &嚴重度),
        服務表(project, &嚴重度),
        接點定義表(project, &嚴重度),
        契約表(project, &嚴重度),
        人表(project, &嚴重度),
        環境表(project, &嚴重度),
    ];

    if let Some(env_id) = environment
        && let Some(env) = project.environment(env_id)
    {
        out.push(機器表(env, &嚴重度));
        out.push(設備表(env, &嚴重度));
        out.push(外部系統落地表(project, env, &嚴重度));
    }
    out
}

type 嚴重度查詢<'a> = &'a dyn Fn(&Id) -> Option<Severity>;

fn 系統表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    Table {
        kind: Kind::System,
        title: "系統".into(),
        columns: vec![
            "名稱".into(),
            "顯示名".into(),
            "自家／外部".into(),
            "服務數".into(),
        ],
        environment: None,
        empty_hint: "先建一個系統——服務要掛在系統底下。".into(),
        rows: project
            .logical
            .systems
            .iter()
            .map(|s| ResourceRow {
                id: s.id.clone(),
                depth: 0,
                severity: sev(&s.id),
                cells: vec![
                    s.slug.clone(),
                    s.name.clone(),
                    if s.external { "外部" } else { "自家" }.into(),
                    project
                        .logical
                        .containers
                        .iter()
                        .filter(|c| c.system == s.id)
                        .count()
                        .to_string(),
                ],
                resource: Resource::System(s.clone()),
            })
            .collect(),
    }
}

fn 服務表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    // 每個環境一欄「幾台」。這是這張表最有用的東西：一眼看出哪個環境還沒建。
    let mut columns = vec![
        "名稱".into(),
        "顯示名".into(),
        "所屬系統".into(),
        "接點".into(),
    ];
    columns.extend(project.environments.iter().map(|e| e.slug.clone()));

    Table {
        kind: Kind::Container,
        title: "服務".into(),
        columns,
        environment: None,
        empty_hint: "還沒有任何服務。Redis、Consul 這種會接收請求的程序都算。".into(),
        rows: project
            .logical
            .containers
            .iter()
            .map(|c| {
                let mut cells = vec![
                    c.slug.clone(),
                    c.name.clone(),
                    project
                        .logical
                        .systems
                        .iter()
                        .find(|s| s.id == c.system)
                        .map(|s| s.slug.clone())
                        // 指向不存在的系統時把 id 印出來——那是 L012，
                        // 使用者需要看到是哪個 id 壞了。
                        .unwrap_or_else(|| c.system.to_string()),
                    c.endpoints
                        .iter()
                        .map(|e| e.slug.as_str())
                        .collect::<Vec<_>>()
                        .join("、"),
                ];
                cells.extend(project.environments.iter().map(|env| {
                    match env
                        .instances()
                        .iter()
                        .filter(|i| i.container == c.id)
                        .count()
                    {
                        0 => "—".to_string(),
                        n => format!("{n} 台"),
                    }
                }));
                ResourceRow {
                    id: c.id.clone(),
                    depth: 0,
                    severity: sev(&c.id),
                    cells,
                    resource: Resource::Container(c.clone()),
                }
            })
            .collect(),
    }
}

fn 接點定義表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    // 接點定義掛在服務或外部系統身上，兩邊攤在同一張表裡看比較好核對
    // ——「哪個服務忘了定義接點」是實際會問的問題。
    let mut rows = Vec::new();
    for c in &project.logical.containers {
        for d in &c.endpoints {
            rows.push(ResourceRow {
                id: d.id.clone(),
                depth: 0,
                severity: sev(&d.id),
                cells: vec![c.slug.clone(), d.slug.clone(), d.protocol.to_string()],
                resource: Resource::EndpointDef {
                    owner: c.id.clone(),
                    def: d.clone(),
                },
            });
        }
    }
    for s in &project.logical.systems {
        for d in &s.endpoints {
            rows.push(ResourceRow {
                id: d.id.clone(),
                depth: 0,
                severity: sev(&d.id),
                cells: vec![s.slug.clone(), d.slug.clone(), d.protocol.to_string()],
                resource: Resource::EndpointDef {
                    owner: s.id.clone(),
                    def: d.clone(),
                },
            });
        }
    }

    Table {
        kind: Kind::EndpointDef,
        title: "接點定義".into(),
        columns: vec!["掛在誰身上".into(), "名稱".into(), "協定".into()],
        environment: None,
        empty_hint: "接點定義是「這個服務怎麼被連上」。契約要指定連到哪一個。".into(),
        rows,
    }
}

fn 契約表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    let 端名 = |e: &crate::logical::RelationshipEnd| -> String {
        use crate::logical::RelationshipEnd as E;
        match e {
            E::Container(id) => project
                .logical
                .container(id)
                .map(|c| c.slug.clone())
                .unwrap_or_else(|| id.to_string()),
            E::System(id) => project
                .logical
                .systems
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.slug.clone())
                .unwrap_or_else(|| id.to_string()),
            E::Person(id) => project
                .logical
                .people
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.slug.clone())
                .unwrap_or_else(|| id.to_string()),
        }
    };

    let mut columns = vec!["名稱".into(), "來源".into(), "目標".into(), "用途".into()];
    // 每個環境一欄「實現了幾段」。契約的重點就是「每個環境都要實現」。
    columns.extend(project.environments.iter().map(|e| e.slug.clone()));

    Table {
        kind: Kind::Relationship,
        title: "契約".into(),
        columns,
        environment: None,
        empty_hint: "契約是「誰要連誰」的母版。每個環境都必須實現，沒實現就會被 lint 抓出來。"
            .into(),
        rows: project
            .logical
            .relationships
            .iter()
            .map(|r| {
                let mut cells = vec![
                    r.slug.clone(),
                    端名(&r.from),
                    端名(&r.to),
                    r.purpose.clone(),
                ];
                cells.extend(project.environments.iter().map(|env| {
                    match env.connections.iter().filter(|c| c.serves == r.id).count() {
                        0 => "—".to_string(),
                        n => format!("{n} 段"),
                    }
                }));
                ResourceRow {
                    id: r.id.clone(),
                    depth: 0,
                    severity: sev(&r.id),
                    cells,
                    resource: Resource::Relationship(r.clone()),
                }
            })
            .collect(),
    }
}

fn 人表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    Table {
        kind: Kind::Person,
        title: "人".into(),
        columns: vec!["名稱".into(), "顯示名".into()],
        environment: None,
        empty_hint: "「使用者連上系統」是 C4 Context 圖最前面那一段，少了它整條流量就缺頭。".into(),
        rows: project
            .logical
            .people
            .iter()
            .map(|p| ResourceRow {
                id: p.id.clone(),
                depth: 0,
                severity: sev(&p.id),
                cells: vec![p.slug.clone(), p.name.clone()],
                resource: Resource::Person(p.clone()),
            })
            .collect(),
    }
}

fn 環境表(project: &Project, sev: 嚴重度查詢<'_>) -> Table {
    Table {
        kind: Kind::Environment,
        title: "環境".into(),
        columns: vec![
            "名稱".into(),
            "顯示名".into(),
            "機器".into(),
            "落地".into(),
            "連線".into(),
        ],
        environment: None,
        empty_hint: "至少要有一個環境，lint 才有話說——邏輯層自己不會缺什麼。".into(),
        rows: project
            .environments
            .iter()
            .map(|e| ResourceRow {
                id: e.id.clone(),
                depth: 0,
                severity: sev(&e.id),
                cells: vec![
                    e.slug.clone(),
                    e.name.clone(),
                    數節點(&e.nodes).to_string(),
                    e.instances().len().to_string(),
                    e.connections.len().to_string(),
                ],
                resource: Resource::Environment(e.clone()),
            })
            .collect(),
    }
}

fn 數節點(nodes: &[DeploymentNode]) -> usize {
    nodes.len() + nodes.iter().map(|n| 數節點(&n.children)).sum::<usize>()
}

fn 機器表(env: &crate::environment::Environment, sev: 嚴重度查詢<'_>) -> Table {
    // 巢狀攤平成一層，用 `depth` 縮排。表格畫不出樹，但縮排看得出層級。
    fn 走(
        nodes: &[DeploymentNode],
        parent: Option<Id>,
        env_id: &Id,
        depth: u32,
        sev: 嚴重度查詢<'_>,
        out: &mut Vec<ResourceRow>,
    ) {
        for n in nodes {
            out.push(ResourceRow {
                id: n.id.clone(),
                depth,
                severity: sev(&n.id),
                cells: vec![
                    n.slug.clone(),
                    n.kind.to_string(),
                    n.instances
                        .iter()
                        .map(|i| i.slug.as_str())
                        .collect::<Vec<_>>()
                        .join("、"),
                ],
                resource: Resource::Node {
                    environment: env_id.clone(),
                    within: parent.clone(),
                    node: n.clone(),
                },
            });
            走(&n.children, Some(n.id.clone()), env_id, depth + 1, sev, out);
        }
    }

    let mut rows = Vec::new();
    走(&env.nodes, None, &env.id, 0, sev, &mut rows);

    Table {
        kind: Kind::Node,
        title: "機器".into(),
        columns: vec!["名稱".into(), "種類".into(), "上面跑的服務".into()],
        environment: Some(env.id.clone()),
        empty_hint: "站點、實體機、VM、Linux 容器都是機器。站點是拿來裝別的機器的。".into(),
        rows,
    }
}

fn 設備表(env: &crate::environment::Environment, sev: 嚴重度查詢<'_>) -> Table {
    Table {
        kind: Kind::Infra,
        title: "設備".into(),
        columns: vec!["名稱".into(), "VIP".into()],
        environment: Some(env.id.clone()),
        empty_hint: "F5 這類 VIP 設備。經過它的流量在模型裡會拆成兩段。".into(),
        rows: env
            .infra
            .iter()
            .map(|n| ResourceRow {
                id: n.id.clone(),
                depth: 0,
                severity: sev(&n.id),
                cells: vec![
                    n.slug.clone(),
                    n.endpoints
                        .iter()
                        .map(|e| {
                            format!(
                                "{}（{}）",
                                e.slug,
                                e.address.as_deref().unwrap_or("沒有位址")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("、"),
                ],
                resource: Resource::Infra {
                    environment: env.id.clone(),
                    node: n.clone(),
                },
            })
            .collect(),
    }
}

fn 外部系統落地表(
    project: &Project,
    env: &crate::environment::Environment,
    sev: 嚴重度查詢<'_>,
) -> Table {
    Table {
        kind: Kind::SystemInstance,
        title: "外部系統落地".into(),
        columns: vec!["名稱".into(), "對應系統".into(), "位址".into()],
        environment: Some(env.id.clone()),
        empty_hint:
            "金流之類的外部系統在這個環境打哪個位址。那些機器不是我們的，所以不放在機器底下。"
                .into(),
        rows: env
            .systems
            .iter()
            .map(|s| ResourceRow {
                id: s.id.clone(),
                depth: 0,
                severity: sev(&s.id),
                cells: vec![
                    s.slug.clone(),
                    project
                        .logical
                        .systems
                        .iter()
                        .find(|x| x.id == s.system)
                        .map(|x| x.slug.clone())
                        .unwrap_or_else(|| s.system.to_string()),
                    s.endpoints
                        .iter()
                        .filter_map(|e| e.address.clone())
                        .collect::<Vec<_>>()
                        .join("、"),
                ],
                resource: Resource::SystemInstance {
                    environment: env.id.clone(),
                    instance: s.clone(),
                },
            })
            .collect(),
    }
}
