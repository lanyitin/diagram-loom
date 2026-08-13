//! 從試算表匯入連線資料。
//!
//! 現實中的資料通常就是一張攤平的大表，所以格式是「單一工作表，一列一條連線」，
//! 匯入時自動補齊還不存在的環境、機器、服務與 Endpoint（upsert）。
//! 欄位定義見 `docs/excel-import.md`。
//!
//! # 為什麼中間隔一層 [`Sheet`]
//!
//! 核心的 [`import`] 只認識「表頭 + 幾列字串」，不認識 xlsx 也不認識 csv。
//! 這樣絕大多數的邏輯不需要準備檔案就能測，檔案格式只剩兩個薄薄的轉接函式。
//!
//! # 匯入不刪東西
//!
//! 已存在就更新、不存在就新增，但**絕不刪除**。刪除一律由使用者在 UI 明確操作——
//! 一張漏了幾列的試算表不該把既有資料清掉。

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::Project;
use crate::environment::{
    Connection, ConnectionKind, ContainerInstance, DeploymentNode, Endpoint, Endpointing,
    Environment, InfrastructureNode, InstanceRef, NodeKind,
};
use crate::id::Id;
use crate::logical::{
    Container, EndpointDef, Protocol, Relationship, RelationshipEnd, SoftwareSystem,
};
use crate::slug;

/// 表頭必須具備的欄位。缺任何一個就整份拒絕，不做半套匯入。
const REQUIRED_COLUMNS: [&str; 7] = [
    "environment",
    "purpose",
    "from_node",
    "to_node",
    "to_endpoint",
    "to_address",
    "protocol",
];

/// 每一列都不能留空的欄位。
///
/// `to_address` 不在其中：萬用字元的列是在指涉一群既有機器，
/// 位址由各機器自己的定義提供，那一列填位址反而沒有意義。
const REQUIRED_VALUES: [&str; 6] = [
    "environment",
    "purpose",
    "from_node",
    "to_node",
    "to_endpoint",
    "protocol",
];

/// 所有認得的欄位。
const KNOWN_COLUMNS: [&str; 16] = [
    "environment",
    "purpose",
    "serves",
    "kind",
    "from_site",
    "to_site",
    "from_node",
    "from_service",
    "from_endpoint",
    "to_node",
    "to_service",
    "to_endpoint",
    "to_address",
    "protocol",
    "expect",
    "note",
];

/// 一張攤平的表：表頭加上若干列。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Sheet {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            headers: headers
                .into_iter()
                .map(|h| h.trim().to_lowercase())
                .collect(),
            rows,
        }
    }

    fn column_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h == name)
    }

    /// 取某一列某一欄的值，已去頭尾空白。缺欄或缺值都回空字串。
    pub fn value(&self, row: usize, column: &str) -> &str {
        self.column_index(column)
            .and_then(|i| self.rows[row].get(i))
            .map(|v| v.trim())
            .unwrap_or("")
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// 表頭缺少必填欄位。一次列出全部，不要讓使用者修一個跑一次。
    MissingColumns(Vec<String>),
    /// 某一列的必填欄位是空的。
    EmptyValue { row: usize, column: String },
    /// 認不得的協定。
    UnknownProtocol { row: usize, value: String },
    /// `expect` 不是數字。
    BadExpect { row: usize, value: String },
    /// 名稱不是正規寫法，無法安全使用。
    NotNormalized {
        row: usize,
        column: String,
        produced: String,
        suggestion: String,
    },
    /// 目標是設備（`to_service` 留空）時，無法推導這條連線服務哪條邏輯連線。
    InfraRowNeedsServes { row: usize },
    /// `kind` 欄位認不得。
    UnknownKind { row: usize, value: String },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::MissingColumns(columns) => {
                write!(f, "表頭缺少必填欄位：{}", columns.join("、"))
            }
            ImportError::EmptyValue { row, column } => {
                write!(f, "第 {row} 列的 {column} 不能是空的")
            }
            ImportError::UnknownProtocol { row, value } => write!(
                f,
                "第 {row} 列的協定 {value} 認不得，可用：TCP、UDP、UNIX_SOCKET、JDBC、FILE"
            ),
            ImportError::BadExpect { row, value } => {
                write!(f, "第 {row} 列的 expect「{value}」不是數字")
            }
            ImportError::NotNormalized {
                row,
                column,
                produced,
                suggestion,
            } => write!(
                f,
                "第 {row} 列的 {column}「{produced}」不是正規寫法，建議改成 {suggestion}"
            ),
            ImportError::InfraRowNeedsServes { row } => write!(
                f,
                "第 {row} 列的目標是設備，必須填 serves 指明它服務哪條邏輯連線"
            ),
            ImportError::UnknownKind { row, value } => write!(
                f,
                "第 {row} 列的 kind「{value}」認不得，可用：primary（或留空）、fallback"
            ),
        }
    }
}

