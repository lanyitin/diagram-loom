//! 編輯：對專案的一次修改。
//!
//! # 為什麼是一個 enum，而不是一堆 setter
//!
//! 復原機制（見 [`crate::history`]）需要知道「剛剛發生了什麼」。若編輯散成
//! 幾十個 `&mut` 的 setter，每加一個就要記得同步一次歷史，遲早會漏掉一個——
//! 而漏掉的那一個會**靜靜地**讓復原跳過一步，使用者按了兩次才回到上一步。
//!
//! 收成一個 enum，歷史只需要包住 [`apply`] 這一個入口。
//!
//! # 每個變體都對應一條 lint 規則的修法
//!
//! 這不是巧合，是刻意的。本工具的迴圈是「lint 說哪裡缺 → 使用者補上」，
//! 所以第一批能編輯的東西，就是 lint 會抱怨的那些東西：
//!
//! | 規則 | 修法 |
//! | --- | --- |
//! | L004 / L005 | [`Edit::SetExpect`] |
//! | L006 | [`Edit::SetAddress`] |
//! | L007 | [`Edit::SetPurpose`] |
//! | L008 | [`Edit::SetStandalone`] |
//!
//! 對應關係寫在 [`fix_for`]，**不寫在前端**——否則新增規則時，
//! 前端那份對照表一定會忘記更新。
//!
//! # 找不到目標一律報錯，不安靜地不做事
//!
//! 對一個賣點是「怕漏」的工具來說，「按了沒反應」是最糟的失敗方式：
//! 使用者以為補好了，其實沒有，而 lint 還在叫他也會以為是誤報。

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{
    ConnectionKind, DeploymentNode, Endpoint, Endpointing, Environment, InstanceRef,
};
use crate::id::Id;
use crate::lint::{self, Finding, Rule};

/// 連線的哪一端。萬用字元兩端都可能出現。
///
/// 名字不叫 `Side`，是為了不跟 `table::Side`（表格上一端的完整樣貌）撞名——
/// 型別匯出到 TypeScript 之後是同一個命名空間，撞了就產不出來。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionEnd {
    From,
    To,
}

impl fmt::Display for ConnectionEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionEnd::From => write!(f, "來源"),
            ConnectionEnd::To => write!(f, "目標"),
        }
    }
}

/// 對專案的一次修改。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Edit {
    /// L006 的修法：補上某個 Endpoint 的實際位址。
    ///
    /// Endpoint 可能掛在 Instance、設備或外部系統落地上，所以只給 id，
    /// 由 [`apply`] 自己找。前端不需要知道它住在哪一層。
    SetAddress {
        environment: Id,
        endpoint: Id,
        /// `None` 表示清空。清空是合法操作——它會讓 L006 重新叫，
        /// 那正是「我還不知道位址」該有的狀態。
        address: Option<String>,
    },
    /// L007 的修法：填上用途。
    ///
    /// `environment` 為 `None` 時指的是邏輯層的 Relationship。
    SetPurpose {
        environment: Option<Id>,
        subject: Id,
        purpose: String,
    },
    /// L004 / L005 的修法：萬用字元的期望數量。
    SetExpect {
        environment: Id,
        connection: Id,
        side: ConnectionEnd,
        expect: Option<u32>,
    },
    /// L008 的修法：標記「刻意獨立」，例如冷備機。
    SetStandalone {
        environment: Id,
        /// Instance 或外部系統落地的 id。
        subject: Id,
        standalone: bool,
    },
    /// 改成正常路徑或備援路徑。
    SetConnectionKind {
        environment: Id,
        connection: Id,
        kind: ConnectionKind,
    },
    /// 刪掉一條環境層連線。
    ///
    /// 這是目前唯一的刪除操作，而且刪除一定要先看 [`preview`]——
    /// 少一條連線正是本工具存在要抓的東西。
    DeleteConnection { environment: Id, connection: Id },
}

