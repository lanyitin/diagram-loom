//! 圖上還沒指定的形狀，可以指給哪個模型元素。
//!
//! # 為什麼是清單，不是右鍵
//!
//! 想做的事是「使用者上傳自己畫好的圖，工具在上面疊加標註資訊」。直覺的做法是
//! 點一個方框、跳出選單問它代表誰——但**嵌入協定沒有選取事件**，站在 iframe
//! 外面永遠不知道使用者選了哪個形狀（見 `docs/canvas-engine.md`）。
//!
//! 把流程反過來就成立了：列出圖上**還沒指定的形狀**，做成一張待辦清單。
//!
//! 而且這樣可能比右鍵更好。這個工具的命是**怕漏**：右鍵一次處理一個，
//! 使用者得自己記得去點每一個框，永遠不知道自己還剩幾個沒點；
//! 清單天生會說「還剩 12 個」。
//!
//! # 為什麼這件事在 Rust
//!
//! 「這個框可以指給誰」跟 lint、對帳是同一類判斷——它問的是模型裡有什麼、
//! 哪些已經被用掉了。放前端會養出第二套標準，然後跟對帳慢慢分岔。
//!
//! JS 只負責說「圖上有這些沒指定的形狀」（從 XML 挖 cell id 與文字），
//! 這裡回答「這代表什麼」。跟對帳是同一條分界線。
//!
//! 這裡也**建在 [`crate::reconcile::model_elements`] 上**，不自己再走一次模型樹。
//! 對帳問的是「模型有什麼」，標註問的也是同一件事，答案只能有一份。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::environment::Environment;
use crate::id::Id;
use crate::reconcile::{ElementKind, model_elements};
use crate::slug::slugify;

/// 圖上一個沒有 `loomId` 的形狀。由 JS 從 XML 挖出來。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UnboundShape {
    /// XML 裡的 cell id。指定時要改的就是它，所以它是這裡唯一的身分。
    pub cell: String,
    /// 圖上的文字。**可能是空的**——沒有文字的形狀通常是裝飾，
    /// 但它仍然要送進來，否則畫面上的「還剩幾個」會少算。
    pub label: String,
    /// 是線還是框。線只能指給連線，框只能指給元素——見 [`annotate`]。
    #[serde(default)]
    pub edge: bool,
}

/// 一個可以被指定的模型元素。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: Id,
    pub kind: ElementKind,
    pub label: String,
}

/// 「這個框看起來就是那個元素」。
///
/// 只是建議，**不會自己套用**。指定要由人按下去——猜錯一個綁定，
/// 之後的對帳會拿它當事實，而使用者不會知道自己沒看過那一項。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Guess {
    pub cell: String,
    pub target: Id,
}

/// 一次標註要用的全部材料。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    /// 框可以指給誰。
    pub shapes: Vec<Target>,
    /// 線可以指給誰。
    pub connections: Vec<Target>,
    /// 猜得到的那幾個。清單很長時，這是唯一讓它跑得完的東西。
    pub guesses: Vec<Guess>,
}

/// 算出「這些沒指定的形狀，可以指給誰」。
///
/// # 已經被指定走的不再出現
///
/// `bound` 是圖上已經綁著的 `loomId`（JS 從 XML 挖的）。一個模型元素只能對應
/// 一個形狀——對帳是集合比對，同一個 id 出現在兩個形狀上，兩邊就永遠對不起來
/// （見 `src/lib/diagram.ts` 的模組註解）。所以指定過的元素要從清單裡拿掉，
/// 而不是讓使用者自己記得別選第二次。
///
/// # 線跟框分開
///
/// 線只能指給連線，框只能指給元素。混在一起的話，使用者可以把一個方框指成
/// 一條連線——那在對帳時會變成一個永遠對不上的東西，而且看不出是怎麼來的。
pub fn annotate(env: &Environment, bound: &[Id], shapes: &[UnboundShape]) -> Annotation {
    let taken: BTreeSet<&Id> = bound.iter().collect();

    let mut boxes: Vec<Target> = Vec::new();
    let mut connections: Vec<Target> = Vec::new();

    // 順序沿用 model_elements：站點 → 機器 → 服務實體。那是使用者讀圖的順序。
    for element in model_elements(env) {
        if taken.contains(&element.id) {
            continue;
        }
        let target = Target {
            id: element.id,
            kind: element.kind,
            label: element.label,
        };
        if target.kind == ElementKind::Connection {
            connections.push(target);
        } else {
            boxes.push(target);
        }
    }

    let mut guesses = guess(shapes, &boxes, false);
    guesses.extend(guess(shapes, &connections, true));
    guesses.sort_by(|a, b| a.cell.cmp(&b.cell));

    Annotation {
        shapes: boxes,
        connections,
        guesses,
    }
}