impl std::error::Error for ImportError {}

/// 匯入過程中值得提醒、但不足以中斷的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportWarning {
    /// 同一個 Endpoint 在不同列填了不同位址。保留先出現的那個。
    AddressConflict {
        row: usize,
        endpoint: String,
        kept: String,
        ignored: String,
    },
    /// 表頭有認不得的欄位，已忽略。
    UnknownColumn(String),
}

impl fmt::Display for ImportWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportWarning::AddressConflict {
                row,
                endpoint,
                kept,
                ignored,
            } => write!(
                f,
                "第 {row} 列：{endpoint} 已經是 {kept}，忽略不一致的 {ignored}"
            ),
            ImportWarning::UnknownColumn(name) => write!(f, "忽略認不得的欄位 {name}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// 新建立的連線數。
    pub connections: usize,
    /// 已存在、只更新了用途的連線數。重新匯入同一份檔案時這個數字會等於總列數。
    pub connections_updated: usize,
    pub environments_created: usize,
    pub containers_created: usize,
    pub instances_created: usize,
    pub warnings: Vec<ImportWarning>,
}

/// 把一張表匯進專案。專案會被就地修改。
pub fn import(project: &mut Project, sheet: &Sheet) -> Result<ImportReport, ImportError> {
    check_headers(sheet)?;

    let mut report = ImportReport::default();
    for name in &sheet.headers {
        if !KNOWN_COLUMNS.contains(&name.as_str()) {
            report
                .warnings
                .push(ImportWarning::UnknownColumn(name.clone()));
        }
    }

    for index in 0..sheet.row_count() {
        import_row(project, sheet, index, &mut report)?;
    }

    Ok(report)
}

fn check_headers(sheet: &Sheet) -> Result<(), ImportError> {
    let missing: Vec<String> = REQUIRED_COLUMNS
        .iter()
        .filter(|c| sheet.column_index(c).is_none())
        .map(|c| c.to_string())
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ImportError::MissingColumns(missing))
    }
}

