//! 圖與模型的對帳。
//!
//! 圖與模型「誰都不是老大」，兩邊都能改。工具的職責不是自動同步，
//! 而是**指出哪裡對不上，讓人裁決**。
//!
//! # 為什麼需要 base 快照
//!
//! 只比對「現在的模型」與「現在的圖」**無法分辨**這兩件事——兩者長得一模一樣：
//!
//! - 使用者**從圖上刪掉了** `redis-07`
//! - 別人**在模型裡新增了** `redis-07`，圖還沒跟上
//!
//! 因此存一份上次對帳完的快照當 base，三方比對（同 git 的策略）。
//!
//! # 這裡不認識 draw.io
//!
//! JS 從 XML 挖出 `[{loomId, label}]` 交給這裡，這裡只回答「這代表什麼」。
//! 對帳的判斷與 lint 是同一類規則，住在一起才不會演化成兩套標準。
//!
//! 圖形的種類刻意不從 JS 傳進來：判斷差異用不到它，而模型自己就知道
//! 每個元素是什麼。少一個欄位就少一個會對不上的地方。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::environment::Environment;
use crate::id::Id;
use crate::store::FileStore;

/// base 快照的存放位置。
pub const SYNC_STATE_PATH: &str = "diagrams/.sync-state.yaml";

/// 模型裡的元素是什麼東西。只用於顯示與「要不要建到模型裡」的提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "kebab-case")]
pub enum ElementKind {
    // ── 環境層（分身）。詳圖畫的就是這些。 ──
    DeploymentNode,
    ContainerInstance,
    InfrastructureNode,
    SoftwareSystemInstance,
    Connection,

    // ── 邏輯層（母版）。**只有簡圖會用到。** ──
    //
    // Context 圖畫的是人與系統，Container 圖畫的是服務與契約——那些都是
    // 邏輯層的東西，一個也不在環境層。它們不在這裡的時候，一張混著
    // Context 與 Container 的圖上，那些框**沒有任何東西可以指**。
    //
    // 詳圖不給這些：詳圖的意思是「這個環境實際跑成什麼樣」，
    // 指到母版等於說「這個框代表那一整群」——那正是簡圖才允許的事。
    Person,
    SoftwareSystem,
    Container,
    Relationship,
}

impl ElementKind {
    /// 這一種是邏輯層的嗎。
    pub fn is_logical(self) -> bool {
        matches!(
            self,
            ElementKind::Person
                | ElementKind::SoftwareSystem
                | ElementKind::Container
                | ElementKind::Relationship
        )
    }
}

/// 模型這一側的一個元素。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelElement {
    pub id: Id,
    pub kind: ElementKind,
    pub label: String,
}

/// 圖這一側的一個形狀。由 JS 從 draw.io 的自訂屬性挖出來。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagramElement {
    pub id: Id,
    pub label: String,
}

/// 上次對帳完的樣子。每張圖各一份。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSnapshot {
    #[serde(default)]
    pub elements: Vec<DiagramElement>,
}

impl SyncSnapshot {
    fn label_of(&self, id: &Id) -> Option<&str> {
        self.elements
            .iter()
            .find(|e| &e.id == id)
            .map(|e| e.label.as_str())
    }
}

/// 所有圖的 base 快照。以圖的相對路徑為鍵。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub diagrams: BTreeMap<String, SyncSnapshot>,
}