/// 猜「這個形狀就是那個元素」。
///
/// # 只在**兩邊都只有一個**的時候才猜
///
/// 圖上兩個框都叫 `redis`、模型裡兩個元素都叫 `redis`——這時候正確答案是
/// 「不知道」。猜一個給人看，人會直接按下去，而按錯的那一個之後沒有人會再
/// 檢查它。**寧可少猜，不要猜錯**：少猜的代價是多按一次選單，猜錯的代價是
/// 一個沒有人發現的錯誤綁定。
fn guess(shapes: &[UnboundShape], targets: &[Target], edges: bool) -> Vec<Guess> {
    let candidates: Vec<&UnboundShape> = shapes
        .iter()
        .filter(|s| s.edge == edges && !slugify(&s.label).is_empty())
        .collect();

    // 兩輪：先用完全一樣的名字配對，剩下的才用「一個包含另一個」。
    // 反過來的話，`redis` 會先被 `redis-01` 這種局部相符配走，
    // 而真正叫 `redis` 的那個元素反而落空。
    let mut used_shapes: BTreeSet<&str> = BTreeSet::new();
    let mut used_targets: BTreeSet<&Id> = BTreeSet::new();
    let mut out: Vec<Guess> = Vec::new();

    for exact in [true, false] {
        // 這一輪誰配得上誰。只有「這個形狀只配得上這個元素、而這個元素也
        // 只配得上這個形狀」才算數。
        let mut by_shape: BTreeMap<&str, Vec<&Id>> = BTreeMap::new();
        let mut by_target: BTreeMap<&Id, Vec<&str>> = BTreeMap::new();

        for shape in &candidates {
            if used_shapes.contains(shape.cell.as_str()) {
                continue;
            }
            for target in targets {
                if used_targets.contains(&target.id) {
                    continue;
                }
                let hit = if exact {
                    slugify(&shape.label) == slugify(&target.label)
                } else {
                    overlaps(&shape.label, &target.label)
                };
                if hit {
                    by_shape.entry(&shape.cell).or_default().push(&target.id);
                    by_target.entry(&target.id).or_default().push(&shape.cell);
                }
            }
        }

        for (cell, matched) in &by_shape {
            let [target] = matched[..] else { continue };
            if by_target[target].len() != 1 {
                continue;
            }
            used_shapes.insert(cell);
            used_targets.insert(target);
            out.push(Guess {
                cell: (*cell).to_string(),
                target: target.clone(),
            });
        }
    }

    out
}