impl Edit {
    /// 給復原選單看的短標籤，例如「復原：填上位址」。
    pub fn label(&self) -> &'static str {
        match self {
            Edit::SetAddress { .. } => "修改位址",
            Edit::SetPurpose { .. } => "修改用途",
            Edit::SetExpect { .. } => "修改期望數量",
            Edit::SetStandalone { .. } => "標記刻意獨立",
            Edit::SetConnectionKind { .. } => "修改連線種類",
            Edit::DeleteConnection { .. } => "刪除連線",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    NoSuchEnvironment(Id),
    NoSuchEndpoint(Id),
    NoSuchConnection(Id),
    NoSuchRelationship(Id),
    NoSuchSubject(Id),
    /// 對一個「指名單一 Instance」的端點設定期望數量。
    NotAPattern {
        connection: Id,
        side: ConnectionEnd,
    },
    /// 這條規則沒有「填一格就好」的修法。
    NoFix(Rule),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoSuchEnvironment(id) => write!(f, "找不到環境 {id}"),
            EditError::NoSuchEndpoint(id) => write!(f, "找不到 Endpoint {id}"),
            EditError::NoSuchConnection(id) => write!(f, "找不到連線 {id}"),
            EditError::NoSuchRelationship(id) => write!(f, "找不到邏輯連線 {id}"),
            EditError::NoSuchSubject(id) => write!(f, "找不到 {id}"),
            EditError::NotAPattern { connection, side } => write!(
                f,
                "連線 {connection} 的{side}端不是萬用字元，沒有期望數量可以設定"
            ),
            EditError::NoFix(rule) => {
                write!(f, "{} 沒有填一格就能解決的修法", rule.code())
            }
        }
    }
}

impl std::error::Error for EditError {}

/// 套用一次修改。失敗時 `project` **完全沒被動過**。
///
/// 每個分支都會先找到目標才動手，所以不會出現改到一半失敗的中間狀態。
pub fn apply(project: &mut Project, edit: &Edit) -> Result<(), EditError> {
    match edit {
        Edit::SetAddress {
            environment,
            endpoint,
            address,
        } => {
            let env = 找環境(project, environment)?;
            let target =
                找端點(env, endpoint).ok_or_else(|| EditError::NoSuchEndpoint(endpoint.clone()))?;
            // 空字串等同沒填。否則使用者按了空白鍵存檔，L006 就被騙過去了。
            target.address = address
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Ok(())
        }

        Edit::SetPurpose {
            environment: None,
            subject,
            purpose,
        } => {
            let rel = project
                .logical
                .relationships
                .iter_mut()
                .find(|r| &r.id == subject)
                .ok_or_else(|| EditError::NoSuchRelationship(subject.clone()))?;
            rel.purpose = purpose.trim().to_string();
            Ok(())
        }

        Edit::SetPurpose {
            environment: Some(env_id),
            subject,
            purpose,
        } => {
            let env = 找環境(project, env_id)?;
            let conn = env
                .connections
                .iter_mut()
                .find(|c| &c.id == subject)
                .ok_or_else(|| EditError::NoSuchConnection(subject.clone()))?;
            conn.purpose = purpose.trim().to_string();
            Ok(())
        }

        Edit::SetExpect {
            environment,
            connection,
            side,
            expect,
        } => {
            let env = 找環境(project, environment)?;
            let conn = env
                .connections
                .iter_mut()
                .find(|c| &c.id == connection)
                .ok_or_else(|| EditError::NoSuchConnection(connection.clone()))?;
            let 那一端 = match side {
                ConnectionEnd::From => &mut conn.from,
                ConnectionEnd::To => &mut conn.to,
            };
            match 那一端 {
                Endpointing::Instance {
                    target: InstanceRef::Pattern { expect: slot, .. },
                    ..
                } => {
                    *slot = *expect;
                    Ok(())
                }
                _ => Err(EditError::NotAPattern {
                    connection: connection.clone(),
                    side: *side,
                }),
            }
        }

        Edit::SetStandalone {
            environment,
            subject,
            standalone,
        } => {
            let env = 找環境(project, environment)?;
            if let Some(instance) = 找實例(&mut env.nodes, subject) {
                instance.standalone = *standalone;
                return Ok(());
            }
            if let Some(system) = env.systems.iter_mut().find(|s| &s.id == subject) {
                system.standalone = *standalone;
                return Ok(());
            }
            Err(EditError::NoSuchSubject(subject.clone()))
        }

        Edit::SetConnectionKind {
            environment,
            connection,
            kind,
        } => {
            let env = 找環境(project, environment)?;
            let conn = env
                .connections
                .iter_mut()
                .find(|c| &c.id == connection)
                .ok_or_else(|| EditError::NoSuchConnection(connection.clone()))?;
            conn.kind = *kind;
            Ok(())
        }

        Edit::DeleteConnection {
            environment,
            connection,
        } => {
            let env = 找環境(project, environment)?;
            let 原本 = env.connections.len();
            env.connections.retain(|c| &c.id != connection);
            if env.connections.len() == 原本 {
                return Err(EditError::NoSuchConnection(connection.clone()));
            }
            Ok(())
        }
    }
}

