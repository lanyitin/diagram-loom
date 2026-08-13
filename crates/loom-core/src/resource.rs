//! 資源的檢視與增刪改。
//!
//! # 為什麼需要這一層
//!
//! 階段 5 把「編輯」定義成「**每一條 lint 會叫的都要修得掉**」。那是個
//! **修補迴圈**，它默默假設了模型已經存在——從 Excel 匯進來、或開一個現成專案。
//!
//! 但空專案的 lint 是**空的**。沒有服務就沒有 L001，沒有契約就沒有 L002。
//! 工具沒話可說，也就沒東西可按。**從零開始的話，那個迴圈根本啟動不了。**
//!
//! 所以這裡補的不是方便功能，是「**創作迴圈**」：先有母版，lint 才有話說。
//!
//! # 一個 enum，不是二十四個 Edit 變體
//!
//! 八種資源乘上增／改／刪是二十四個變體。收成 [`Resource`]（是什麼）加上
//! 三個 [`crate::edit::Edit`] 變體（要做什麼），復原的標籤與驗證都只寫一次。
//!
//! # 刪除允許留下懸空的參照
//!
//! 刪一個服務不會連帶刪掉它的落地與契約——那可能一次消失幾十個東西，
//! 而這工具的重點就是「怕漏」。改成**讓它懸空，由 L012 叫出來**
//! （見 `lint::Rule::L012` 與 `docs/decisions.md`）。
//!
//! 刪除確認框會把「會多出哪些問題」列出來，所以使用者不是盲刪。

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::edit::EditError;
use crate::environment::{
    DeploymentNode, Endpoint, Environment, InfrastructureNode, NodeKind, SoftwareSystemInstance,
};
use crate::id::Id;
use crate::logical::{Container, EndpointDef, Person, Protocol, Relationship, SoftwareSystem};

/// 一個可以增／改／刪的模型元素。
///
/// 帶的是**完整的值**而不是「欄位差異」：整份換掉的語意最單純，
/// 而復原本來就是存整份快照（見 [`crate::history`]），省不了什麼。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Resource {
    // ── 邏輯層：母版。從零開始時先建這些。 ──
    Person(Person),
    System(SoftwareSystem),
    Container(Container),
    /// 接點定義掛在服務或外部系統身上，所以要指名擁有者。
    EndpointDef {
        owner: Id,
        def: EndpointDef,
    },
    Relationship(Relationship),

    // ── 環境層：分身。 ──
    Environment(Environment),
    /// 站點、實體機、VM、Linux 容器。`within` 是父節點。
    Node {
        environment: Id,
        within: Option<Id>,
        node: DeploymentNode,
    },
    /// F5 這類 VIP 設備。
    Infra {
        environment: Id,
        node: InfrastructureNode,
    },
    /// 設備上的一個 VIP。設備不對應邏輯層元素，所以它的接點沒有 `def`。
    InfraEndpoint {
        environment: Id,
        node: Id,
        endpoint: Endpoint,
    },
    /// 外部系統在這個環境的落地。
    SystemInstance {
        environment: Id,
        instance: SoftwareSystemInstance,
    },
}

impl Resource {
    /// 這個元素的 id。增／改／刪都靠它比對。
    pub fn id(&self) -> &Id {
        match self {
            Resource::Person(p) => &p.id,
            Resource::System(s) => &s.id,
            Resource::Container(c) => &c.id,
            Resource::EndpointDef { def, .. } => &def.id,
            Resource::Relationship(r) => &r.id,
            Resource::Environment(e) => &e.id,
            Resource::Node { node, .. } => &node.id,
            Resource::Infra { node, .. } => &node.id,
            Resource::InfraEndpoint { endpoint, .. } => &endpoint.id,
            Resource::SystemInstance { instance, .. } => &instance.id,
        }
    }

