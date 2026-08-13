//! 一個環境的查表。
//!
//! # 為什麼需要它
//!
//! [`Environment`] 上的便利方法（`instances()`、`instance()`、`instances_matching()`）
//! 每次呼叫都會**重走整棵部署樹並配置一個新的 Vec**。單獨用沒問題，
//! 但 lint 與覆蓋矩陣都要對每條連線做萬用字元比對——幾百條連線 ×
//! 幾千個 Instance，光是配置就是幾十 MB，整體變成平方級。
//!
//! 實測：1200 條連線 / 7200 個 Instance 的專案，lint 從 563ms 降到 15.8ms。
//! 詳見 `docs/scale-limits.md`。
//!
//! # 它只是查表
//!
//! 這裡**不含任何判斷**。什麼叫「缺漏」是 [`crate::lint`] 的事，
//! 這裡只負責讓那些判斷跑得快。

use std::collections::{HashMap, HashSet};

use crate::Project;
use crate::environment::{Connection, ContainerInstance, DeploymentNode, Environment};
use crate::id::Id;

pub(crate) struct EnvIndex<'a> {
    /// 這個環境所有的 Instance，含巢狀節點底下的。
    pub all: Vec<&'a ContainerInstance>,
    by_id: HashMap<&'a Id, &'a ContainerInstance>,
    /// 依 slug 排序，讓萬用字元比對可以二分搜尋出候選範圍。
    by_slug: Vec<&'a ContainerInstance>,
    /// 依 `serves` 分好組的實際連線。
    /// 否則每條邏輯連線都要重掃一次全部連線。
    serving: HashMap<&'a Id, Vec<&'a Connection>>,
    /// 邏輯連線的 id，用來認出「指向不存在的契約」的連線。
    known_relationships: HashSet<&'a Id>,
    /// 邏輯層的人。人沒有實體，只能確認「這個 id 真的存在」。
    known_people: HashSet<&'a Id>,
    /// 每個 Instance 落在哪些 DeploymentNode 底下（含所有祖先）。
    /// `within` 要用它判斷「這台在不在指定的站點裡」。
    ancestors: HashMap<&'a Id, Vec<&'a Id>>,
    /// 這個環境有哪些 DeploymentNode。`within` 指向不存在的節點是 L003。
    known_nodes: HashSet<&'a Id>,
}

impl<'a> EnvIndex<'a> {
    pub fn build(project: &'a Project, env: &'a Environment) -> Self {
        let all = env.instances();
        let by_id = all.iter().map(|i| (&i.id, *i)).collect();

        let mut by_slug = all.clone();
        by_slug.sort_by(|a, b| a.slug.cmp(&b.slug));

        let mut ancestors: HashMap<&Id, Vec<&Id>> = HashMap::new();
        let mut known_nodes: HashSet<&Id> = HashSet::new();
        for node in &env.nodes {
            visit(node, &mut vec![], &mut ancestors, &mut known_nodes);
        }

        let mut serving: HashMap<&Id, Vec<&Connection>> = HashMap::new();
        for conn in &env.connections {
            serving.entry(&conn.serves).or_default().push(conn);
        }

        Self {
            all,
            by_id,
            by_slug,
            serving,
            known_relationships: project
                .logical
                .relationships
                .iter()
                .map(|r| &r.id)
                .collect(),
            known_people: project.logical.people.iter().map(|p| &p.id).collect(),
            ancestors,
            known_nodes,
        }
    }

    pub fn knows_node(&self, id: &Id) -> bool {
        self.known_nodes.contains(id)
    }

    pub fn instance(&self, id: &Id) -> Option<&'a ContainerInstance> {
        self.by_id.get(id).copied()
    }

    pub fn knows_relationship(&self, id: &Id) -> bool {
        self.known_relationships.contains(id)
    }

    pub fn knows_person(&self, id: &Id) -> bool {
        self.known_people.contains(id)
    }

    /// 服務某條邏輯連線的所有實際連線。
    pub fn serving(&self, relationship: &Id) -> &[&'a Connection] {
        const NONE: &[&Connection] = &[];
        self.serving
            .get(relationship)
            .map(Vec::as_slice)
            .unwrap_or(NONE)
    }

    /// 符合樣式的 Instance。
    ///
    /// 樣式的第一段（第一個 `*` 之前）是**必定成立的字面前綴**——
    /// `redis-*` 一定以 `redis-` 開頭。所以先在排序過的清單上二分搜出
    /// 那段前綴的範圍，只對範圍內的做完整比對。
    ///
    /// `*foo` 這種沒有前綴的樣式，範圍就是全部，退回線性掃描——正確但慢，
    /// 而實務上不會有人這樣寫。`tests/lint_project.rs` 有測試釘住這個邊界。
    /// 符合樣式的 Instance。`within` 有值時只算那個節點底下的（含所有子孫）。
    pub fn matching_within(
        &self,
        pattern: &str,
        within: Option<&Id>,
    ) -> Vec<&'a ContainerInstance> {
        let all = self.matching_pattern(pattern);
        let Some(node) = within else { return all };
        all.into_iter()
            .filter(|i| {
                self.ancestors
                    .get(&i.id)
                    .is_some_and(|chain| chain.contains(&node))
            })
            .collect()
    }

    fn matching_pattern(&self, pattern: &str) -> Vec<&'a ContainerInstance> {
        let prefix = pattern.split('*').next().unwrap_or("");
        let lo = self.by_slug.partition_point(|i| i.slug.as_str() < prefix);
        let hi = self
            .by_slug
            .partition_point(|i| i.slug.as_str() < prefix || i.slug.starts_with(prefix));

        self.by_slug[lo..hi]
            .iter()
            .filter(|i| crate::pattern::matches(pattern, &i.slug))
            .copied()
            .collect()
    }
}

/// 走一遍部署樹，記下每個 Instance 的祖先鏈與所有節點 id。
fn visit<'a>(
    node: &'a DeploymentNode,
    chain: &mut Vec<&'a Id>,
    ancestors: &mut HashMap<&'a Id, Vec<&'a Id>>,
    nodes: &mut HashSet<&'a Id>,
) {
    nodes.insert(&node.id);
    chain.push(&node.id);

    for inst in &node.instances {
        ancestors.insert(&inst.id, chain.clone());
    }
    for child in &node.children {
        visit(child, chain, ancestors, nodes);
    }

    chain.pop();
}