/// 「如果套用這次修改，lint 的結果會怎麼變」。
///
/// # 為什麼不另外寫一套影響分析
///
/// 刪除前想知道的是「會不會弄壞什麼」。那個問題**已經有答案了**——就是 lint。
/// 另外寫一套判斷，等於維護第二份「什麼叫缺漏」的標準，兩份遲早會不一致，
/// 而且不一致的時候使用者信的是先看到的那份。
///
/// 作法跟匯入預覽一模一樣：把真的 [`apply`] 跑在複本上，再比對前後的發現。
/// 代價是複製一份專案，幾千個元素是毫秒級。
pub fn preview(project: &Project, edit: &Edit) -> Result<Impact, EditError> {
    let 之前 = lint::lint(project);

    let mut 之後專案 = project.clone();
    apply(&mut 之後專案, edit)?;
    let 之後 = lint::lint(&之後專案);

    Ok(Impact {
        introduced: 之後.iter().filter(|f| !之前.contains(f)).cloned().collect(),
        resolved: 之前.iter().filter(|f| !之後.contains(f)).cloned().collect(),
    })
}

/// [`preview`] 的結果：這次修改會弄壞什麼、會修好什麼。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Impact {
    /// 套用後**新冒出來**的發現。刪除確認框要顯眼地列出這些。
    pub introduced: Vec<Finding>,
    /// 套用後**消失**的發現。
    pub resolved: Vec<Finding>,
}

impl Impact {
    /// 有沒有東西會被弄壞。
    pub fn is_safe(&self) -> bool {
        self.introduced.is_empty()
    }
}

/// 這項發現該用哪種控制項來修。
///
/// 只描述「長什麼樣的輸入」，不描述畫面細節——畫面是前端的事，
/// 但「L006 要填的是位址不是數字」是規則，屬於這裡。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Fix {
    /// 填一段文字（位址、用途）。
    Text {
        /// 欄位提示，例如「10.0.1.11:6379」。
        hint: String,
        current: Option<String>,
    },
    /// 填一個數字（expect）。附上實際符合的數量當預設值。
    Count { suggestion: Option<u32> },
    /// 一個開關（standalone）。
    Toggle { label: String },
}

/// 使用者在 [`Fix`] 的控制項裡填的東西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FixValue {
    Text(String),
    Count(Option<u32>),
    Toggle(bool),
}

/// 把「哪一項發現 + 使用者填了什麼」變成一次 [`Edit`]。
///
/// # 為什麼這一步也在 Rust
///
/// 前端拿到的是 [`Fix`]（該長出什麼控制項）與使用者填的 [`FixValue`]。
/// 「L006 的答案要寫進哪個欄位」是規則，不是畫面——寫在前端的話，
/// 新增一條規則就要記得同步兩個地方，而漏掉的那次是靜靜地不作用。
///
/// 所以前端全程不需要知道 [`Edit`] 有哪些變體。
pub fn edit_for(project: &Project, finding: &Finding, value: &FixValue) -> Result<Edit, EditError> {
    let 環境 = || {
        finding
            .environment
            .clone()
            .ok_or_else(|| EditError::NoSuchEnvironment(Id::new("(未指定)")))
    };

    match (finding.rule, value) {
        (Rule::L006, FixValue::Text(text)) => Ok(Edit::SetAddress {
            environment: 環境()?,
            endpoint: finding.subject.clone(),
            address: Some(text.clone()),
        }),
        (Rule::L007, FixValue::Text(text)) => Ok(Edit::SetPurpose {
            // 邏輯層的 L007 沒有環境，那正是 `SetPurpose` 用 `Option` 的原因。
            environment: finding.environment.clone(),
            subject: finding.subject.clone(),
            purpose: text.clone(),
        }),
        (Rule::L004 | Rule::L005, FixValue::Count(count)) => Ok(Edit::SetExpect {
            environment: 環境()?,
            connection: finding.subject.clone(),
            side: 萬用字元在哪一端(project, finding)?,
            expect: *count,
        }),
        (Rule::L008, FixValue::Toggle(on)) => Ok(Edit::SetStandalone {
            environment: 環境()?,
            subject: finding.subject.clone(),
            standalone: *on,
        }),
        // 型別對不上（例如拿數字去填位址）或這條規則本來就沒有單欄位修法。
        _ => Err(EditError::NoFix(finding.rule)),
    }
}

