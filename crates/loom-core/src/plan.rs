//! 匯入預覽：套用之前先看清楚會動到什麼。
//!
//! # 為什麼一定要有這一步
//!
//! 匯入是唯一會**大量**改動資料的操作。欄位貼錯一格，整個環境就被洗掉，
//! 而且使用者不會馬上發現——他會以為工具算錯了。
//!
//! # 預覽怎麼保證跟實際一樣
//!
//! 不重寫一套「如果匯入的話會怎樣」的邏輯——那註定會跟真正的 importer 漂移。
//! 作法是**把真的 importer 跑在專案的複本上**，然後比對前後兩份的清單。
//! 所以預覽顯示的東西，就是按下套用之後真正會發生的事。
//!
//! 代價是要複製一份專案。幾千個元素的複製是毫秒級，不值得為此換掉正確性。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::importer::{ImportError, Sheet, import};

/// 動到的是什麼東西。畫面上用來分組。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Element {
    Environment,
    /// 邏輯層的服務。
    Container,
    /// 邏輯層的接點定義。
    EndpointDef,
    /// 邏輯層的連線契約。
    Relationship,
    DeploymentNode,
    ContainerInstance,
    /// 某個 Endpoint 的實際位址。
    Address,
    InfrastructureNode,
    SoftwareSystemInstance,
    Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: ChangeKind,
    pub element: Element,
    /// 人看得懂的位置，例如 `prod / redis-01`。
    pub label: String,
    /// 更新前的值。新增時是 `None`。
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub changes: Vec<Change>,
    /// 檔案裡有、但內容完全相同的項目數。重新匯入時這個數字會很大。
    pub unchanged: u32,
    /// importer 自己的計數與警告（例如位址衝突）。
    pub warnings: Vec<String>,
    /// 這份檔案會動到哪些環境。
    pub environments: Vec<String>,
}

/// 算出「如果把這張表匯進去會發生什麼」。
///
/// 回傳計畫**與套用後的專案**。前端按下套用時直接用後者，
/// 不必再跑一次——再跑一次就有可能得到不一樣的結果（例如新產生的 UUID）。
pub fn plan(project: &Project, sheet: &Sheet) -> Result<(Plan, Project), ImportError> {
    let before = inventory_of(project);

    let mut after_project = project.clone();
    let report = import(&mut after_project, sheet)?;
    let after = inventory_of(&after_project);

    let mut changes = Vec::new();
    let mut unchanged = 0u32;

    for (key, (element, label, after)) in &after {
        match before.get(key) {
            None => changes.push(Change {
                kind: ChangeKind::Added,
                element: *element,
                label: label.clone(),
                before: None,
                after: after.clone(),
            }),
            Some((_, _, before)) if before != after => changes.push(Change {
                kind: ChangeKind::Updated,
                element: *element,
                label: label.clone(),
                before: Some(before.clone()),
                after: after.clone(),
            }),
            Some(_) => unchanged += 1,
        }
    }

    // 先依種類、再依名稱。同一類的變更會排在一起，畫面上好讀。
    changes.sort_by(|a, b| (a.element, &a.label).cmp(&(b.element, &b.label)));

    Ok((
        Plan {
            changes,
            unchanged,
            warnings: report.warnings.iter().map(|w| w.to_string()).collect(),
            environments: touched_environments(sheet),
        },
        after_project,
    ))
}

/// 這張表提到哪些環境。讓使用者在套用前就知道範圍。
fn touched_environments(sheet: &Sheet) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for row in 0..sheet.row_count() {
        let v = sheet.value(row, "environment").trim();
        if !v.is_empty() && !seen.iter().any(|s| s == v) {
            seen.push(v.to_string());
        }
    }
    seen
}

/// 把整個專案攤成「路徑 → (種類, 顯示名, 值)」。
///
/// 鍵用 slug 組成而不是 UUID——UUID 每次匯入都會不一樣，用它比對的話
/// 什麼都會變成「新增」。slug 才是使用者心中的身分。
type Inventory = BTreeMap<String, (Element, String, String)>;

fn inventory_of(p: &Project) -> Inventory {
    let mut out: Inventory = BTreeMap::new();

    for c in &p.logical.containers {
        out.insert(
            format!("邏輯/服務/{}", c.slug),
            (Element::Container, c.slug.clone(), c.name.clone()),
        );
        for e in &c.endpoints {
            out.insert(
                format!("邏輯/服務/{}/接點/{}", c.slug, e.slug),
                (
                    Element::EndpointDef,
                    format!("{}／{}", c.slug, e.slug),
                    format!("{:?}", e.protocol),
                ),
            );
        }
    }

    for r in &p.logical.relationships {
        out.insert(
            format!("邏輯/契約/{}", r.slug),
            (Element::Relationship, r.slug.clone(), r.purpose.clone()),
        );
    }

    for env in &p.environments {
        let e = &env.slug;
        out.insert(
            format!("環境/{e}"),
            (Element::Environment, e.clone(), env.name.clone()),
        );

        for node in &env.nodes {
            node_list(&mut out, e, node);
        }

        for infra in &env.infra {
            out.insert(
                format!("環境/{e}/設備/{}", infra.slug),
                (
                    Element::InfrastructureNode,
                    format!("{e}／{}", infra.slug),
                    String::new(),
                ),
            );
            for ep in &infra.endpoints {
                out.insert(
                    format!("環境/{e}/設備/{}/位址/{}", infra.slug, ep.slug),
                    (
                        Element::Address,
                        format!("{e}／{}／{}", infra.slug, ep.slug),
                        ep.address.clone().unwrap_or_default(),
                    ),
                );
            }
        }

        for sys in &env.systems {
            out.insert(
                format!("環境/{e}/外部系統/{}", sys.slug),
                (
                    Element::SoftwareSystemInstance,
                    format!("{e}／{}", sys.slug),
                    String::new(),
                ),
            );
        }

        for conn in &env.connections {
            // 連線沒有名字，身分是「服務哪條契約 + 兩端接到哪」——
            // 跟 importer 判斷 upsert 的依據必須一致，否則預覽會跟實際對不上。
            let relationship_id = p
                .logical
                .relationships
                .iter()
                .find(|r| r.id == conn.serves)
                .map(|r| r.slug.clone())
                .unwrap_or_else(|| conn.serves.to_string());
            let identity = format!("{relationship_id}|{:?}|{:?}", conn.from, conn.to);
            out.insert(
                format!("環境/{e}/連線/{identity}"),
                (
                    Element::Connection,
                    format!("{e}／{relationship_id}"),
                    conn.purpose.clone(),
                ),
            );
        }
    }

    out
}

fn node_list(out: &mut Inventory, env: &str, node: &crate::environment::DeploymentNode) {
    out.insert(
        format!("環境/{env}/機器/{}", node.slug),
        (
            Element::DeploymentNode,
            format!("{env}／{}", node.slug),
            format!("{:?}", node.kind),
        ),
    );

    for inst in &node.instances {
        out.insert(
            format!("環境/{env}/服務實體/{}", inst.slug),
            (
                Element::ContainerInstance,
                format!("{env}／{}", inst.slug),
                node.slug.clone(),
            ),
        );
        for ep in &inst.endpoints {
            out.insert(
                format!("環境/{env}/服務實體/{}/位址/{}", inst.slug, ep.slug),
                (
                    Element::Address,
                    format!("{env}／{}／{}", inst.slug, ep.slug),
                    ep.address.clone().unwrap_or_default(),
                ),
            );
        }
    }

    for child in &node.children {
        node_list(out, env, child);
    }
}
