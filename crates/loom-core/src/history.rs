//! 復原／重做，以及「有沒有未儲存的變更」。
//!
//! # 為什麼存整份快照，而不是存反向操作
//!
//! 反向操作省記憶體，但每加一種 [`Edit`] 就要**再寫一次它的反面**，
//! 而反面寫錯的後果是：復原之後資料看起來正常，其實已經壞了。
//! 對一個用來管理數百條連線的工具，這是最不能接受的失敗方式——
//! 使用者根本不會發現。
//!
//! 快照做不到寫錯：復原就是把整份換回去。代價是記憶體。實測一個
//! 數百條連線的專案序列化後在百 KB 等級，乘上 [`DEPTH`] 步仍在數十 MB 內，
//! 而桌面應用有的是記憶體。**用記憶體換「不可能出錯」是划算的。**
//!
//! # 未儲存狀態為什麼用「跟存檔時的內容比對」
//!
//! 直覺作法是記一個「存檔之後改了幾次」的計數器。但那會答錯一種很常見的情況：
//! 改了一筆、覺得不對、按復原——內容已經跟磁碟上一模一樣了，計數器卻說有兩次變更。
//! 使用者會被逼著存一次不必要的檔。
//!
//! 直接比對內容不會答錯。專案不大，比一次是微秒級，而且只在每次編輯後比一次。

use crate::Project;
use crate::edit::{Edit, EditError};

/// 最多記幾步。超過就丟掉最舊的。
///
/// 100 步遠超過「手滑了想退回去」的需求，而真正要退很遠的時候，
/// 使用者要的其實是重新開檔，不是按 100 次復原。
pub const DEPTH: usize = 100;

/// 一次修改留下的痕跡：改之前的樣子，加上它叫什麼。
struct Step {
    before: Project,
    label: &'static str,
}

/// 專案 + 它的編輯歷史。
///
/// **所有修改都必須走 [`History::edit`]。** 若有人繞過去直接改
/// `project`，歷史就會少一步，而復原會安靜地跳過那次修改。
/// 所以 [`History::project`] 只給唯讀參考。
pub struct History {
    current: Project,
    past: Vec<Step>,
    /// 被復原掉的那些。一有新的修改就清空——分岔的歷史沒有直覺的語意。
    future: Vec<Step>,
    /// 上次存檔時的內容。`None` 表示還沒存過。
    saved: Option<Project>,
}

impl History {
    /// 從磁碟讀進來的專案：一開始就是「已儲存」的狀態。
    pub fn opened(project: Project) -> Self {
        Self {
            saved: Some(project.clone()),
            current: project,
            past: Vec::new(),
            future: Vec::new(),
        }
    }

    pub fn project(&self) -> &Project {
        &self.current
    }

    /// 套用一次修改。**失敗時歷史完全沒被動過**——
    /// 否則使用者會多出一步「什麼都沒發生」的可復原步驟。
    pub fn edit(&mut self, edit: &Edit) -> Result<(), EditError> {
        let mut after = self.current.clone();
        crate::edit::apply(&mut after, edit)?;

        self.past.push(Step {
            before: std::mem::replace(&mut self.current, after),
            label: edit.label(),
        });
        if self.past.len() > DEPTH {
            self.past.remove(0);
        }
        self.future.clear();
        Ok(())
    }