/// 前端只填了一個數字，沒說是哪一端——因為那不該由它判斷。
fn 萬用字元在哪一端(
    project: &Project,
    finding: &Finding,
) -> Result<ConnectionEnd, EditError> {
    let conn = 環境的(project, finding)
        .and_then(|env| env.connections.iter().find(|c| c.id == finding.subject))
        .ok_or_else(|| EditError::NoSuchConnection(finding.subject.clone()))?;

    for (端, side) in [
        (ConnectionEnd::From, &conn.from),
        (ConnectionEnd::To, &conn.to),
    ] {
        if matches!(
            side,
            Endpointing::Instance {
                target: InstanceRef::Pattern { .. },
                ..
            }
        ) {
            return Ok(端);
        }
    }
    Err(EditError::NotAPattern {
        connection: finding.subject.clone(),
        side: ConnectionEnd::To,
    })
}

/// 一項發現配上它的修法。`None` 表示這條規則沒辦法用單一欄位修好
/// （L001／L002 要新增連線，那是另一個層級的操作）。
pub fn fix_for(project: &Project, finding: &Finding) -> Option<Fix> {
    match finding.rule {
        Rule::L006 => Some(Fix::Text {
            hint: "10.0.1.11:6379".into(),
            current: 環境的(project, finding)
                .and_then(|env| 看端點(env, &finding.subject))
                .and_then(|e| e.address.clone()),
        }),
        Rule::L007 => Some(Fix::Text {
            hint: "這條連線是做什麼用的".into(),
            current: None,
        }),
        Rule::L004 | Rule::L005 => Some(Fix::Count {
            suggestion: 實際數量(project, finding),
        }),
        Rule::L008 => Some(Fix::Toggle {
            label: "刻意獨立（冷備機等）".into(),
        }),
        Rule::L001 | Rule::L002 | Rule::L003 => None,
    }
}

/// L004 的 detail 裡已經有「實際符合 N 個」，但那是給人看的字串。
/// 這裡重新算一次給輸入框當預設值——解析字串來取數字太脆弱了。
fn 實際數量(project: &Project, finding: &Finding) -> Option<u32> {
    let env = 環境的(project, finding)?;
    let conn = env.connections.iter().find(|c| c.id == finding.subject)?;
    for side in [&conn.from, &conn.to] {
        if let Endpointing::Instance {
            target:
                InstanceRef::Pattern {
                    slug_pattern,
                    within,
                    ..
                },
            ..
        } = side
        {
            let index = crate::index::EnvIndex::build(project, env);
            return Some(index.matching_within(slug_pattern, within.as_ref()).len() as u32);
        }
    }
    None
}

fn 環境的<'a>(project: &'a Project, finding: &Finding) -> Option<&'a Environment> {
    project.environment(finding.environment.as_ref()?)
}

fn 找環境<'a>(project: &'a mut Project, id: &Id) -> Result<&'a mut Environment, EditError> {
    project
        .environments
        .iter_mut()
        .find(|e| &e.id == id)
        .ok_or_else(|| EditError::NoSuchEnvironment(id.clone()))
}

/// Endpoint 可能掛在三種地方，這裡一次找完。
fn 找端點<'a>(env: &'a mut Environment, id: &Id) -> Option<&'a mut Endpoint> {
    fn 走節點<'a>(nodes: &'a mut [DeploymentNode], id: &Id) -> Option<&'a mut Endpoint> {
        for node in nodes {
            for instance in &mut node.instances {
                if let Some(found) = instance.endpoints.iter_mut().find(|e| &e.id == id) {
                    return Some(found);
                }
            }
            if let Some(found) = 走節點(&mut node.children, id) {
                return Some(found);
            }
        }
        None
    }

    if let Some(found) = 走節點(&mut env.nodes, id) {
        return Some(found);
    }
    for node in &mut env.infra {
        if let Some(found) = node.endpoints.iter_mut().find(|e| &e.id == id) {
            return Some(found);
        }
    }
    for system in &mut env.systems {
        if let Some(found) = system.endpoints.iter_mut().find(|e| &e.id == id) {
            return Some(found);
        }
    }
    None
}

/// [`找端點`] 的唯讀版。
fn 看端點<'a>(env: &'a Environment, id: &Id) -> Option<&'a Endpoint> {
    env.instances()
        .into_iter()
        .flat_map(|i| &i.endpoints)
        .chain(env.infra.iter().flat_map(|n| &n.endpoints))
        .chain(env.systems.iter().flat_map(|s| &s.endpoints))
        .find(|e| &e.id == id)
}

fn 找實例<'a>(
    nodes: &'a mut [DeploymentNode],
    id: &Id,
) -> Option<&'a mut crate::environment::ContainerInstance> {
    for node in nodes {
        if let Some(found) = node.instances.iter_mut().find(|i| &i.id == id) {
            return Some(found);
        }
        if let Some(found) = 找實例(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}
