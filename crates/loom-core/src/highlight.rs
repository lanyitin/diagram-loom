//! 圖上哪些東西該亮起來。
//!
//! # 為什麼是調暗不是隱藏
//!
//! 詳圖畫得出來但人看不動——實測那份 fixture 的 prod 是 100 條線。
//! 所以要能篩，但**篩掉的東西是調暗，不是拿掉**。
//!
//! 差別是全部：隱藏等於把課本不看的段落用剪刀剪掉，剪過的課本永遠回答不了
//! 「這一頁有沒有漏字」；調暗只是用螢光筆塗黃要看的那段，**整頁的字一個都沒少**。
//!
//! 具體地說：**篩選不改變圖上有哪些形狀，所以對帳完全不受篩選影響。**
//! 這裡回傳的是「誰該亮」，畫面拿它去改 `opacity`——不刪任何東西。
//!
//! # 為什麼這件事在 Rust
//!
//! 「勾了這條契約，哪些東西算相關」跟 lint 是同一類判斷。放前端會養出
//! 第二套「什麼叫相關」，然後兩套慢慢分岔，而症狀是圖跟表格各說各話。
//!
//! 而且這裡刻意**建在 [`crate::table::rows`] 上**，不自己再走一次萬用字元展開。
//! `Row::subjects` 已經回答過「這條連線牽涉到哪些元素」，重寫一份就是同一個
//! 問題有兩個答案。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{DeploymentNode, Environment};
use crate::id::Id;
use crate::table;

/// 使用者勾了什麼。
///
/// 兩個條件是 **AND**，跟連線表上「搜尋 + 只看有問題」的組合方式一致——
/// 同一個畫面裡兩種組合法會讓人算不準自己看到的是什麼。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Focus {
    /// 只留這幾條契約的。**空的表示不篩**，不是「一條都不要」——
    /// 空的當成全暗的話，一打開篩選面板整張圖就會先黑掉。
    #[serde(default)]
    pub relationships: Vec<Id>,
    /// 只留 lint 有意見的。
    #[serde(default)]
    pub problems: bool,
}

impl Focus {
    /// 什麼都沒勾。畫面拿這個決定「根本不用調暗」。
    pub fn is_empty(&self) -> bool {
        self.relationships.is_empty() && !self.problems
    }
}

/// 該亮的東西。沒被列到的就是要調暗的。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    /// 該亮的形狀。含機器與站點——見 [`highlight`] 裡祖先那一段。
    pub shapes: Vec<Id>,
    /// 該亮的線（實際連線的 id）。
    pub connections: Vec<Id>,
}

/// 算出這個環境裡哪些東西該亮。
///
/// # 祖先也要亮
///
/// 只亮連線兩端的話，圖上會是幾個亮著的服務浮在一片灰裡——**看不出它們跑在
/// 哪台機器上**，而那正是部署圖唯一要回答的問題。所以一個形狀亮，它所在的
/// 機器與站點也要跟著亮。
pub fn highlight(project: &Project, environment: &Id, focus: &Focus) -> Highlight {
    let wanted: BTreeSet<&Id> = focus.relationships.iter().collect();

    let mut shapes: BTreeSet<Id> = BTreeSet::new();
    let mut connections: BTreeSet<Id> = BTreeSet::new();

    for row in table::rows(project) {
        if &row.environment != environment {
            continue;
        }
        if !wanted.is_empty() && !wanted.contains(&row.serves) {
            continue;
        }
        if focus.problems && row.rules.is_empty() {
            continue;
        }

        connections.insert(row.id.clone());
        shapes.extend(row.subjects);
    }

    if let Some(env) = project.environment(environment) {
        light_ancestors(&env.nodes, &mut shapes);
    }

    Highlight {
        shapes: shapes.into_iter().collect(),
        connections: connections.into_iter().collect(),
    }
}

/// 底下有任何東西亮著的節點，自己也要亮。回傳這棵子樹裡有沒有亮的。
fn light_ancestors(nodes: &[DeploymentNode], lit: &mut BTreeSet<Id>) -> bool {
    let mut any = false;
    for node in nodes {
        let below = light_ancestors(&node.children, lit)
            | node.instances.iter().any(|i| lit.contains(&i.id));
        if below {
            lit.insert(node.id.clone());
            any = true;
        }
        if lit.contains(&node.id) {
            any = true;
        }
    }
    any
}