    /// 用一份全新的專案取代目前這份，例如匯入套用後的結果。
    ///
    /// 匯入不是一次 [`Edit`]（它會動到成百上千個地方），但它一樣**必須可以復原**——
    /// 它其實是最需要復原的那個操作。
    pub fn replace(&mut self, project: Project, label: &'static str) {
        self.past.push(Step {
            before: std::mem::replace(&mut self.current, project),
            label,
        });
        if self.past.len() > DEPTH {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.past.pop() else {
            return false;
        };
        self.future.push(Step {
            before: std::mem::replace(&mut self.current, step.before),
            label: step.label,
        });
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.future.pop() else {
            return false;
        };
        self.past.push(Step {
            before: std::mem::replace(&mut self.current, step.before),
            label: step.label,
        });
        true
    }

    /// 記下「現在這份已經存到磁碟了」。
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.current.clone());
    }

    /// 目前內容跟磁碟上的不一樣。關視窗前要問的就是這個。
    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.current)
    }

    /// 復原選單要顯示什麼，例如「復原：刪除連線」。`None` 表示沒得復原。
    pub fn undo_label(&self) -> Option<&'static str> {
        self.past.last().map(|s| s.label)
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.future.last().map(|s| s.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Edit;
    use crate::environment::*;
    use crate::id::Id;
    use crate::logical::Logical;

    fn project_of() -> Project {
        Project {
            id: Id::new("p"),
            slug: "p".into(),
            name: "測試".into(),
            logical: Logical {
                people: vec![],
                systems: vec![],
                containers: vec![],
                relationships: vec![],
            },
            environments: vec![Environment {
                id: Id::new("env-prod"),
                slug: "prod".into(),
                name: "正式".into(),
                nodes: vec![],
                infra: vec![],
                systems: vec![],
                connections: vec![Connection {
                    id: Id::new("conn-1"),
                    serves: Id::new("r-1"),
                    purpose: "原本的用途".into(),
                    kind: ConnectionKind::Primary,
                    from: Endpointing::Person {
                        person: Id::new("who"),
                    },
                    to: Endpointing::Infra {
                        node: Id::new("f5"),
                        endpoint: None,
                    },
                    memo: String::new(),
                }],
                memo: String::new(),
            }],
            memo: String::new(),
        }
    }

    fn set_purpose(purpose: &str) -> Edit {
        Edit::SetPurpose {
            environment: Some(Id::new("env-prod")),
            subject: Id::new("conn-1"),
            purpose: purpose.into(),
        }
    }

    fn purpose_of(h: &History) -> String {
        h.project().environments[0].connections[0].purpose.clone()
    }

    #[test]
    fn redo_lands_back_where_undo_started() {
        let mut h = History::opened(project_of());
        h.edit(&set_purpose("第一次")).unwrap();
        h.edit(&set_purpose("第二次")).unwrap();

        assert_eq!(purpose_of(&h), "第二次");
        assert!(h.undo());
        assert_eq!(purpose_of(&h), "第一次");
        assert!(h.undo());
        assert_eq!(purpose_of(&h), "原本的用途");
        assert!(!h.undo(), "已經到底了還說可以復原");

        assert!(h.redo());
        assert_eq!(purpose_of(&h), "第一次");
        assert!(h.redo());
        assert_eq!(purpose_of(&h), "第二次");
        assert!(!h.redo());
    }

    #[test]
    fn undoing_back_to_the_saved_content_is_not_dirty() {
        // 這是計數器作法會答錯的那個情況：改了、又改回去，
        // 內容明明跟存檔時一模一樣，卻還逼使用者存一次。
        let mut h = History::opened(project_of());
        assert!(!h.is_dirty());

        h.edit(&set_purpose("改一下")).unwrap();
        assert!(h.is_dirty());

        h.undo();
        assert!(
            !h.is_dirty(),
            "內容已經跟磁碟一樣了，不該還說有未儲存的變更"
        );
    }

    #[test]
    fn saving_clears_the_dirty_flag() {
        let mut h = History::opened(project_of());
        h.edit(&set_purpose("改一下")).unwrap();
        h.mark_saved();
        assert!(!h.is_dirty());

        // 存檔不影響復原——存過的東西一樣可以退回去。
        assert!(h.undo());
        assert!(h.is_dirty());
    }

    #[test]
    fn a_failed_edit_leaves_no_empty_step() {
        let mut h = History::opened(project_of());
        let err = h.edit(&Edit::SetPurpose {
            environment: Some(Id::new("env-prod")),
            subject: Id::new("根本沒這條"),
            purpose: "x".into(),
        });

        assert!(err.is_err());
        assert!(!h.is_dirty(), "失敗的修改卻把專案標成未儲存");
        assert_eq!(h.undo_label(), None, "失敗的修改卻留下一個可以復原的步驟");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut h = History::opened(project_of());
        h.edit(&set_purpose("第一次")).unwrap();
        h.undo();
        h.edit(&set_purpose("走另一條路")).unwrap();

        assert_eq!(h.redo_label(), None, "分岔之後還留著舊的重做");
        assert_eq!(purpose_of(&h), "走另一條路");
    }

    #[test]
    fn undo_label_names_the_last_action() {
        let mut h = History::opened(project_of());
        assert_eq!(h.undo_label(), None);

        h.edit(&set_purpose("x")).unwrap();
        assert_eq!(h.undo_label(), Some("修改用途"));

        h.edit(&Edit::DeleteConnection {
            environment: Id::new("env-prod"),
            connection: Id::new("conn-1"),
        })
        .unwrap();
        assert_eq!(h.undo_label(), Some("刪除連線"));

        h.undo();
        assert_eq!(h.redo_label(), Some("刪除連線"));
        assert_eq!(h.undo_label(), Some("修改用途"));
    }

    #[test]
    fn wholesale_replacement_like_import_is_undoable() {
        let mut h = History::opened(project_of());
        let mut is_new = project_of();
        is_new.environments[0].connections.clear();

        h.replace(is_new, "匯入");
        assert!(h.project().environments[0].connections.is_empty());

        assert!(h.undo());
        assert_eq!(h.project().environments[0].connections.len(), 1);
    }

    #[test]
    fn past_the_depth_limit_the_oldest_step_is_dropped() {
        let mut h = History::opened(project_of());
        for i in 0..DEPTH + 10 {
            h.edit(&set_purpose(&format!("第 {i} 次"))).unwrap();
        }

        let mut steps_undone = 0;
        while h.undo() {
            steps_undone += 1;
        }
        assert_eq!(steps_undone, DEPTH);
        // 退到底不會回到最初，因為最舊的幾步已經被丟掉了。
        assert_eq!(purpose_of(&h), "第 9 次");
    }
}