fn import_row(
    project: &mut Project,
    sheet: &Sheet,
    index: usize,
    report: &mut ImportReport,
) -> Result<(), ImportError> {
    // 給使用者看的列號：跳過表頭，且從 1 開始，對應試算表上看到的行號。
    let row = index + 2;

    for column in REQUIRED_VALUES {
        if sheet.value(index, column).is_empty() {
            return Err(ImportError::EmptyValue {
                row,
                column: column.into(),
            });
        }
    }

    // 非萬用字元的列指的是一台具體機器，位址不能省。
    if !sheet.value(index, "to_node").contains('*') && sheet.value(index, "to_address").is_empty() {
        return Err(ImportError::EmptyValue {
            row,
            column: "to_address".into(),
        });
    }

    let env_slug = normalized(sheet.value(index, "environment"), row, "environment")?;
    let from_node = normalized(sheet.value(index, "from_node"), row, "from_node")?;
    let to_node_raw = sheet.value(index, "to_node").to_string();
    let from_service =
        optional_normalized(sheet.value(index, "from_service"), row, "from_service")?;
    let from_site = optional_normalized(sheet.value(index, "from_site"), row, "from_site")?;
    let to_site = optional_normalized(sheet.value(index, "to_site"), row, "to_site")?;
    let to_service = optional_normalized(sheet.value(index, "to_service"), row, "to_service")?;
    let to_endpoint = normalized(sheet.value(index, "to_endpoint"), row, "to_endpoint")?;
    let from_endpoint =
        optional_normalized(sheet.value(index, "from_endpoint"), row, "from_endpoint")?;
    let protocol = parse_protocol(sheet.value(index, "protocol"), row)?;
    let expect = parse_expect(sheet.value(index, "expect"), row)?;
    let purpose = sheet.value(index, "purpose").to_string();
    let address = sheet.value(index, "to_address").to_string();
    let serves_slug = sheet.value(index, "serves").to_string();
    let kind = match sheet.value(index, "kind").to_lowercase().as_str() {
        "" | "primary" | "正常" => ConnectionKind::Primary,
        "fallback" | "備援" => ConnectionKind::Fallback,
        other => {
            return Err(ImportError::UnknownKind {
                row,
                value: other.to_string(),
            });
        }
    };

    // `to_node` 含 `*` 代表這一列在**指涉**既有的一群機器，不是在定義新機器。
    let to_is_pattern = to_node_raw.contains('*');

    ensure_environment(project, &env_slug, report);

    // ── 來源端 ─────────────────────────────────────────────
    let from_side = if let Some(service) = &from_service {
        let container = ensure_container(project, service, report);
        if let Some(ep) = &from_endpoint {
            ensure_endpoint_def(project, &container, ep, protocol);
        }
        let site = from_site
            .as_ref()
            .map(|s| ensure_site(project, &env_slug, s));
        let instance = ensure_instance(
            project,
            &env_slug,
            &from_node,
            site.as_ref(),
            service,
            &container,
            report,
        );
        Endpointing::Instance {
            target: InstanceRef::One(instance),
            endpoint: from_endpoint
                .as_ref()
                .and_then(|ep| endpoint_def_id(project, &container, ep)),
        }
    } else {
        Endpointing::Infra {
            node: ensure_infra(project, &env_slug, &from_node),
            endpoint: None,
        }
    };

    // ── 目標端 ─────────────────────────────────────────────
    let (to_side, to_container) = if let Some(service) = &to_service {
        let container = ensure_container(project, service, report);
        let def = ensure_endpoint_def(project, &container, &to_endpoint, protocol);

        let target = if to_is_pattern {
            InstanceRef::Pattern {
                slug_pattern: format!("{to_node_raw}-{service}"),
                // 填了 to_site 就把範圍限定在那個機房底下。
                //
                // 這一步不做的話，站點建了也沒用——`expect` 還是在數總量，
                // 機器從一個機房搬到另一個就抓不到（見 L-C）。
                within: to_site.as_ref().map(|s| ensure_site(project, &env_slug, s)),
                expect,
            }
        } else {
            let node = normalized(&to_node_raw, row, "to_node")?;
            let site = to_site.as_ref().map(|s| ensure_site(project, &env_slug, s));
            let instance = ensure_instance(
                project,
                &env_slug,
                &node,
                site.as_ref(),
                service,
                &container,
                report,
            );
            set_instance_address(
                project,
                &env_slug,
                &instance,
                &def,
                &to_endpoint,
                protocol,
                &address,
                row,
                report,
            );
            InstanceRef::One(instance)
        };

        (
            Endpointing::Instance {
                target,
                endpoint: Some(def),
            },
            Some(container),
        )
    } else {
        let node = normalized(&to_node_raw, row, "to_node")?;
        let infra = ensure_infra(project, &env_slug, &node);
        let endpoint =
            set_infra_address(project, &env_slug, &infra, &to_endpoint, protocol, &address);
        (
            Endpointing::Infra {
                node: infra,
                endpoint: Some(endpoint),
            },
            None,
        )
    };

    // ── 這條連線服務哪條邏輯連線 ────────────────────────────
    let serves = if !serves_slug.is_empty() {
        let slug = normalized(&serves_slug, row, "serves")?;
        ensure_relationship_by_slug(
            project,
            &slug,
            &from_service,
            &to_container,
            &to_endpoint,
            &purpose,
        )
    } else {
        // 沒填 serves 時只能靠兩端的服務推導。目標是設備就推不出來。
        let (Some(from), Some(to)) = (&from_service, &to_container) else {
            return Err(ImportError::InfraRowNeedsServes { row });
        };
        let from_container = ensure_container(project, from, report);
        let slug = format!("{from}-連-{}", container_slug(project, to));
        ensure_relationship(project, &slug, &from_container, to, &to_endpoint, &purpose)
    };

    // 連線也要 upsert，跟其他元素一樣（見 docs/excel-import.md）。
    //
    // 連線沒有名字，所以身分是「服務哪條契約 + 兩端接到哪」。同一個環境裡
    // 這三者相同就是同一條，只有用途可能被改寫。
    //
    // 少了這一步，每次重新匯入同一份 Excel 都會把整份連線複製一遍——
    // 而重新匯入正是這個功能最常見的用法。
    let env = environment_mut(project, &env_slug);
    match env
        .connections
        .iter_mut()
        .find(|c| c.serves == serves && c.from == from_side && c.to == to_side)
    {
        Some(existing) => {
            existing.purpose = purpose;
            existing.kind = kind;
            report.connections_updated += 1;
        }
        None => {
            env.connections.push(Connection {
                id: Id::generate(),
                serves,
                purpose,
                kind,
                from: from_side,
                to: to_side,
            });
            report.connections += 1;
        }
    }

    Ok(())
}