    /// 給復原選單看的名字，例如「新增服務」。
    pub fn kind_name(&self) -> &'static str {
        match self {
            Resource::Person(_) => "人",
            Resource::System(_) => "系統",
            Resource::Container(_) => "服務",
            Resource::EndpointDef { .. } => "接點定義",
            Resource::Relationship(_) => "契約",
            Resource::Environment(_) => "環境",
            Resource::Node { .. } => "機器",
            Resource::Infra { .. } => "設備",
            Resource::InfraEndpoint { .. } => "設備接點",
            Resource::SystemInstance { .. } => "外部系統落地",
        }
    }

    /// 顯示名，錯誤訊息用。
    pub fn slug(&self) -> &str {
        match self {
            Resource::Person(p) => &p.slug,
            Resource::System(s) => &s.slug,
            Resource::Container(c) => &c.slug,
            Resource::EndpointDef { def, .. } => &def.slug,
            Resource::Relationship(r) => &r.slug,
            Resource::Environment(e) => &e.slug,
            Resource::Node { node, .. } => &node.slug,
            Resource::Infra { node, .. } => &node.slug,
            Resource::InfraEndpoint { endpoint, .. } => &endpoint.slug,
            Resource::SystemInstance { instance, .. } => &instance.slug,
        }
    }
}

/// 空白的新資源，給表單當起點。
///
/// # 為什麼 id 在這裡就發好
///
/// 跟 `connect::propose`、`batch::plan` 同一個理由：`apply` 必須是決定性的。
/// 若 id 等到套用時才產生，復原之後重做會得到一個不同 id 的元素。
pub fn blank(kind: Kind, environment: Option<Id>, owner: Option<Id>) -> Resource {
    let id = Id::generate();
    match kind {
        Kind::Person => Resource::Person(Person {
            id,
            slug: String::new(),
            name: String::new(),
        }),
        Kind::System => Resource::System(SoftwareSystem {
            id,
            slug: String::new(),
            name: String::new(),
            // 預設是外部系統：自家系統靠自己的 Container 落地，
            // 而使用者手動新增「系統」時，多半是在記一個對外的相依。
            external: true,
            endpoints: vec![],
        }),
        Kind::Container => Resource::Container(Container {
            id,
            slug: String::new(),
            name: String::new(),
            system: owner.unwrap_or_else(|| Id::new("")),
            endpoints: vec![],
        }),
        Kind::EndpointDef => Resource::EndpointDef {
            owner: owner.unwrap_or_else(|| Id::new("")),
            def: EndpointDef {
                id,
                slug: String::new(),
                protocol: Protocol::Tcp,
            },
        },
        Kind::Relationship => Resource::Relationship(Relationship {
            id,
            slug: String::new(),
            purpose: String::new(),
            // 兩端一開始一定是空的，而 L012 會叫。那是對的：
            // 半成品就是該被指出來，不是被型別擋住。
            from: crate::logical::RelationshipEnd::Container(Id::new("")),
            to: crate::logical::RelationshipEnd::Container(Id::new("")),
            to_endpoint: Id::new(""),
        }),
        Kind::Environment => Resource::Environment(Environment {
            id,
            slug: String::new(),
            name: String::new(),
            nodes: vec![],
            infra: vec![],
            systems: vec![],
            connections: vec![],
        }),
        Kind::Node => Resource::Node {
            environment: environment.unwrap_or_else(|| Id::new("")),
            within: owner,
            node: DeploymentNode {
                id,
                slug: String::new(),
                kind: NodeKind::VirtualMachine,
                children: vec![],
                instances: vec![],
            },
        },
        Kind::Infra => Resource::Infra {
            environment: environment.unwrap_or_else(|| Id::new("")),
            node: InfrastructureNode {
                id,
                slug: String::new(),
                endpoints: vec![],
            },
        },
        Kind::InfraEndpoint => Resource::InfraEndpoint {
            environment: environment.unwrap_or_else(|| Id::new("")),
            node: owner.unwrap_or_else(|| Id::new("")),
            endpoint: Endpoint {
                id,
                slug: String::new(),
                def: None,
                protocol: Protocol::Tcp,
                address: None,
            },
        },
        Kind::SystemInstance => Resource::SystemInstance {
            environment: environment.unwrap_or_else(|| Id::new("")),
            instance: SoftwareSystemInstance {
                id,
                slug: String::new(),
                system: owner.unwrap_or_else(|| Id::new("")),
                endpoints: vec![],
                standalone: false,
            },
        },
    }
}