impl SyncState {
    /// 讀取 base 快照。
    ///
    /// **只有「檔案不存在」才視為「還沒對帳過」**——第一次使用專案時本來就沒有
    /// 這個檔。其他失敗（權限不足、檔案毀損）一律回報：若也當成空的，
    /// 使用者會看到「所有東西都是新增的」，可能就把圖清掉了。
    pub fn load(store: &impl FileStore) -> Result<Self, String> {
        match store.read(SYNC_STATE_PATH) {
            Ok(text) => yaml_serde::from_str(&text).map_err(|e| e.to_string()),
            Err(e) if e.is_not_found() => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn save(&self, store: &mut impl FileStore) -> Result<(), String> {
        let text = yaml_serde::to_string(self).map_err(|e| e.to_string())?;
        store
            .write(SYNC_STATE_PATH, &text)
            .map_err(|e| e.to_string())
    }

    pub fn snapshot(&self, diagram: &str) -> SyncSnapshot {
        self.diagrams.get(diagram).cloned().unwrap_or_default()
    }
}

/// 一項對不上的地方。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub id: Id,
    /// 給人看的名字，取自目前找得到的最好來源。
    pub label: String,
    pub kind: DifferenceKind,
}

impl Difference {
    /// 這項差異可以怎麼處理。順序即建議順序，第一個是多數情況下的合理選擇。
    pub fn resolutions(&self) -> Vec<Resolution> {
        self.kind.resolutions()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DifferenceKind {
    /// base 有、圖上沒了 → 使用者從圖上刪掉了。
    RemovedFromDiagram,
    /// base 有、模型沒了 → 模型那邊刪掉了，圖還留著。
    RemovedFromModel,
    /// base 沒有、只有模型有 → 模型新增，圖還沒跟上。
    AddedToModel { kind: ElementKind },
    /// base 沒有、只有圖上有 → 圖上多了一個綁著 loomId 但模型查無此元素的形狀。
    AddedToDiagram,
    /// 只有模型那邊改了名字。
    RenamedInModel { from: String, to: String },
    /// 只有圖上改了名字。
    RenamedInDiagram { from: String, to: String },
    /// 兩邊都改了名字，而且改得不一樣。**這一定要問人。**
    RenameConflict {
        /// 上次對帳時的名字。兩邊都是新出現的元素時為 `None`。
        base: Option<String>,
        model: String,
        diagram: String,
    },
}

impl DifferenceKind {
    pub fn resolutions(&self) -> Vec<Resolution> {
        match self {
            DifferenceKind::RemovedFromDiagram => {
                vec![Resolution::AddToDiagram, Resolution::RemoveFromModel]
            }
            DifferenceKind::RemovedFromModel => {
                vec![Resolution::RemoveFromDiagram, Resolution::CreateInModel]
            }
            DifferenceKind::AddedToModel { .. } => {
                vec![Resolution::AddToDiagram, Resolution::LeaveOffDiagram]
            }
            DifferenceKind::AddedToDiagram => {
                vec![Resolution::CreateInModel, Resolution::RemoveFromDiagram]
            }
            DifferenceKind::RenamedInModel { .. } => {
                vec![Resolution::UseModelLabel, Resolution::UseDiagramLabel]
            }
            DifferenceKind::RenamedInDiagram { .. } => {
                vec![Resolution::UseDiagramLabel, Resolution::UseModelLabel]
            }
            DifferenceKind::RenameConflict { .. } => {
                vec![Resolution::UseModelLabel, Resolution::UseDiagramLabel]
            }
        }
    }

    /// 是否非得由人裁決。其餘的差異有合理的預設選擇。
    pub fn needs_human(&self) -> bool {
        matches!(self, DifferenceKind::RenameConflict { .. })
    }
}

/// 使用者可以選的處理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    AddToDiagram,
    RemoveFromDiagram,
    CreateInModel,
    RemoveFromModel,
    /// 這個元素刻意不畫在這張圖上。
    LeaveOffDiagram,
    UseModelLabel,
    UseDiagramLabel,
}

/// 這種圖的「模型側」是什麼。
///
/// **唯一的一份**——標註選單問的是它，弱檢查問的也是它。分成兩份的話，
/// 一張簡圖上指得到的東西，弱檢查會說「模型裡沒有」，而那是一個工具
/// 自己造出來的假缺漏。
pub fn elements_for(
    logical: &crate::logical::Logical,
    env: &Environment,
    kind: crate::diagrams::DiagramKind,
) -> Vec<ModelElement> {
    match kind {
        crate::diagrams::DiagramKind::Detail => model_elements(env),
        crate::diagrams::DiagramKind::Simple => {
            let mut all = logical_elements(logical);
            all.extend(model_elements(env));
            all
        }
    }
}

/// 簡圖的**弱檢查**：圖上引用的東西，模型裡還在不在。
///
/// # 為什麼簡圖不做完整的三方比對
///
/// 簡圖刻意簡化：一個框可能代表一整群，而且它本來就不會把每台機器都畫上去。
/// 拿完整比對去問它，會得到一整頁「模型有、圖上沒有」——**那些全部不是缺漏，
/// 是簡化本身**。而一份全是雜訊的報告，實際效果等於沒有報告。
///
/// # 但也不能完全不檢查
///
/// 「不對帳」如果只是安靜地跳過，那就是這個專案最怕的那種失敗：有人把簡圖
/// 貼進文件，同事看到它從這個工具長出來，**理所當然以為它被檢查過了**。
///
/// 所以留一條擋得住最丟臉那種錯的：**有人把整個服務刪了，投影片上那個框就該叫。**
/// 擋不了漏連線，但擋得掉「圖上畫著一個已經不存在的東西」。
///
/// `model` 要用 [`elements_for`] 算，不然簡圖上指到的邏輯層元素會被誤判成懸空。
pub fn dangling(model: &[ModelElement], diagram: &[DiagramElement]) -> Vec<Difference> {
    let known: BTreeSet<&Id> = model.iter().map(|e| &e.id).collect();
    let mut out: Vec<Difference> = diagram
        .iter()
        .filter(|d| !known.contains(&d.id))
        .map(|d| Difference {
            id: d.id.clone(),
            label: d.label.clone(),
            kind: DifferenceKind::AddedToDiagram,
        })
        .collect();
    // 依 id 排序，理由同 `reconcile`：輸出要穩定、可整份比對。
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 三方比對。**只給詳圖用。**
///
/// 簡圖請用 [`dangling`]——理由寫在那裡。
///
/// 回傳結果依 id 排序，讓輸出穩定、可整份比對。
pub fn reconcile(
    model: &[ModelElement],
    diagram: &[DiagramElement],
    base: &SyncSnapshot,
) -> Vec<Difference> {
    let model_by_id: BTreeMap<&Id, &ModelElement> = model.iter().map(|e| (&e.id, e)).collect();
    let diagram_by_id: BTreeMap<&Id, &DiagramElement> =
        diagram.iter().map(|e| (&e.id, e)).collect();

    let mut all: BTreeSet<&Id> = BTreeSet::new();
    all.extend(model_by_id.keys().copied());
    all.extend(diagram_by_id.keys().copied());
    all.extend(base.elements.iter().map(|e| &e.id));

    let mut differences = Vec::new();

    for id in all {
        let in_base = base.label_of(id);
        let in_model = model_by_id.get(id).copied();
        let in_diagram = diagram_by_id.get(id).copied();

        let found = match (in_base, in_model, in_diagram) {
            // 兩邊都在：只剩名字可能對不上。
            (base_label, Some(m), Some(d)) => compare_labels(id, base_label, m, d),

            // base 有、圖上沒了。
            (Some(_), Some(m), None) => Some(Difference {
                id: id.clone(),
                label: m.label.clone(),
                kind: DifferenceKind::RemovedFromDiagram,
            }),

            // base 有、模型沒了。
            (Some(_), None, Some(d)) => Some(Difference {
                id: id.clone(),
                label: d.label.clone(),
                kind: DifferenceKind::RemovedFromModel,
            }),

            // 兩邊都沒了：意見一致，只要把它從 base 拿掉。
            (Some(_), None, None) => None,

            // base 沒有，只有模型有。
            (None, Some(m), None) => Some(Difference {
                id: id.clone(),
                label: m.label.clone(),
                kind: DifferenceKind::AddedToModel { kind: m.kind },
            }),

            // base 沒有，只有圖上有。
            (None, None, Some(d)) => Some(Difference {
                id: id.clone(),
                label: d.label.clone(),
                kind: DifferenceKind::AddedToDiagram,
            }),

            // 三邊都沒有：id 不會憑空出現在 all 裡。
            (None, None, None) => None,
        };

        differences.extend(found);
    }

    differences
}

fn compare_labels(
    id: &Id,
    base: Option<&str>,
    model: &ModelElement,
    diagram: &DiagramElement,
) -> Option<Difference> {
    // 兩邊名字一樣就沒事，即使雙方都從 base 改成了同一個新名字。
    if model.label == diagram.label {
        return None;
    }

    let kind = match base {
        Some(base) if model.label == base => DifferenceKind::RenamedInDiagram {
            from: base.to_string(),
            to: diagram.label.clone(),
        },
        Some(base) if diagram.label == base => DifferenceKind::RenamedInModel {
            from: base.to_string(),
            to: model.label.clone(),
        },
        base => DifferenceKind::RenameConflict {
            base: base.map(str::to_string),
            model: model.label.clone(),
            diagram: diagram.label.clone(),
        },
    };

    Some(Difference {
        id: id.clone(),
        label: model.label.clone(),
        kind,
    })
}

/// 對帳完成後，用「雙方都同意的樣子」產生新的 base。
///
/// 只收兩邊都存在且名字一致的元素——還在爭議中的東西不該進 base，
/// 否則下次對帳就會把爭議當成已解決。
pub fn settled_snapshot(model: &[ModelElement], diagram: &[DiagramElement]) -> SyncSnapshot {
    let diagram_by_id: BTreeMap<&Id, &DiagramElement> =
        diagram.iter().map(|e| (&e.id, e)).collect();

    let mut elements: Vec<DiagramElement> = model
        .iter()
        .filter_map(|m| {
            let d = diagram_by_id.get(&m.id)?;
            (d.label == m.label).then(|| DiagramElement {
                id: m.id.clone(),
                label: m.label.clone(),
            })
        })
        .collect();

    elements.sort_by(|a, b| a.id.cmp(&b.id));
    SyncSnapshot { elements }
}

/// 某個環境裡「應該畫得出來」的所有元素。
///
/// 這是對帳時模型那一側的輸入。
pub fn model_elements(env: &Environment) -> Vec<ModelElement> {
    let mut found = Vec::new();

    fn walk(nodes: &[crate::environment::DeploymentNode], found: &mut Vec<ModelElement>) {
        for node in nodes {
            found.push(ModelElement {
                id: node.id.clone(),
                kind: ElementKind::DeploymentNode,
                label: node.slug.clone(),
            });
            for instance in &node.instances {
                found.push(ModelElement {
                    id: instance.id.clone(),
                    kind: ElementKind::ContainerInstance,
                    label: instance.slug.clone(),
                });
            }
            walk(&node.children, found);
        }
    }
    walk(&env.nodes, &mut found);

    for node in &env.infra {
        found.push(ModelElement {
            id: node.id.clone(),
            kind: ElementKind::InfrastructureNode,
            label: node.slug.clone(),
        });
    }

    for system in &env.systems {
        found.push(ModelElement {
            id: system.id.clone(),
            kind: ElementKind::SoftwareSystemInstance,
            label: system.slug.clone(),
        });
    }

    for connection in &env.connections {
        found.push(ModelElement {
            id: connection.id.clone(),
            kind: ElementKind::Connection,
            label: connection.purpose.clone(),
        });
    }

    found
}

/// 邏輯層有哪些元素。**只有簡圖用得到。**
///
/// # 為什麼要有這一支
///
/// [`model_elements`] 只走環境層——它是為了詳圖寫的，而詳圖畫的就是
/// 「這個環境實際跑成什麼樣」。
///
/// 但使用者自己畫的圖不一定是部署圖，也可能是 Context 或 Container，
/// 甚至兩種混在一起。那些圖上的框是**人、系統、服務、契約**，
/// 一個都不在環境層——於是它們在標註選單裡找不到任何可以指的東西。
///
/// 順序是使用者讀圖的順序：人 → 系統 → 服務 → 契約，也就是 C4 由外往內。
pub fn logical_elements(logical: &crate::logical::Logical) -> Vec<ModelElement> {
    let mut found = Vec::new();

    for person in &logical.people {
        found.push(ModelElement {
            id: person.id.clone(),
            kind: ElementKind::Person,
            label: person.slug.clone(),
        });
    }
    for system in &logical.systems {
        found.push(ModelElement {
            id: system.id.clone(),
            kind: ElementKind::SoftwareSystem,
            label: system.slug.clone(),
        });
    }
    for container in &logical.containers {
        found.push(ModelElement {
            id: container.id.clone(),
            kind: ElementKind::Container,
            label: container.slug.clone(),
        });
    }
    for relationship in &logical.relationships {
        found.push(ModelElement {
            id: relationship.id.clone(),
            kind: ElementKind::Relationship,
            label: relationship.slug.clone(),
        });
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(pairs: &[(&str, &str)]) -> Vec<ModelElement> {
        pairs
            .iter()
            .map(|(id, label)| ModelElement {
                id: Id::new(*id),
                kind: ElementKind::ContainerInstance,
                label: (*label).into(),
            })
            .collect()
    }

    fn diagram_of(pairs: &[(&str, &str)]) -> Vec<DiagramElement> {
        pairs
            .iter()
            .map(|(id, label)| DiagramElement {
                id: Id::new(*id),
                label: (*label).into(),
            })
            .collect()
    }

    fn snapshot_of(pairs: &[(&str, &str)]) -> SyncSnapshot {
        SyncSnapshot {
            elements: diagram_of(pairs),
        }
    }

    #[test]
    fn no_diff_when_all_three_agree() {
        let diffs = reconcile(
            &model(&[("a", "redis-01")]),
            &diagram_of(&[("a", "redis-01")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert!(diffs.is_empty());
    }

    #[test]
    fn deleted_from_the_diagram() {
        let diffs = reconcile(
            &model(&[("a", "redis-01")]),
            &diagram_of(&[]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, DifferenceKind::RemovedFromDiagram);
    }

    #[test]
    fn added_in_the_model() {
        // 跟上面那個測試的差別只在 base 有沒有這個元素——
        // 沒有 base 就分不出這兩種情況。
        let diffs = reconcile(
            &model(&[("a", "redis-01")]),
            &diagram_of(&[]),
            &snapshot_of(&[]),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs[0].kind,
            DifferenceKind::AddedToModel {
                kind: ElementKind::ContainerInstance
            }
        );
    }

    #[test]
    fn deleted_in_the_model_but_still_on_the_diagram() {
        let diffs = reconcile(
            &model(&[]),
            &diagram_of(&[("a", "redis-01")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert_eq!(diffs[0].kind, DifferenceKind::RemovedFromModel);
    }

    #[test]
    fn diagram_has_a_shape_the_model_does_not_know() {
        let diffs = reconcile(
            &model(&[]),
            &diagram_of(&[("a", "redis-01")]),
            &snapshot_of(&[]),
        );
        assert_eq!(diffs[0].kind, DifferenceKind::AddedToDiagram);
    }

    #[test]
    fn deleting_on_both_sides_is_agreement() {
        let diffs = reconcile(
            &model(&[]),
            &diagram_of(&[]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert!(diffs.is_empty());
    }

    #[test]
    fn renamed_only_in_the_model() {
        let diffs = reconcile(
            &model(&[("a", "redis-primary")]),
            &diagram_of(&[("a", "redis-01")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert_eq!(
            diffs[0].kind,
            DifferenceKind::RenamedInModel {
                from: "redis-01".into(),
                to: "redis-primary".into()
            }
        );
    }

    #[test]
    fn renamed_only_on_the_diagram() {
        let diffs = reconcile(
            &model(&[("a", "redis-01")]),
            &diagram_of(&[("a", "快取主機")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert_eq!(
            diffs[0].kind,
            DifferenceKind::RenamedInDiagram {
                from: "redis-01".into(),
                to: "快取主機".into()
            }
        );
    }

    #[test]
    fn renaming_both_sides_differently_is_a_conflict() {
        let diffs = reconcile(
            &model(&[("a", "redis-primary")]),
            &diagram_of(&[("a", "快取主機")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert_eq!(
            diffs[0].kind,
            DifferenceKind::RenameConflict {
                base: Some("redis-01".into()),
                model: "redis-primary".into(),
                diagram: "快取主機".into(),
            }
        );
        assert!(diffs[0].kind.needs_human());
    }

    #[test]
    fn renaming_both_sides_alike_is_not_a_conflict() {
        let diffs = reconcile(
            &model(&[("a", "快取主機")]),
            &diagram_of(&[("a", "快取主機")]),
            &snapshot_of(&[("a", "redis-01")]),
        );
        assert!(diffs.is_empty());
    }

    #[test]
    fn results_are_sorted_by_id() {
        let diffs = reconcile(
            &model(&[("c", "三"), ("a", "一"), ("b", "二")]),
            &diagram_of(&[]),
            &snapshot_of(&[]),
        );
        let ids: Vec<&str> = diffs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn new_base_keeps_only_what_both_sides_agree_on() {
        let model = model(&[("a", "同意"), ("b", "模型這樣叫"), ("c", "只有模型有")]);
        let diagram = diagram_of(&[("a", "同意"), ("b", "圖上那樣叫"), ("d", "只有圖上有")]);

        let snapshot = settled_snapshot(&model, &diagram);

        let ids: Vec<&str> = snapshot.elements.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["a"], "還在爭議的東西不該進 base");
    }

    #[test]
    fn reconciling_again_after_resolving_is_clean() {
        let model = model(&[("a", "redis-01")]);
        let diagram = diagram_of(&[("a", "redis-01")]);

        let base = settled_snapshot(&model, &diagram);
        assert!(reconcile(&model, &diagram, &base).is_empty());
    }

    #[test]
    fn every_difference_offers_at_least_two_choices() {
        let cases = [
            DifferenceKind::RemovedFromDiagram,
            DifferenceKind::RemovedFromModel,
            DifferenceKind::AddedToModel {
                kind: ElementKind::Connection,
            },
            DifferenceKind::AddedToDiagram,
            DifferenceKind::RenamedInModel {
                from: "a".into(),
                to: "b".into(),
            },
            DifferenceKind::RenamedInDiagram {
                from: "a".into(),
                to: "b".into(),
            },
            DifferenceKind::RenameConflict {
                base: None,
                model: "a".into(),
                diagram: "b".into(),
            },
        ];

        for kind in cases {
            assert!(
                kind.resolutions().len() >= 2,
                "{kind:?} 只給一個選項，等於沒得選"
            );
        }
    }
    // ── 簡圖的弱檢查 ──────────────────────────────────────────

    #[test]
    fn a_simple_diagram_only_reports_things_the_model_no_longer_has() {
        // 完整比對會對簡圖噴出一整頁「模型有、圖上沒有」——那些全部不是缺漏，
        // 是簡化本身。一份全是雜訊的報告，實際效果等於沒有報告。
        let m = model(&[("a", "甲"), ("b", "乙"), ("c", "丙")]);
        let d = diagram_of(&[("a", "甲")]);

        assert_eq!(dangling(&m, &d), vec![]);
    }

    #[test]
    fn a_simple_diagram_still_reports_a_shape_pointing_at_nothing() {
        // 這是留下來的那一條：有人把整個服務刪了，投影片上那個框就該叫。
        let m = model(&[("a", "甲")]);
        let d = diagram_of(&[("a", "甲"), ("沒了", "被刪掉的服務")]);

        let found = dangling(&m, &d);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, Id::from("沒了"));
        assert_eq!(found[0].kind, DifferenceKind::AddedToDiagram);
    }

    #[test]
    fn a_simple_diagram_can_point_at_the_logical_layer_without_looking_dangling() {
        // `elements_for` 是唯一的一份：標註選單給得出人，弱檢查就不能說
        // 「模型裡沒有這個人」。分成兩份的話，工具會自己造出假缺漏。
        let logical = crate::logical::Logical {
            people: vec![crate::logical::Person {
                id: Id::from("p-customer"),
                slug: "customer".into(),
                name: "客戶".into(),
                memo: String::new(),
            }],
            systems: vec![],
            containers: vec![],
            relationships: vec![],
        };
        let env = Environment {
            id: Id::from("env-prod"),
            slug: "prod".into(),
            name: "正式".into(),
            nodes: vec![],
            infra: vec![],
            systems: vec![],
            connections: vec![],
            memo: String::new(),
        };

        let m = elements_for(&logical, &env, crate::diagrams::DiagramKind::Simple);
        let d = diagram_of(&[("p-customer", "客戶")]);
        assert_eq!(dangling(&m, &d), vec![]);

        // 詳圖那一側就會說它懸空——那是對的，詳圖本來就不該指到母版。
        let detail = elements_for(&logical, &env, crate::diagrams::DiagramKind::Detail);
        assert_eq!(dangling(&detail, &d).len(), 1);
    }
}