// ── 名稱與數值的解析 ────────────────────────────────────────

fn normalized(value: &str, row: usize, column: &str) -> Result<String, ImportError> {
    if slug::is_normalized(value) {
        return Ok(value.to_string());
    }
    Err(ImportError::NotNormalized {
        row,
        column: column.into(),
        produced: value.to_string(),
        suggestion: slug::slugify(value),
    })
}

fn optional_normalized(
    value: &str,
    row: usize,
    column: &str,
) -> Result<Option<String>, ImportError> {
    if value.is_empty() {
        return Ok(None);
    }
    normalized(value, row, column).map(Some)
}

fn parse_protocol(value: &str, row: usize) -> Result<Protocol, ImportError> {
    match value.to_uppercase().replace('-', "_").as_str() {
        "TCP" => Ok(Protocol::Tcp),
        "UDP" => Ok(Protocol::Udp),
        "UNIX_SOCKET" | "SOCKET" => Ok(Protocol::UnixSocket),
        "JDBC" => Ok(Protocol::Jdbc),
        "FILE" => Ok(Protocol::File),
        _ => Err(ImportError::UnknownProtocol {
            row,
            value: value.to_string(),
        }),
    }
}

fn parse_expect(value: &str, row: usize) -> Result<Option<u32>, ImportError> {
    if value.is_empty() {
        return Ok(None);
    }
    value.parse().map(Some).map_err(|_| ImportError::BadExpect {
        row,
        value: value.to_string(),
    })
}

// ── upsert：有就用、沒有就建，一律不刪 ──────────────────────

fn ensure_environment(project: &mut Project, slug: &str, report: &mut ImportReport) {
    if project.environments.iter().any(|e| e.slug == slug) {
        return;
    }
    project.environments.push(Environment {
        id: Id::generate(),
        slug: slug.to_string(),
        name: slug.to_string(),
        nodes: vec![],
        infra: vec![],
        systems: vec![],
        connections: vec![],
    });
    report.environments_created += 1;
}

fn environment_mut<'a>(project: &'a mut Project, slug: &str) -> &'a mut Environment {
    project
        .environments
        .iter_mut()
        .find(|e| e.slug == slug)
        .expect("環境在此之前已由 ensure_environment 建立")
}

fn ensure_container(project: &mut Project, slug: &str, report: &mut ImportReport) -> Id {
    if let Some(existing) = project.logical.containers.iter().find(|c| c.slug == slug) {
        return existing.id.clone();
    }

    // 匯入的表沒有系統欄位，全部歸到一個預設系統底下，之後可在 UI 調整。
    let system = ensure_default_system(project);
    let id = Id::generate();
    project.logical.containers.push(Container {
        id: id.clone(),
        slug: slug.to_string(),
        name: slug.to_string(),
        system,
        endpoints: vec![],
    });
    report.containers_created += 1;
    id
}

fn ensure_default_system(project: &mut Project) -> Id {
    if let Some(existing) = project.logical.systems.iter().find(|s| !s.external) {
        return existing.id.clone();
    }
    let id = Id::generate();
    project.logical.systems.push(SoftwareSystem {
        id: id.clone(),
        slug: project.slug.clone(),
        name: project.name.clone(),
        external: false,
        endpoints: vec![],
    });
    id
}

fn container_slug(project: &Project, id: &Id) -> String {
    project
        .logical
        .container(id)
        .map(|c| c.slug.clone())
        .unwrap_or_default()
}