/// 名字短的那個是不是包在長的那個裡面。
///
/// 「Apache 叢集」對得上 `apache-01`——這是圖上最常見的寫法：人在圖上寫的是
/// 中文說明，模型裡存的是 slug。
///
/// **兩個字以下不算**。`db` 之類的字太容易在別的名字裡出現，配出來的東西
/// 沒有意義，而使用者只會看到一個莫名其妙的建議。
fn overlaps(label: &str, target: &str) -> bool {
    let (a, b) = (slugify(label), slugify(target));
    let short = if a.chars().count() <= b.chars().count() {
        &a
    } else {
        &b
    };
    if short.chars().count() < 2 {
        return false;
    }
    a.contains(b.as_str()) || b.contains(a.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{
        Connection, ContainerInstance, DeploymentNode, Endpointing, InstanceRef, NodeKind,
    };

    fn instance(slug: &str) -> ContainerInstance {
        ContainerInstance {
            id: Id::from(format!("i-{slug}")),
            slug: slug.into(),
            container: Id::from("c"),
            endpoints: vec![],
            standalone: false,
            memo: String::new(),
        }
    }

    /// 一台機器上兩個服務，外加一條連線。
    fn fixture() -> Environment {
        Environment {
            id: Id::from("e"),
            slug: "prod".into(),
            name: "prod".into(),
            nodes: vec![DeploymentNode {
                id: Id::from("n-vm"),
                slug: "vm-01".into(),
                kind: NodeKind::VirtualMachine,
                children: vec![],
                instances: vec![instance("apache-01"), instance("redis-01")],
                memo: String::new(),
            }],
            infra: vec![],
            systems: vec![],
            connections: vec![Connection {
                id: Id::from("conn-1"),
                serves: Id::from("r-1"),
                purpose: "查快取".into(),
                kind: Default::default(),
                from: Endpointing::Instance {
                    target: InstanceRef::One(Id::from("i-apache-01")),
                    endpoint: None,
                },
                to: Endpointing::Instance {
                    target: InstanceRef::One(Id::from("i-redis-01")),
                    endpoint: None,
                },
                memo: String::new(),
            }],
            memo: String::new(),
        }
    }

    fn shape(cell: &str, label: &str) -> UnboundShape {
        UnboundShape {
            cell: cell.into(),
            label: label.into(),
            edge: false,
        }
    }

    fn edge(cell: &str, label: &str) -> UnboundShape {
        UnboundShape {
            edge: true,
            ..shape(cell, label)
        }
    }

    fn ids(targets: &[Target]) -> Vec<&str> {
        targets.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn already_assigned_elements_leave_the_list() {
        let a = annotate(&fixture(), &[Id::from("i-redis-01")], &[]);
        assert_eq!(ids(&a.shapes), vec!["n-vm", "i-apache-01"]);
    }

    #[test]
    fn edges_and_boxes_get_different_targets() {
        // 一個方框指得成一條連線的話，對帳會多出一個永遠對不上的東西。
        let a = annotate(&fixture(), &[], &[]);
        assert_eq!(ids(&a.connections), vec!["conn-1"]);
        assert!(!ids(&a.shapes).contains(&"conn-1"));
    }

    #[test]
    fn an_identical_name_is_guessed() {
        let a = annotate(&fixture(), &[], &[shape("c1", "redis-01")]);
        assert_eq!(
            a.guesses,
            vec![Guess {
                cell: "c1".into(),
                target: Id::from("i-redis-01")
            }]
        );
    }

    #[test]
    fn a_chinese_label_matches_the_slug_inside_it() {
        // 圖上寫中文說明、模型裡存 slug，是最常見的一種對法。
        let a = annotate(&fixture(), &[], &[shape("c1", "Apache-01 叢集")]);
        assert_eq!(a.guesses[0].target, Id::from("i-apache-01"));
    }

    #[test]
    fn two_shapes_wanting_the_same_element_get_no_guess() {
        // 正確答案是「不知道」。猜一個給人看，人會直接按下去。
        let a = annotate(
            &fixture(),
            &[],
            &[shape("c1", "redis-01"), shape("c2", "redis-01")],
        );
        assert!(a.guesses.is_empty());
    }

    #[test]
    fn a_shape_matching_two_elements_gets_no_guess() {
        let env = Environment {
            nodes: vec![DeploymentNode {
                instances: vec![instance("redis-01"), instance("redis-02")],
                ..fixture().nodes[0].clone()
            }],
            ..fixture()
        };
        let a = annotate(&env, &[], &[shape("c1", "redis")]);
        assert!(a.guesses.is_empty());
    }

    #[test]
    fn an_exact_match_wins_over_a_partial_one() {
        // 先跑局部相符的話，`vm-01` 會被 `vm-01 主機` 配走，
        // 而真正叫 `vm-01` 的那個框反而落空。
        let a = annotate(
            &fixture(),
            &[],
            &[shape("c1", "vm-01 主機"), shape("c2", "vm-01")],
        );
        let picked: Vec<&str> = a
            .guesses
            .iter()
            .filter(|g| g.target == Id::from("n-vm"))
            .map(|g| g.cell.as_str())
            .collect();
        assert_eq!(picked, vec!["c2"]);
    }

    #[test]
    fn a_shape_with_no_text_is_never_guessed() {
        // 沒有文字的形狀通常是裝飾。它仍要列出來讓人看得到，但不能亂猜。
        let a = annotate(&fixture(), &[], &[shape("c1", "  ")]);
        assert!(a.guesses.is_empty());
    }

    #[test]
    fn a_two_letter_name_is_too_short_to_match_partially() {
        let a = annotate(&fixture(), &[], &[shape("c1", "a")]);
        assert!(a.guesses.is_empty());
    }

    #[test]
    fn a_line_is_guessed_against_connections_only() {
        // 線上寫「查快取」對得上那條連線的用途；同一段文字不該配到任何方框。
        let a = annotate(&fixture(), &[], &[edge("c1", "查快取")]);
        assert_eq!(a.guesses[0].target, Id::from("conn-1"));
    }

    #[test]
    fn guesses_never_point_at_something_already_assigned() {
        let a = annotate(
            &fixture(),
            &[Id::from("i-redis-01")],
            &[shape("c1", "redis-01")],
        );
        assert!(a.guesses.is_empty());
    }
}