/// [`blank`] 要建哪一種。跟 [`Resource`] 分開，因為前端要先選種類才有內容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Person,
    System,
    Container,
    EndpointDef,
    Relationship,
    Environment,
    Node,
    Infra,
    InfraEndpoint,
    SystemInstance,
}

impl Resource {
    /// 「新增服務」「刪除契約」這種標籤。
    ///
    /// 回傳 `&'static str` 是因為復原歷史存的是靜態字串——
    /// 只有這十種乘上三個動詞，列舉得完。
    pub(crate) fn kind_name_label(&self, 動作: &str) -> &'static str {
        match (動作, self) {
            ("新增", Resource::Person(_)) => "新增人",
            ("新增", Resource::System(_)) => "新增系統",
            ("新增", Resource::Container(_)) => "新增服務",
            ("新增", Resource::EndpointDef { .. }) => "新增接點定義",
            ("新增", Resource::Relationship(_)) => "新增契約",
            ("新增", Resource::Environment(_)) => "新增環境",
            ("新增", Resource::Node { .. }) => "新增機器",
            ("新增", Resource::Infra { .. }) => "新增設備",
            ("新增", Resource::InfraEndpoint { .. }) => "新增設備接點",
            ("新增", Resource::SystemInstance { .. }) => "新增外部系統落地",
            ("刪除", Resource::Person(_)) => "刪除人",
            ("刪除", Resource::System(_)) => "刪除系統",
            ("刪除", Resource::Container(_)) => "刪除服務",
            ("刪除", Resource::EndpointDef { .. }) => "刪除接點定義",
            ("刪除", Resource::Relationship(_)) => "刪除契約",
            ("刪除", Resource::Environment(_)) => "刪除環境",
            ("刪除", Resource::Node { .. }) => "刪除機器",
            ("刪除", Resource::Infra { .. }) => "刪除設備",
            ("刪除", Resource::InfraEndpoint { .. }) => "刪除設備接點",
            ("刪除", Resource::SystemInstance { .. }) => "刪除外部系統落地",
            (_, Resource::Person(_)) => "修改人",
            (_, Resource::System(_)) => "修改系統",
            (_, Resource::Container(_)) => "修改服務",
            (_, Resource::EndpointDef { .. }) => "修改接點定義",
            (_, Resource::Relationship(_)) => "修改契約",
            (_, Resource::Environment(_)) => "修改環境",
            (_, Resource::Node { .. }) => "修改機器",
            (_, Resource::Infra { .. }) => "修改設備",
            (_, Resource::InfraEndpoint { .. }) => "修改設備接點",
            (_, Resource::SystemInstance { .. }) => "修改外部系統落地",
        }
    }
}