fn ensure_endpoint_def(
    project: &mut Project,
    container: &Id,
    slug: &str,
    protocol: Protocol,
) -> Id {
    let target = project
        .logical
        .containers
        .iter_mut()
        .find(|c| &c.id == container)
        .expect("服務在此之前已由 ensure_container 建立");

    if let Some(existing) = target.endpoints.iter().find(|e| e.slug == slug) {
        return existing.id.clone();
    }

    let id = Id::generate();
    target.endpoints.push(EndpointDef {
        id: id.clone(),
        slug: slug.to_string(),
        protocol,
    });
    id
}

fn endpoint_def_id(project: &Project, container: &Id, slug: &str) -> Option<Id> {
    project
        .logical
        .container(container)?
        .endpoints
        .iter()
        .find(|e| e.slug == slug)
        .map(|e| e.id.clone())
}

/// Instance 的名稱固定是 `{機器}-{服務}`。
///
/// 一台機器可能跑多個服務，光用機器名當識別會撞在一起；
/// 加上服務名之後既唯一又可預測，重跑匯入會得到同一個名字。
/// 也剛好讓試算表上 `redis-vm-t*` 這種寫法仍然選得到
/// `redis-vm-t01-redis`（樣式以 `*` 結尾）。
/// 站點（機房）。沒有就建一個。
///
/// 站點是**裝別的節點用的**，本身不跑東西——所以它只有 `children`，
/// 沒有 `instances`。
fn ensure_site(project: &mut Project, env_slug: &str, site_slug: &str) -> Id {
    let env = environment_mut(project, env_slug);
    if let Some(found) = env
        .nodes
        .iter()
        .find(|n| n.slug == site_slug && n.kind == NodeKind::Site)
    {
        return found.id.clone();
    }
    let id = Id::generate();
    env.nodes.push(DeploymentNode {
        id: id.clone(),
        slug: site_slug.to_string(),
        kind: NodeKind::Site,
        children: vec![],
        instances: vec![],
    });
    id
}

/// 把機器擺到正確的位置：有站點就放進站點底下，沒有就放最外層。
///
/// 已經存在但擺錯地方的會被**搬過去**——試算表補上 `site` 欄位之後
/// 重新匯入，機器就會自己歸位，不必手動搬。
fn place_node(env: &mut Environment, node_slug: &str, site: Option<&Id>) -> Id {
    // 先看它是不是已經在對的位置。
    let under_target = match site {
        Some(sid) => env
            .nodes
            .iter()
            .find(|n| &n.id == sid)
            .map(|s| s.children.iter().any(|c| c.slug == node_slug))
            .unwrap_or(false),
        None => env.nodes.iter().any(|n| n.slug == node_slug),
    };
    if under_target {
        return find_machine(&env.nodes, node_slug).expect("剛剛才確認在的");
    }

    // 不在對的位置：從別處抽出來（或新建），再放進去。
    let machine = take_machine(&mut env.nodes, node_slug).unwrap_or_else(|| DeploymentNode {
        id: Id::generate(),
        slug: node_slug.to_string(),
        kind: NodeKind::VirtualMachine,
        children: vec![],
        instances: vec![],
    });
    let id = machine.id.clone();

    match site {
        Some(sid) => env
            .nodes
            .iter_mut()
            .find(|n| &n.id == sid)
            .expect("站點在此之前已由 ensure_site 建立")
            .children
            .push(machine),
        None => env.nodes.push(machine),
    }
    id
}

fn find_machine(nodes: &[DeploymentNode], slug: &str) -> Option<Id> {
    for n in nodes {
        if n.slug == slug {
            return Some(n.id.clone());
        }
        if let Some(found) = find_machine(&n.children, slug) {
            return Some(found);
        }
    }
    None
}

fn take_machine(nodes: &mut Vec<DeploymentNode>, slug: &str) -> Option<DeploymentNode> {
    if let Some(i) = nodes.iter().position(|n| n.slug == slug) {
        return Some(nodes.remove(i));
    }
    for n in nodes.iter_mut() {
        if let Some(found) = take_machine(&mut n.children, slug) {
            return Some(found);
        }
    }
    None
}

