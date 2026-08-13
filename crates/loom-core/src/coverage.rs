//! 覆蓋矩陣：每條邏輯連線 × 每個環境，這格實現了沒有。
//!
//! # 為什麼需要它
//!
//! 一張攤平的連線表只告訴你**已經有的**東西，而漏掉的東西本來就不在表上。
//! 矩陣把「每個環境都要實現每條契約」這條通則畫成方格，**空格就是漏洞**。
//!
//! # 它不重新定義什麼叫缺漏
//!
//! 狀態一律從 [`crate::lint`] 的結果推導。如果這裡自己判斷一次，
//! 遲早會跟 lint 面板講出不一樣的話——同一個環境，矩陣說沒事、
//! 面板說有錯，那使用者就不會再相信任何一邊了。
//!
//! 這裡唯一自己算的是**數字**（幾段、幾台），那不是判斷。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;
use crate::lint::{Finding, Rule, Severity, lint};

/// 一格的狀態。畫面上用顏色區分，順序即嚴重度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// 沒有任何問題。
    Realized,
    /// 有警告，但路是通的。
    Warning,
    /// 有錯誤，但至少畫了連線。例如走不通、數量對不上。
    Broken,
    /// 這個環境完全沒有實現這條契約——**這就是使用者最怕的那格**。
    Missing,
}

/// 矩陣裡的一格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub relationship: Id,
    pub environment: Id,
    pub status: Status,
    /// 這條契約在這個環境被拆成幾段實際連線。
    /// dev 直連是 1，prod 走 F5 是 2。
    pub segments: u32,
    /// 目標端展開成幾個 Instance。萬用字元會展開成一整群。
    pub targets: u32,
    /// 目標端註明的期望數量。`None` 代表沒用萬用字元，或用了卻沒註明。
    pub expect: Option<u32>,
    /// 這格對應到哪些 lint 規則。畫面上點下去就能跳到 lint 面板。
    pub rules: Vec<Rule>,
}

/// 整張矩陣。列是邏輯連線，欄是環境。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Matrix {
    /// 列的順序，與 `project.logical.relationships` 相同。
    pub relationships: Vec<Id>,
    /// 欄的順序，與 `project.environments` 相同。
    pub environments: Vec<Id>,
    /// 每格一項，依 (relationship, environment) 排列，長度為兩者相乘。
    pub cells: Vec<Cell>,
}

impl Matrix {
    pub fn cell(&self, relationship: &Id, environment: &Id) -> Option<&Cell> {
        self.cells
            .iter()
            .find(|c| &c.relationship == relationship && &c.environment == environment)
    }

    /// 完全沒實現的格子。使用者最想先看到的就是這些。
    pub fn missing(&self) -> impl Iterator<Item = &Cell> {
        self.cells.iter().filter(|c| c.status == Status::Missing)
    }
}

/// 算出整張矩陣。
pub fn coverage(project: &Project) -> Matrix {
    let findings = lint(project);

    let mut cells =
        Vec::with_capacity(project.logical.relationships.len() * project.environments.len());

    for env in &project.environments {
        let index = EnvIndex::build(project, env);
        // 這個環境裡，每條實際連線是為了服務哪條契約——用來把
        // 「某條連線的錯」歸到「某條契約的那一格」。
        let 連線歸屬: HashMap<&Id, &Id> =
            env.connections.iter().map(|c| (&c.id, &c.serves)).collect();

        let 這格的規則 = 依格子分組(&findings, env, &連線歸屬);

        for rel in &project.logical.relationships {
            let serving = index.serving(&rel.id);
            let rules = 這格的規則.get(&rel.id).cloned().unwrap_or_default();

            // 目標端的規模看最後一段——那才是真正抵達的地方。
            // 走 F5 時第一段的目標是設備，數量沒有意義。
            let (targets, expect) = serving
                .last()
                .map(|conn| 目標規模(&index, &conn.to))
                .unwrap_or((0, None));

            cells.push(Cell {
                relationship: rel.id.clone(),
                environment: env.id.clone(),
                status: 判定(serving.is_empty(), &rules),
                segments: serving.len() as u32,
                targets,
                expect,
                rules,
            });
        }
    }

    // 依「先列後欄」排好，前端不必再排一次。
    cells.sort_by(|a, b| {
        let 列 = |c: &Cell| {
            project
                .logical
                .relationships
                .iter()
                .position(|r| r.id == c.relationship)
        };
        let 欄 = |c: &Cell| {
            project
                .environments
                .iter()
                .position(|e| e.id == c.environment)
        };
        (列(a), 欄(a)).cmp(&(列(b), 欄(b)))
    });

    Matrix {
        relationships: project
            .logical
            .relationships
            .iter()
            .map(|r| r.id.clone())
            .collect(),
        environments: project.environments.iter().map(|e| e.id.clone()).collect(),
        cells,
    }
}

/// 狀態一律由 lint 的結果決定，這裡不做第二套判斷。
fn 判定(沒有連線: bool, rules: &[Rule]) -> Status {
    if 沒有連線 {
        return Status::Missing;
    }
    match rules.iter().map(|r| r.severity()).max() {
        Some(Severity::Error) => Status::Broken,
        Some(Severity::Warning) => Status::Warning,
        _ => Status::Realized,
    }
}

/// 把這個環境的 findings 歸到各條契約底下。
///
/// 有些規則直接以契約為對象（L001、L002），有些以實際連線為對象
/// （L003、L004、L005、L007）——後者要透過 `serves` 繞回去。
/// 不屬於任何契約的（例如某台機器忘了填位址）不會進矩陣，
/// 它們只出現在 lint 面板。
fn 依格子分組<'a>(
    findings: &'a [Finding],
    env: &Environment,
    連線歸屬: &HashMap<&Id, &'a Id>,
) -> HashMap<Id, Vec<Rule>> {
    let mut grouped: HashMap<Id, Vec<Rule>> = HashMap::new();

    for f in findings {
        if f.environment.as_ref() != Some(&env.id) {
            continue;
        }
        let 契約 = match 連線歸屬.get(&f.subject) {
            Some(serves) => (*serves).clone(),
            None => f.subject.clone(),
        };
        let bucket = grouped.entry(契約).or_default();
        if !bucket.contains(&f.rule) {
            bucket.push(f.rule);
        }
    }

    for rules in grouped.values_mut() {
        rules.sort();
    }
    grouped
}

/// 這一段連線的目標端展開成幾個，以及註明的期望數量。
fn 目標規模(index: &EnvIndex<'_>, to: &Endpointing) -> (u32, Option<u32>) {
    match to {
        Endpointing::Instance { target, .. } => match target {
            InstanceRef::One(_) => (1, None),
            InstanceRef::Pattern {
                slug_pattern,
                expect,
            } => (index.matching(slug_pattern).len() as u32, *expect),
        },
        Endpointing::Infra { .. } | Endpointing::System { .. } => (1, None),
    }
}