/// 這個 slug 在它該唯一的範圍裡已經有人用了。
///
/// # 為什麼 slug 一定要唯一
///
/// slug 是**人看的識別**，而萬用字元（`redis-*`）比對的就是它。
/// 兩個同名的東西會讓 `expect` 數錯，而使用者以為那是兩個不同的東西——
/// 正好是這個工具要防的誤會。id 是 UUID 不會撞，但 slug 會。
fn 撞名(existing: impl IntoIterator<Item = (Id, String)>, r: &Resource) -> Result<(), EditError> {
    let slug = r.slug().trim();
    if slug.is_empty() {
        return Err(EditError::EmptySlug(r.kind_name()));
    }
    for (id, 有的) in existing {
        if 有的 == slug && &id != r.id() {
            return Err(EditError::SlugTaken {
                kind: r.kind_name(),
                slug: slug.to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn add(project: &mut Project, r: &Resource) -> Result<(), EditError> {
    if 找得到(project, r) {
        return Err(EditError::AlreadyExists(r.id().clone()));
    }
    寫進去(project, r, true)
}

pub(crate) fn update(project: &mut Project, r: &Resource) -> Result<(), EditError> {
    if !找得到(project, r) {
        return Err(EditError::NoSuchSubject(r.id().clone()));
    }
    寫進去(project, r, false)
}

pub(crate) fn delete(project: &mut Project, r: &Resource) -> Result<(), EditError> {
    if !找得到(project, r) {
        return Err(EditError::NoSuchSubject(r.id().clone()));
    }
    let id = r.id().clone();
    match r {
        Resource::Person(_) => project.logical.people.retain(|x| x.id != id),
        Resource::System(_) => project.logical.systems.retain(|x| x.id != id),
        Resource::Container(_) => project.logical.containers.retain(|x| x.id != id),
        Resource::Relationship(_) => project.logical.relationships.retain(|x| x.id != id),
        Resource::EndpointDef { owner, .. } => {
            if let Some(c) = project
                .logical
                .containers
                .iter_mut()
                .find(|c| &c.id == owner)
            {
                c.endpoints.retain(|e| e.id != id);
            }
            if let Some(s) = project.logical.systems.iter_mut().find(|s| &s.id == owner) {
                s.endpoints.retain(|e| e.id != id);
            }
        }
        Resource::Environment(_) => project.environments.retain(|e| e.id != id),
        Resource::Node { environment, .. } => {
            let env = 找環境(project, environment)?;
            移除節點(&mut env.nodes, &id);
        }
        Resource::Infra { environment, .. } => {
            找環境(project, environment)?.infra.retain(|n| n.id != id);
        }
        Resource::InfraEndpoint {
            environment, node, ..
        } => {
            let env = 找環境(project, environment)?;
            if let Some(n) = env.infra.iter_mut().find(|n| &n.id == node) {
                n.endpoints.retain(|e| e.id != id);
            }
        }
        Resource::SystemInstance { environment, .. } => {
            找環境(project, environment)?.systems.retain(|s| s.id != id);
        }
    }
    Ok(())
}

/// 新增與修改共用。`新的` 為 true 時 push，否則就地換掉。
fn 寫進去(project: &mut Project, r: &Resource, 新的: bool) -> Result<(), EditError> {
    match r {
        Resource::Person(p) => {
            撞名(
                project
                    .logical
                    .people
                    .iter()
                    .map(|x| (x.id.clone(), x.slug.clone())),
                r,
            )?;
            換或推(&mut project.logical.people, p.clone(), 新的, |x| &x.id);
        }
        Resource::System(s) => {
            撞名(
                project
                    .logical
                    .systems
                    .iter()
                    .map(|x| (x.id.clone(), x.slug.clone())),
                r,
            )?;
            換或推(&mut project.logical.systems, s.clone(), 新的, |x| &x.id);
        }
        Resource::Container(c) => {
            撞名(
                project
                    .logical
                    .containers
                    .iter()
                    .map(|x| (x.id.clone(), x.slug.clone())),
                r,
            )?;
            換或推(&mut project.logical.containers, c.clone(), 新的, |x| {
                &x.id
            });
        }
        Resource::Relationship(rel) => {
            撞名(
                project
                    .logical
                    .relationships
                    .iter()
                    .map(|x| (x.id.clone(), x.slug.clone())),
                r,
            )?;
            換或推(
                &mut project.logical.relationships,
                rel.clone(),
                新的,
                |x| &x.id,
            );
        }
        Resource::EndpointDef { owner, def } => {
            // 接點定義掛在服務或外部系統身上，兩邊都要找。
            if let Some(c) = project
                .logical
                .containers
                .iter_mut()
                .find(|c| &c.id == owner)
            {
                撞名(
                    c.endpoints.iter().map(|e| (e.id.clone(), e.slug.clone())),
                    r,
                )?;
                換或推(&mut c.endpoints, def.clone(), 新的, |x| &x.id);
                return Ok(());
            }
            if let Some(s) = project.logical.systems.iter_mut().find(|s| &s.id == owner) {
                撞名(
                    s.endpoints.iter().map(|e| (e.id.clone(), e.slug.clone())),
                    r,
                )?;
                換或推(&mut s.endpoints, def.clone(), 新的, |x| &x.id);
                return Ok(());
            }
            return Err(EditError::NoSuchSubject(owner.clone()));
        }
        Resource::Environment(e) => {
            撞名(
                project
                    .environments
                    .iter()
                    .map(|x| (x.id.clone(), x.slug.clone())),
                r,
            )?;
            // 環境是整份換掉的，而它裡面裝著機器與連線。修改時只動名字那幾欄，
            // 否則使用者改個名字就會把整個環境的內容清空。
            match project.environments.iter_mut().find(|x| x.id == e.id) {
                Some(舊的) => {
                    舊的.slug = e.slug.clone();
                    舊的.name = e.name.clone();
                }
                None => project.environments.push(e.clone()),
            }
        }
        Resource::Node {
            environment,
            within,
            node,
        } => {
            let env = 找環境(project, environment)?;
            撞名(所有節點(&env.nodes), r)?;
            // 改的時候只動這個節點自己的欄位，不要連子節點與落地一起換掉。
            if let Some(舊的) = 找節點(&mut env.nodes, &node.id) {
                舊的.slug = node.slug.clone();
                舊的.kind = node.kind;
                return Ok(());
            }
            let 放進去 = match within {
                None => &mut env.nodes,
                Some(parent) => {
                    &mut 找節點(&mut env.nodes, parent)
                        .ok_or_else(|| EditError::NoSuchSubject(parent.clone()))?
                        .children
                }
            };
            放進去.push(node.clone());
        }
        Resource::Infra { environment, node } => {
            let env = 找環境(project, environment)?;
            撞名(env.infra.iter().map(|n| (n.id.clone(), n.slug.clone())), r)?;
            match env.infra.iter_mut().find(|n| n.id == node.id) {
                // 同理：改名不該把它身上的 VIP 清掉。
                Some(舊的) => 舊的.slug = node.slug.clone(),
                None => env.infra.push(node.clone()),
            }
        }
        Resource::InfraEndpoint {
            environment,
            node,
            endpoint,
        } => {
            let env = 找環境(project, environment)?;
            let n = env
                .infra
                .iter_mut()
                .find(|n| &n.id == node)
                .ok_or_else(|| EditError::NoSuchSubject(node.clone()))?;
            撞名(
                n.endpoints.iter().map(|e| (e.id.clone(), e.slug.clone())),
                r,
            )?;
            換或推(&mut n.endpoints, endpoint.clone(), 新的, |x| &x.id);
        }
        Resource::SystemInstance {
            environment,
            instance,
        } => {
            let env = 找環境(project, environment)?;
            撞名(
                env.systems.iter().map(|s| (s.id.clone(), s.slug.clone())),
                r,
            )?;
            match env.systems.iter_mut().find(|s| s.id == instance.id) {
                Some(舊的) => {
                    舊的.slug = instance.slug.clone();
                    舊的.system = instance.system.clone();
                    舊的.standalone = instance.standalone;
                }
                None => env.systems.push(instance.clone()),
            }
        }
    }
    Ok(())
}

fn 換或推<T: Clone>(v: &mut Vec<T>, 新值: T, 是新的: bool, id: impl Fn(&T) -> &Id) {
    if 是新的 {
        v.push(新值);
        return;
    }
    let 目標 = id(&新值).clone();
    if let Some(slot) = v.iter_mut().find(|x| id(x) == &目標) {
        *slot = 新值;
    }
}

fn 找得到(project: &Project, r: &Resource) -> bool {
    let id = r.id();
    match r {
        Resource::Person(_) => project.logical.people.iter().any(|x| &x.id == id),
        Resource::System(_) => project.logical.systems.iter().any(|x| &x.id == id),
        Resource::Container(_) => project.logical.containers.iter().any(|x| &x.id == id),
        Resource::Relationship(_) => project.logical.relationships.iter().any(|x| &x.id == id),
        Resource::EndpointDef { owner, .. } => {
            let 在服務上 = project
                .logical
                .containers
                .iter()
                .filter(|c| &c.id == owner)
                .any(|c| c.endpoints.iter().any(|e| &e.id == id));
            let 在系統上 = project
                .logical
                .systems
                .iter()
                .filter(|s| &s.id == owner)
                .any(|s| s.endpoints.iter().any(|e| &e.id == id));
            在服務上 || 在系統上
        }
        Resource::Environment(_) => project.environments.iter().any(|e| &e.id == id),
        Resource::Node { environment, .. } => project
            .environment(environment)
            .is_some_and(|env| 所有節點(&env.nodes).into_iter().any(|(i, _)| &i == id)),
        Resource::Infra { environment, .. } => project
            .environment(environment)
            .is_some_and(|env| env.infra.iter().any(|n| &n.id == id)),
        Resource::InfraEndpoint {
            environment, node, ..
        } => project.environment(environment).is_some_and(|env| {
            env.infra
                .iter()
                .filter(|n| &n.id == node)
                .any(|n| n.endpoints.iter().any(|e| &e.id == id))
        }),
        Resource::SystemInstance { environment, .. } => project
            .environment(environment)
            .is_some_and(|env| env.systems.iter().any(|s| &s.id == id)),
    }
}

fn 所有節點(nodes: &[DeploymentNode]) -> Vec<(Id, String)> {
    let mut out = Vec::new();
    for n in nodes {
        out.push((n.id.clone(), n.slug.clone()));
        out.extend(所有節點(&n.children));
    }
    out
}

fn 找節點<'a>(nodes: &'a mut [DeploymentNode], id: &Id) -> Option<&'a mut DeploymentNode> {
    for node in nodes {
        if &node.id == id {
            return Some(node);
        }
        if let Some(found) = 找節點(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}

/// 刪節點連子孫一起走——它們本來就住在裡面，不是「參照」。
fn 移除節點(nodes: &mut Vec<DeploymentNode>, id: &Id) {
    nodes.retain(|n| &n.id != id);
    for n in nodes {
        移除節點(&mut n.children, id);
    }
}

fn 找環境<'a>(project: &'a mut Project, id: &Id) -> Result<&'a mut Environment, EditError> {
    project
        .environments
        .iter_mut()
        .find(|e| &e.id == id)
        .ok_or_else(|| EditError::NoSuchEnvironment(id.clone()))
}

/// 一個全新的空專案。
///
/// # 為什麼不預先塞一個環境或範例系統
///
/// 預先塞東西的話，使用者第一件事是**刪掉他不要的**，而刪除比新增難懂
/// （要看懂懸空參照的警告）。空的比較誠實：畫面上每張表都會說
/// 「這是什麼、為什麼需要它」，那才是第一次用的人需要的。
pub fn new_project(name: &str) -> Project {
    let name = name.trim();
    Project {
        id: Id::generate(),
        slug: crate::slug::slugify(name),
        name: name.to_string(),
        logical: crate::logical::Logical {
            people: vec![],
            systems: vec![],
            containers: vec![],
            relationships: vec![],
        },
        environments: vec![],
    }
}