fn ensure_instance(
    project: &mut Project,
    env_slug: &str,
    node_slug: &str,
    site: Option<&Id>,
    service_slug: &str,
    container: &Id,
    report: &mut ImportReport,
) -> Id {
    let instance_slug = format!("{node_slug}-{service_slug}");
    let env = environment_mut(project, env_slug);

    // 先把機器擺到正確的位置——即使 Instance 已經存在也要做。
    // 少了這一步，補上 site 欄位之後重新匯入，機器不會歸位。
    let node_id = place_node(env, node_slug, site);

    if let Some(found) = env
        .instances()
        .into_iter()
        .find(|i| i.slug == instance_slug)
    {
        return found.id.clone();
    }

    let instance = ContainerInstance {
        id: Id::generate(),
        slug: instance_slug,
        container: container.clone(),
        endpoints: vec![],
        standalone: false,
    };
    let id = instance.id.clone();

    find_machine_mut(&mut env.nodes, &node_id)
        .expect("剛剛才安置好的")
        .instances
        .push(instance);

    report.instances_created += 1;
    id
}

fn find_machine_mut<'a>(
    nodes: &'a mut [DeploymentNode],
    id: &Id,
) -> Option<&'a mut DeploymentNode> {
    // 先用不可變借用把位置找出來，再一次可變借用——
    // 邊走邊借的寫法過不了 borrow checker。
    if let Some(i) = nodes.iter().position(|n| &n.id == id) {
        return nodes.get_mut(i);
    }
    for n in nodes.iter_mut() {
        if let Some(found) = find_machine_mut(&mut n.children, id) {
            return Some(found);
        }
    }
    None
}

fn ensure_infra(project: &mut Project, env_slug: &str, node_slug: &str) -> Id {
    let env = environment_mut(project, env_slug);
    if let Some(existing) = env.infra.iter().find(|n| n.slug == node_slug) {
        return existing.id.clone();
    }

    let id = Id::generate();
    env.infra.push(InfrastructureNode {
        id: id.clone(),
        slug: node_slug.to_string(),
        endpoints: vec![],
    });
    id
}

/// 位址其實屬於 Endpoint，不屬於連線。同一台被多條連線指到時，
/// 這個值會在試算表上重複出現；不一致時保留先出現的並提醒。
#[allow(clippy::too_many_arguments)]
fn set_instance_address(
    project: &mut Project,
    env_slug: &str,
    instance: &Id,
    def: &Id,
    slug: &str,
    protocol: Protocol,
    address: &str,
    row: usize,
    report: &mut ImportReport,
) {
    let env = environment_mut(project, env_slug);
    let Some(target) = find_instance_mut(env, instance) else {
        return;
    };

    if let Some(existing) = target
        .endpoints
        .iter()
        .find(|e| e.def.as_ref() == Some(def))
    {
        warn_if_conflicting(existing, address, slug, row, report);
        return;
    }

    target.endpoints.push(Endpoint {
        id: Id::generate(),
        slug: slug.to_string(),
        def: Some(def.clone()),
        protocol,
        address: Some(address.to_string()),
    });
}

fn set_infra_address(
    project: &mut Project,
    env_slug: &str,
    infra: &Id,
    slug: &str,
    protocol: Protocol,
    address: &str,
) -> Id {
    let env = environment_mut(project, env_slug);
    let node = env
        .infra
        .iter_mut()
        .find(|n| &n.id == infra)
        .expect("設備在此之前已由 ensure_infra 建立");

    if let Some(existing) = node.endpoints.iter().find(|e| e.slug == slug) {
        return existing.id.clone();
    }

    let id = Id::generate();
    node.endpoints.push(Endpoint {
        id: id.clone(),
        slug: slug.to_string(),
        def: None,
        protocol,
        address: Some(address.to_string()),
    });
    id
}

fn warn_if_conflicting(
    existing: &Endpoint,
    address: &str,
    slug: &str,
    row: usize,
    report: &mut ImportReport,
) {
    let Some(kept) = &existing.address else {
        return;
    };
    if kept == address {
        return;
    }
    report.warnings.push(ImportWarning::AddressConflict {
        row,
        endpoint: slug.to_string(),
        kept: kept.clone(),
        ignored: address.to_string(),
    });
}

fn find_instance_mut<'a>(env: &'a mut Environment, id: &Id) -> Option<&'a mut ContainerInstance> {
    fn walk<'a>(nodes: &'a mut [DeploymentNode], id: &Id) -> Option<&'a mut ContainerInstance> {
        for node in nodes {
            if let Some(found) = node.instances.iter_mut().find(|i| &i.id == id) {
                return Some(found);
            }
            if let Some(found) = walk(&mut node.children, id) {
                return Some(found);
            }
        }
        None
    }
    walk(&mut env.nodes, id)
}

