//! 資源清單：每種資源一張表。
//!
//! # 為什麼欄位是 Rust 決定的
//!
//! 「服務那張表要顯示幾個接點、在各環境幾台」不是排版選擇，是**模型知識**——
//! 那些數字要走一次實體比對才算得出來。放前端就是把 `coverage` 的邏輯
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

/// 這張表歸哪一層管。
///
/// # 為什麼這是模型知識，不是排版偏好
///
/// **母版與分身的分界是整個領域模型最重要的一件事。** 分頁列若把兩者
/// 畫成同一串，等於在跟使用者說「這些都差不多」。哪張表屬於哪一層
/// 不是畫面決定的，所以由這裡講，前端只負責照著畫分隔線。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TableGroup {
    /// 專案層級：不屬於任何一層。
    ///
    /// 只有「環境」這張表。它列的是**全部**環境，所以不隸屬於某一個環境；
    /// 但它也顯然不是邏輯層的東西。畫面上它不進分頁列，改由環境選單裡的
    /// 「管理環境…」開出來——那正是使用者會去找它的地方。
    Project,
    /// 邏輯層（母版）：定義「有哪些服務、誰要連誰」。
    Logical,
    /// 環境層（分身）：填「實際跑在哪台機器、IP 是什麼」。
    Environment,
}

/// 一種資源的一張表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub kind: Kind,
    /// 分頁上的字，例如「服務」。
    pub title: String,
    pub group: TableGroup,
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
///
/// # 順序是相依順序，不是隨手排的
///
/// 每一張表都需要它前面那張先有東西：沒有系統就掛不了服務、沒有服務就
/// 寫不出契約、沒有機器就放不了服務實體。所以**從頭讀到尾就是一份
/// 「從零開始怎麼建」的說明書**，而畫面上的分頁列直接照這個順序畫。
///
/// 這也是為什麼順序訂在這裡而不是前端：它是相依關係，是模型知識。
pub fn tables(project: &Project, environment: Option<&Id>) -> Vec<Table> {
    let findings = lint(project);
    let severity_of = |id: &Id| {
        findings
            .iter()
            .filter(|f| &f.subject == id)
            .map(|f| f.severity())
            .max()
    };

    let mut out = vec![
        // 專案層級。不進分頁列，由環境選單的「管理環境…」開出來。
        environments_table(project, &severity_of),
        // 邏輯層（母版）。人不依賴任何東西，所以它最前面；
        // 之後每一項都需要它前面那項先存在。
        people_table(project, &severity_of),
        systems_table(project, &severity_of),
        containers_table(project, &severity_of),
        endpoint_defs_table(project, &severity_of),
        relationships_table(project, &severity_of),
    ];

    if let Some(env_id) = environment
        && let Some(env) = project.environment(env_id)
    {
        // 環境層（分身）。機器與設備是先有的地，服務實體與外部系統實體
        // 才放得上去。
        out.push(nodes_table(env, &severity_of));
        out.push(infra_table(env, &severity_of));
        out.push(instances_table(project, env, &severity_of));
        out.push(system_instances_table(project, env, &severity_of));
    }
    out
}

type SeverityLookup<'a> = &'a dyn Fn(&Id) -> Option<Severity>;

fn systems_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
    Table {
        kind: Kind::System,
        title: "系統".into(),
        group: TableGroup::Logical,
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

fn containers_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
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
        group: TableGroup::Logical,
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

fn endpoint_defs_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
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
        group: TableGroup::Logical,
        columns: vec!["掛在誰身上".into(), "名稱".into(), "協定".into()],
        environment: None,
        empty_hint: "接點定義是「這個服務怎麼被連上」。契約要指定連到哪一個。".into(),
        rows,
    }
}

fn relationships_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
    let end_name = |e: &crate::logical::RelationshipEnd| -> String {
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
        group: TableGroup::Logical,
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
                    end_name(&r.from),
                    end_name(&r.to),
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

fn people_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
    Table {
        kind: Kind::Person,
        title: "人".into(),
        group: TableGroup::Logical,
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

fn environments_table(project: &Project, sev: SeverityLookup<'_>) -> Table {
    Table {
        kind: Kind::Environment,
        title: "環境".into(),
        group: TableGroup::Project,
        columns: vec![
            "名稱".into(),
            "顯示名".into(),
            "機器".into(),
            "服務實體".into(),
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
                    count_nodes(&e.nodes).to_string(),
                    e.instances().len().to_string(),
                    e.connections.len().to_string(),
                ],
                resource: Resource::Environment(e.clone()),
            })
            .collect(),
    }
}