/// 這個環境裡有哪些契約可以勾，附上它現在有沒有問題。
///
/// 「哪些契約在這個環境有連線」要走一次 `serves` 分組，是模型知識。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub relationship: Id,
    pub label: String,
    /// 這個環境裡屬於它的連線數。0 表示契約在這裡沒被實現（L001 會叫）。
    pub connections: u32,
    /// 這幾條連線裡有沒有 lint 叫過。
    pub problems: bool,
}

/// 篩選面板要列的契約。**含連線數 0 的**——沒實現的契約正是最該被看見的，
/// 從清單裡消失的話它就永遠不會被勾到。
pub fn contracts(project: &Project, environment: &Id) -> Vec<Contract> {
    // 先分好組再走契約。在迴圈裡呼叫 `rows` 是平方級——契約有幾百條，
    // 而 `rows` 每次都要重跑一次 lint。
    let mut by_relationship: BTreeMap<&Id, (u32, bool)> = BTreeMap::new();
    for row in table::rows(project) {
        if &row.environment != environment {
            continue;
        }
        let rel = project
            .logical
            .relationships
            .iter()
            .find(|r| r.id == row.serves);
        let Some(rel) = rel else { continue };
        let entry = by_relationship.entry(&rel.id).or_default();
        entry.0 += 1;
        entry.1 |= !row.rules.is_empty();
    }

    project
        .logical
        .relationships
        .iter()
        .map(|rel| {
            let (connections, problems) = by_relationship.get(&rel.id).copied().unwrap_or_default();
            Contract {
                relationship: rel.id.clone(),
                label: rel.slug.clone(),
                connections,
                problems,
            }
        })
        .collect()
}