fn ensure_relationship(
    project: &mut Project,
    slug: &str,
    from: &Id,
    to: &Id,
    to_endpoint_slug: &str,
    purpose: &str,
) -> Id {
    if let Some(existing) = project
        .logical
        .relationships
        .iter()
        .find(|r| r.slug == slug)
    {
        return existing.id.clone();
    }

    let to_endpoint = endpoint_def_id(project, to, to_endpoint_slug).unwrap_or_else(Id::generate);
    let id = Id::generate();
    project.logical.relationships.push(Relationship {
        id: id.clone(),
        slug: slug.to_string(),
        purpose: purpose.to_string(),
        from: RelationshipEnd::Container(from.clone()),
        to: RelationshipEnd::Container(to.clone()),
        to_endpoint,
    });
    id
}

/// 使用者明確填了 `serves`：沿用既有的那條，沒有就依這一列的兩端建一條。
fn ensure_relationship_by_slug(
    project: &mut Project,
    slug: &str,
    from_service: &Option<String>,
    to_container: &Option<Id>,
    to_endpoint_slug: &str,
    purpose: &str,
) -> Id {
    if let Some(existing) = project
        .logical
        .relationships
        .iter_mut()
        .find(|r| r.slug == slug)
    {
        // 補上還沒填的用途，但不覆蓋已經有的——跟位址同一條規則。
        //
        // 少了這一段，用 serves 命名的契約會永遠帶著一個 L007 警告，
        // 而且從試算表修不掉。出貨的樣板本來就會產生六個這種警告。
        if existing.purpose.trim().is_empty() {
            existing.purpose = purpose.to_string();
        }
        return existing.id.clone();
    }

    let mut report = ImportReport::default();
    let from = from_service
        .as_ref()
        .map(|s| ensure_container(project, s, &mut report));

    match (from, to_container) {
        (Some(from), Some(to)) => {
            ensure_relationship(project, slug, &from, to, to_endpoint_slug, purpose)
        }
        // 這一列只是路徑的一段（例如「F5 → 後端」），推不出完整的兩端。
        // 先建一條佔位的邏輯連線，等同一個 serves 的其他列補上真正的端點。
        _ => {
            let id = Id::generate();
            project.logical.relationships.push(Relationship {
                id: id.clone(),
                slug: slug.to_string(),
                purpose: purpose.to_string(),
                from: RelationshipEnd::Container(Id::generate()),
                to: RelationshipEnd::Container(Id::generate()),
                to_endpoint: Id::generate(),
            });
            id
        }
    }
}

// ── 檔案格式的轉接 ──────────────────────────────────────────

/// 讀 CSV。第一列是表頭。
pub fn read_csv(path: &Path) -> Result<Sheet, std::io::Error> {
    let mut reader = csv::Reader::from_path(path)?;
    let headers = reader
        .headers()?
        .iter()
        .map(|h| h.to_string())
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    for record in reader.records() {
        rows.push(record?.iter().map(|v| v.to_string()).collect());
    }

    Ok(Sheet::new(headers, rows))
}

/// 讀 xlsx 的第一張工作表。第一列是表頭。
pub fn read_xlsx(path: &Path) -> Result<Sheet, calamine::Error> {
    use calamine::Reader;

    let mut workbook = calamine::open_workbook_auto(path)?;
    let first = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or(calamine::Error::Msg("活頁簿裡沒有任何工作表"))?;
    let range = workbook.worksheet_range(&first)?;

    let mut lines = range.rows();
    let headers: Vec<String> = lines
        .next()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .unwrap_or_default();
    let rows: Vec<Vec<String>> = lines
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect();

    Ok(Sheet::new(headers, rows))
}

/// 一次匯入多張表時共用的欄位順序，也給測試當範本。
pub fn template_headers() -> Vec<String> {
    KNOWN_COLUMNS.iter().map(|c| c.to_string()).collect()
}

/// 把 `(欄位, 值)` 的清單轉成一列，未提供的欄位留空。
pub fn row_from_pairs(pairs: &[(&str, &str)]) -> Vec<String> {
    let lookup: HashMap<&str, &str> = pairs.iter().copied().collect();
    KNOWN_COLUMNS
        .iter()
        .map(|c| lookup.get(c).copied().unwrap_or("").to_string())
        .collect()
}