fn count_nodes(nodes: &[DeploymentNode]) -> usize {
    nodes.len()
        + nodes
            .iter()
            .map(|n| count_nodes(&n.children))
            .sum::<usize>()
}

fn nodes_table(env: &crate::environment::Environment, sev: SeverityLookup<'_>) -> Table {
    // 巢狀攤平成一層，用 `depth` 縮排。表格畫不出樹，但縮排看得出層級。
    fn walk(
        nodes: &[DeploymentNode],
        parent: Option<Id>,
        env_id: &Id,
        depth: u32,
        sev: SeverityLookup<'_>,
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
            walk(&n.children, Some(n.id.clone()), env_id, depth + 1, sev, out);
        }
    }

    let mut rows = Vec::new();
    walk(&env.nodes, None, &env.id, 0, sev, &mut rows);

    Table {
        kind: Kind::Node,
        title: "機器".into(),
        group: TableGroup::Environment,
        columns: vec!["名稱".into(), "種類".into(), "上面跑的服務".into()],
        environment: Some(env.id.clone()),
        empty_hint: "站點、實體機、VM、Linux 容器都是機器。站點是拿來裝別的機器的。".into(),
        rows,
    }
}

/// 服務實體：一個服務在某台機器上跑起來的那一份。**位址就住在這裡。**
///
/// 在它之前，服務實體只能靠「批次建機器」產生，而位址只能等 L006 叫了才改得到。
/// 也就是說「我知道這台的 IP，我想現在填進去」這件事**沒有地方可以做**。
fn instances_table(
    project: &Project,
    env: &crate::environment::Environment,
    sev: SeverityLookup<'_>,
) -> Table {
    fn walk(
        nodes: &[DeploymentNode],
        project: &Project,
        env_id: &Id,
        sev: SeverityLookup<'_>,
        out: &mut Vec<ResourceRow>,
    ) {
        for n in nodes {
            for i in &n.instances {
                let container = project.logical.container(&i.container);
                out.push(ResourceRow {
                    id: i.id.clone(),
                    depth: 0,
                    severity: sev(&i.id),
                    cells: vec![
                        i.slug.clone(),
                        // 指到不存在的服務時要看得出來，不是留一格空白——
                        // 空白看起來像「還沒填」，而這是 L012 會叫的壞掉的參照。
                        container.map_or_else(|| "（找不到這個服務）".into(), |c| c.slug.clone()),
                        n.slug.clone(),
                        addresses_of(i),
                    ],
                    resource: Resource::Instance {
                        environment: env_id.clone(),
                        node: n.id.clone(),
                        instance: i.clone(),
                    },
                });
            }
            walk(&n.children, project, env_id, sev, out);
        }
    }

    let mut rows = Vec::new();
    walk(&env.nodes, project, &env.id, sev, &mut rows);

    Table {
        kind: Kind::Instance,
        title: "服務實體".into(),
        group: TableGroup::Environment,
        columns: vec![
            "名稱".into(),
            "哪個服務".into(),
            "跑在哪台".into(),
            "位址".into(),
        ],
        environment: Some(env.id.clone()),
        empty_hint: "服務實體是「某個服務在某台機器上跑起來的那一份」，IP 與 port 就填在這裡。\
                     12 台 Redis 就是 12 個服務實體。要一次建很多台的話，用機器那一頁的批次建立。"
            .into(),
        rows,
    }
}

/// 服務實體上所有接點的位址，攤成一行。沒填的用「—」佔位，
/// 這樣一眼就看得出「有這個接點但還沒有位址」，而不是「沒有這個接點」。
fn addresses_of(i: &crate::environment::ContainerInstance) -> String {
    if i.endpoints.is_empty() {
        return String::new();
    }
    i.endpoints
        .iter()
        .map(|e| match &e.address {
            Some(a) => format!("{}：{a}", e.slug),
            None => format!("{}：—", e.slug),
        })
        .collect::<Vec<_>>()
        .join("、")
}

fn infra_table(env: &crate::environment::Environment, sev: SeverityLookup<'_>) -> Table {
    Table {
        kind: Kind::Infra,
        title: "設備".into(),
        group: TableGroup::Environment,
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

fn system_instances_table(
    project: &Project,
    env: &crate::environment::Environment,
    sev: SeverityLookup<'_>,
) -> Table {
    Table {
        kind: Kind::SystemInstance,
        title: "外部系統實體".into(),
        group: TableGroup::Environment,
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