/// 一個環境裡總共會畫幾個形狀。畫面拿它顯示「亮了幾個 / 共幾個」。
pub fn shape_count(env: &Environment) -> u32 {
    fn walk(nodes: &[DeploymentNode]) -> u32 {
        nodes
            .iter()
            .map(|n| 1 + n.instances.len() as u32 + walk(&n.children))
            .sum()
    }
    walk(&env.nodes) + env.infra.len() as u32 + env.systems.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{
        Connection, ContainerInstance, DeploymentNode, Endpointing, InstanceRef, NodeKind,
    };
    use crate::logical::{
        Container, EndpointDef, Logical, Protocol, Relationship, RelationshipEnd,
    };

    fn instance(slug: &str) -> ContainerInstance {
        ContainerInstance {
            id: Id::from(format!("i-{slug}")),
            slug: slug.into(),
            container: Id::from("c-app"),
            endpoints: vec![],
            standalone: false,
            memo: String::new(),
        }
    }

    fn one(slug: &str) -> Endpointing {
        Endpointing::Instance {
            target: InstanceRef::One(Id::from(format!("i-{slug}"))),
            endpoint: None,
        }
    }

    /// 站點 → 機器 → 兩個服務，外加兩條契約各一條連線。
    fn fixture() -> Project {
        let site = DeploymentNode {
            id: Id::from("n-site"),
            slug: "dc".into(),
            kind: NodeKind::Site,
            children: vec![DeploymentNode {
                id: Id::from("n-vm"),
                slug: "vm".into(),
                kind: NodeKind::VirtualMachine,
                children: vec![],
                instances: vec![instance("a"), instance("b"), instance("c")],
                memo: String::new(),
            }],
            instances: vec![],
            memo: String::new(),
        };

        let env = Environment {
            id: Id::from("e"),
            slug: "prod".into(),
            name: "prod".into(),
            nodes: vec![site],
            infra: vec![],
            systems: vec![],
            connections: vec![
                Connection {
                    id: Id::from("conn-1"),
                    serves: Id::from("r-1"),
                    purpose: "有寫用途".into(),
                    kind: Default::default(),
                    from: one("a"),
                    to: one("b"),
                    memo: String::new(),
                },
                Connection {
                    id: Id::from("conn-2"),
                    serves: Id::from("r-2"),
                    // 空用途會被 L007 叫，用來當「有問題」的那一條。
                    purpose: String::new(),
                    kind: Default::default(),
                    from: one("a"),
                    to: one("c"),
                    memo: String::new(),
                },
            ],
            memo: String::new(),
        };

        let rel = |id: &str| Relationship {
            id: Id::from(id),
            slug: id.into(),
            purpose: "x".into(),
            from: RelationshipEnd::Container(Id::from("c-app")),
            to: RelationshipEnd::Container(Id::from("c-app")),
            to_endpoint: Id::from("ep"),
            memo: String::new(),
        };

        Project {
            id: Id::from("p"),
            slug: "p".into(),
            name: "p".into(),
            logical: Logical {
                people: vec![],
                systems: vec![],
                containers: vec![Container {
                    id: Id::from("c-app"),
                    slug: "app".into(),
                    name: "app".into(),
                    system: Id::from("s"),
                    endpoints: vec![EndpointDef {
                        id: Id::from("ep"),
                        slug: "ep".into(),
                        protocol: Protocol::Tcp,
                        memo: String::new(),
                    }],
                    memo: String::new(),
                }],
                relationships: vec![rel("r-1"), rel("r-2")],
            },
            environments: vec![env],
            memo: String::new(),
        }
    }

    #[test]
    fn nothing_checked_lights_everything() {
        // 空的當成「一條都不要」的話，一打開篩選面板整張圖就會先黑掉。
        let p = fixture();
        let h = highlight(&p, &Id::from("e"), &Focus::default());
        assert_eq!(h.connections.len(), 2);
        assert!(h.shapes.contains(&Id::from("i-a")));
        assert!(h.shapes.contains(&Id::from("i-c")));
    }

    #[test]
    fn checking_one_contract_leaves_the_others_dark() {
        let p = fixture();
        let h = highlight(
            &p,
            &Id::from("e"),
            &Focus {
                relationships: vec![Id::from("r-1")],
                problems: false,
            },
        );
        assert_eq!(h.connections, vec![Id::from("conn-1")]);
        assert!(h.shapes.contains(&Id::from("i-b")));
        assert!(
            !h.shapes.contains(&Id::from("i-c")),
            "另一條契約的那端不該亮"
        );
    }

    #[test]
    fn the_machine_and_the_site_light_up_too() {
        // 只亮兩端的話，圖上是幾個亮著的服務浮在一片灰裡——看不出它們
        // 跑在哪台機器上，而那正是部署圖唯一要回答的問題。
        let p = fixture();
        let h = highlight(
            &p,
            &Id::from("e"),
            &Focus {
                relationships: vec![Id::from("r-1")],
                problems: false,
            },
        );
        assert!(h.shapes.contains(&Id::from("n-vm")), "機器要亮");
        assert!(h.shapes.contains(&Id::from("n-site")), "站點也要亮");
    }

    #[test]
    fn only_problems_keeps_the_ones_lint_complained_about() {
        let p = fixture();
        let h = highlight(
            &p,
            &Id::from("e"),
            &Focus {
                relationships: vec![],
                problems: true,
            },
        );
        assert_eq!(
            h.connections,
            vec![Id::from("conn-2")],
            "只有沒寫用途的那條"
        );
    }

    #[test]
    fn the_two_conditions_are_and_not_or() {
        // 跟連線表上「搜尋 + 只看有問題」的組合方式一致。
        // 兩種組合法混在同一個畫面裡，人會算不準自己看到的是什麼。
        let p = fixture();
        let h = highlight(
            &p,
            &Id::from("e"),
            &Focus {
                relationships: vec![Id::from("r-1")],
                problems: true,
            },
        );
        assert!(h.connections.is_empty(), "r-1 那條沒問題，所以一條都不該亮");
    }

    #[test]
    fn another_environment_does_not_leak_in() {
        // 一張圖只屬於一個環境。
        let p = fixture();
        let h = highlight(&p, &Id::from("不存在"), &Focus::default());
        assert!(h.connections.is_empty());
        assert!(h.shapes.is_empty());
    }

    #[test]
    fn unimplemented_contracts_stay_in_the_list() {
        // 沒實現的契約正是最該被看見的。從清單裡消失的話它永遠不會被勾到，
        // 而「這個環境少了什麼」正是這工具存在的理由。
        let mut p = fixture();
        p.environments[0]
            .connections
            .retain(|c| c.serves != Id::from("r-2"));
        let list = contracts(&p, &Id::from("e"));
        let orphan = list
            .iter()
            .find(|c| c.relationship == Id::from("r-2"))
            .unwrap();
        assert_eq!(orphan.connections, 0);
    }

    #[test]
    fn the_choice_list_says_which_contracts_are_in_trouble() {
        let p = fixture();
        let list = contracts(&p, &Id::from("e"));
        let by = |id: &str| {
            list.iter()
                .find(|c| c.relationship == Id::from(id))
                .unwrap()
        };
        assert!(!by("r-1").problems);
        assert!(by("r-2").problems, "沒寫用途的那條契約要標出來");
    }

    #[test]
    fn the_shape_count_covers_nested_machines() {
        // 「亮了幾個 / 共幾個」的分母。漏算巢狀的話這個數字會一直說謊。
        let p = fixture();
        assert_eq!(shape_count(&p.environments[0]), 5, "站點 + 機器 + 三個服務");
    }
}
