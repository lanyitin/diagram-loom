//! 連帶刪除：「刪了這幾個，還有哪些東西會跟著變成廢的」。
//!
//! # 為什麼需要這個
//!
//! [`Edit::DeleteResource`] 刻意不連帶刪除——指著它的東西會變成懸空，
//! 由 L012／L003 叫出來（見 `docs/decisions.md`）。那個決定對「人在畫面上
//! 刪一個東西」是對的：一次消失幾十個元素，而這工具的重點是怕漏。
//!
//! 但它對「要大改結構」的情境是災難。刪一個系統會連帶壞掉三十個元素，
//! 一個一個刪要三十趟，每趟都先跑一次 lint 才知道下一個刪誰。實際發生過的
//! 結果是：Agent 算一算，建議使用者**重開一個新專案**。
//!
//! # 「什麼叫跟著壞掉」不另外定義
//!
//! 就是 lint 的 **L012（參照指到不存在的東西）** 與 **L003（連線指到不存在的東西）**。
//! 這兩條規則本來就是為了當刪除的安全網而寫的，它們的 `subject` 正好就是
//! 「**手上握著壞掉參照的那個元素**」——也就是下一個該刪的。
//!
//! 另外寫一套依賴圖等於維護第二份「什麼叫壞掉」的標準，兩份遲早會不一致，
//! 而不一致的時候使用者信的是先看到的那份。理由跟 [`crate::edit::preview`] 一樣。
//!
//! # 這裡只算，不動手
//!
//! [`plan`] 回的是一串還沒套用的 [`Edit`]。**一定要先給人看過。**
//! 連帶刪除是這個專案裡唯一一個「按一下消失幾十個東西」的操作，
//! 沒有預覽的話它就是那種讓人不敢用的功能。

use std::collections::HashSet;

use crate::Project;
use crate::edit::{self, Edit, EditError};
use crate::id::Id;
use crate::lint::{self, Finding, Rule};
use crate::resource;

/// 一個要被刪掉的東西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub edit: Edit,
    /// 人看的名字，例如「服務 redis」。
    ///
    /// 在這裡就算好，因為算的時候手上有那份專案；等到套用之後
    /// 那個元素已經不在了，只剩一個 UUID 沒辦法變回人話。
    pub what: String,
}

/// [`plan`] 的結果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// 連帶掃出來的，照「先壞先刪」的順序。
    pub cascaded: Vec<Removal>,
    /// 掃到「這裡壞了」但**找不到對應元素可以刪**的。
    ///
    /// 目前已知的一種是服務實體身上的 Endpoint：它不是一個獨立的
    /// [`Resource`](crate::resource::Resource)，得改它的擁有者才修得掉。
    ///
    /// **不吞掉。** 連帶刪完還剩下什麼要人自己處理，是他決定要不要按下去
    /// 的關鍵資訊——安靜地少報一項，他會以為清乾淨了。
    pub unresolved: Vec<Finding>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.cascaded.is_empty() && self.unresolved.is_empty()
    }
}

/// 算出連帶要刪的東西。`seeds` 是使用者指名的那幾個刪除。
///
/// **不會動到 `project`。** 全部跑在複本上。
///
/// 失敗只會發生在 `seeds` 本身就套用不了的時候（例如指到不存在的元素）；
/// 那時候連帶的部分沒有意義，直接把原因傳回去。
pub fn plan(project: &Project, seeds: &[Edit]) -> Result<Plan, EditError> {
    let mut scratch = project.clone();
    for e in seeds {
        edit::apply(&mut scratch, e)?;
    }

    let mut out = Plan::default();
    // 處理過的 subject 不再看第二次。這同時是**終止條件**：
    // 每一輪至少讓一個 subject 進來，而 subject 的總數是有限的。
    let mut seen: HashSet<Id> = HashSet::new();

    loop {
        let broken: Vec<Finding> = lint::lint(&scratch)
            .into_iter()
            .filter(|f| matches!(f.rule, Rule::L003 | Rule::L012))
            .filter(|f| !seen.contains(&f.subject))
            .collect();
        if broken.is_empty() {
            break;
        }

        for f in broken {
            // 同一個元素可能同時壞在好幾個地方（契約的兩端都指到空氣）。
            // 那還是同一個元素，只刪一次。
            if !seen.insert(f.subject.clone()) {
                continue;
            }
            match removal_for(&scratch, &f.subject) {
                Some(r) => {
                    edit::apply(&mut scratch, &r.edit)?;
                    out.cascaded.push(r);
                }
                None => out.unresolved.push(f),
            }
        }
    }

    Ok(out)
}

/// 這個 id 是什麼、要用哪一種 `Edit` 才刪得掉。
///
/// 兩種可能：模型元素走 [`Edit::DeleteResource`]，環境層的連線走
/// [`Edit::DeleteConnection`]——連線不是 `Resource`，所以要分開查。
///
/// 呼叫端拿它把「使用者指名的那幾個 id」變成 `seeds`，這樣指名的與
/// 連帶掃到的走的是同一段程式碼，兩者不會出現「這個刪得掉那個刪不掉」。
pub fn removal(project: &Project, id: &Id) -> Option<Removal> {
    removal_for(project, id)
}

fn removal_for(project: &Project, id: &Id) -> Option<Removal> {
    if let Some(r) = resource::find(project, id) {
        return Some(Removal {
            what: format!("{} {}", r.kind_name(), r.slug()),
            edit: Edit::DeleteResource(r),
        });
    }

    for env in &project.environments {
        if let Some(c) = env.connections.iter().find(|c| &c.id == id) {
            let purpose = if c.purpose.trim().is_empty() {
                "沒填用途".to_string()
            } else {
                c.purpose.trim().to_string()
            };
            return Some(Removal {
                what: format!("{} 的一條連線（{purpose}）", env.slug),
                edit: Edit::DeleteConnection {
                    environment: env.id.clone(),
                    connection: c.id.clone(),
                },
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::*;
    use crate::logical::*;
    use crate::resource::Resource;

    /// 訂單系統：apache 連 redis，prod 裡各一台，一條連線把契約實現掉。
    fn project_of() -> Project {
        let redis_ep = EndpointDef {
            id: Id::new("def-redis"),
            slug: "client-port".into(),
            protocol: Protocol::Tcp,
        };
        Project {
            id: Id::new("p"),
            slug: "shop".into(),
            name: "訂單".into(),
            logical: Logical {
                people: vec![],
                systems: vec![SoftwareSystem {
                    id: Id::new("sys-shop"),
                    slug: "shop".into(),
                    name: "訂單系統".into(),
                    external: false,
                    endpoints: vec![],
                }],
                containers: vec![
                    Container {
                        id: Id::new("c-apache"),
                        slug: "apache".into(),
                        name: "代理".into(),
                        system: Id::new("sys-shop"),
                        endpoints: vec![],
                    },
                    Container {
                        id: Id::new("c-redis"),
                        slug: "redis".into(),
                        name: "快取".into(),
                        system: Id::new("sys-shop"),
                        endpoints: vec![redis_ep.clone()],
                    },
                ],
                relationships: vec![Relationship {
                    id: Id::new("r-1"),
                    slug: "apache-to-redis".into(),
                    purpose: "讀寫快取".into(),
                    from: RelationshipEnd::Container(Id::new("c-apache")),
                    to: RelationshipEnd::Container(Id::new("c-redis")),
                    to_endpoint: Id::new("def-redis"),
                }],
            },
            environments: vec![Environment {
                id: Id::new("env-prod"),
                slug: "prod".into(),
                name: "正式".into(),
                infra: vec![],
                systems: vec![],
                connections: vec![Connection {
                    id: Id::new("conn-1"),
                    serves: Id::new("r-1"),
                    purpose: "讀寫快取".into(),
                    kind: ConnectionKind::Primary,
                    from: Endpointing::Instance {
                        target: InstanceRef::One(Id::new("i-apache")),
                        endpoint: None,
                    },
                    to: Endpointing::Instance {
                        target: InstanceRef::One(Id::new("i-redis")),
                        endpoint: Some(Id::new("def-redis")),
                    },
                }],
                nodes: vec![
                    DeploymentNode {
                        id: Id::new("n-apache"),
                        slug: "vm-apache".into(),
                        kind: NodeKind::VirtualMachine,
                        children: vec![],
                        instances: vec![ContainerInstance {
                            id: Id::new("i-apache"),
                            slug: "apache-01".into(),
                            container: Id::new("c-apache"),
                            standalone: false,
                            endpoints: vec![],
                        }],
                    },
                    DeploymentNode {
                        id: Id::new("n-redis"),
                        slug: "vm-redis".into(),
                        kind: NodeKind::VirtualMachine,
                        children: vec![],
                        instances: vec![ContainerInstance {
                            id: Id::new("i-redis"),
                            slug: "redis-01".into(),
                            container: Id::new("c-redis"),
                            standalone: false,
                            endpoints: vec![Endpoint {
                                id: Id::new("ep-redis"),
                                slug: "client-port".into(),
                                def: Some(Id::new("def-redis")),
                                protocol: Protocol::Tcp,
                                address: Some("10.0.1.11:6379".into()),
                            }],
                        }],
                    },
                ],
            }],
        }
    }

    fn delete(project: &Project, id: &str) -> Edit {
        Edit::DeleteResource(resource::find(project, &Id::new(id)).expect("測試資料裡沒這個"))
    }

    fn names(plan: &Plan) -> Vec<&str> {
        plan.cascaded.iter().map(|r| r.what.as_str()).collect()
    }

    #[test]
    fn deleting_a_container_sweeps_up_everything_that_pointed_at_it() {
        // 這就是「一個一個刪要三十趟」的最小版本：刪掉 redis 這個服務，
        // 契約、連線、服務實體全都變成廢的。
        let p = project_of();
        let plan = plan(&p, &[delete(&p, "c-redis")]).unwrap();

        let swept = names(&plan);
        assert!(swept.contains(&"契約 apache-to-redis"), "{swept:?}");
        assert!(swept.contains(&"服務實體 redis-01"), "{swept:?}");
        assert!(
            swept.iter().any(|s| s.starts_with("prod 的一條連線")),
            "連線指到不存在的契約了，應該一起掃掉：{swept:?}"
        );
    }

    #[test]
    fn the_machine_stays_because_an_empty_machine_is_not_broken() {
        // 只掃「壞掉的」，不掃「空掉的」。一台沒有服務的機器是合法狀態
        // ——他可能正要換上別的服務。掃掉的話那是替使用者做決定。
        let p = project_of();
        let plan = plan(&p, &[delete(&p, "c-redis")]).unwrap();
        assert!(
            !names(&plan).contains(&"機器 vm-redis"),
            "空的機器被誤掃了：{:?}",
            names(&plan)
        );
    }

    #[test]
    fn applying_the_whole_plan_leaves_nothing_broken() {
        // 這是連帶刪除唯一的驗收標準：掃完之後 lint 不該再有懸空參照。
        let p = project_of();
        let seeds = vec![delete(&p, "c-redis")];
        let plan = plan(&p, &seeds).unwrap();

        let mut after = p.clone();
        let all: Vec<Edit> = seeds
            .into_iter()
            .chain(plan.cascaded.iter().map(|r| r.edit.clone()))
            .collect();
        edit::apply(&mut after, &Edit::Batch(all)).unwrap();

        let dangling: Vec<_> = lint::lint(&after)
            .into_iter()
            .filter(|f| matches!(f.rule, Rule::L003 | Rule::L012))
            .collect();
        assert!(dangling.is_empty(), "還有懸空的：{dangling:?}");
        assert!(plan.unresolved.is_empty(), "{:?}", plan.unresolved);
    }

    #[test]
    fn deleting_a_system_reaches_all_the_way_down_to_the_connections() {
        // 系統 → 服務 → 契約 → 連線 → 服務實體。這條鏈就是使用者
        // 「想大改結構」時真正面對的東西。
        let p = project_of();
        let plan = plan(&p, &[delete(&p, "sys-shop")]).unwrap();

        let swept = names(&plan);
        assert!(swept.contains(&"服務 apache"), "{swept:?}");
        assert!(swept.contains(&"服務 redis"), "{swept:?}");
        assert!(swept.contains(&"服務實體 apache-01"), "{swept:?}");
        assert!(swept.contains(&"服務實體 redis-01"), "{swept:?}");
    }

    #[test]
    fn planning_never_touches_the_original() {
        // 這是預覽，不是刪除。算完之後專案要跟算之前一模一樣，
        // 否則「先看過再決定」就是假的。
        let p = project_of();
        let before = p.clone();
        plan(&p, &[delete(&p, "sys-shop")]).unwrap();
        assert_eq!(p, before);
    }

    #[test]
    fn a_clean_delete_cascades_to_nothing() {
        // 沒有東西指著一條連線，所以刪它就只是刪它。
        let p = project_of();
        let plan = plan(
            &p,
            &[Edit::DeleteConnection {
                environment: Id::new("env-prod"),
                connection: Id::new("conn-1"),
            }],
        )
        .unwrap();
        assert!(
            plan.cascaded.is_empty(),
            "不該連帶刪任何東西：{:?}",
            names(&plan)
        );
    }

    #[test]
    fn sweeping_a_connection_does_not_lose_the_promise_it_was_keeping() {
        // 刪掉 apache 那台服務實體，那條連線就指到空氣了，會被掃掉。
        //
        // 這裡要釘住的是**契約留了下來**。連線沒了但契約還在，於是
        // 「prod 還沒實現這條契約」會變成一項 lint——資訊沒有消失，
        // 只是從「一條壞掉的連線」變回「一個待補的缺口」。
        //
        // 若連契約都一起掃掉，這個工具就真的漏了，而且沒有人會發現。
        let p = project_of();
        let seeds = vec![delete(&p, "i-apache")];
        let plan = plan(&p, &seeds).unwrap();

        assert!(
            names(&plan)
                .iter()
                .any(|s| s.starts_with("prod 的一條連線")),
            "指到空氣的連線沒被掃掉：{:?}",
            names(&plan)
        );
        assert!(
            !names(&plan).contains(&"契約 apache-to-redis"),
            "契約被連坐了：{:?}",
            names(&plan)
        );

        let mut after = p.clone();
        let all: Vec<Edit> = seeds
            .into_iter()
            .chain(plan.cascaded.iter().map(|r| r.edit.clone()))
            .collect();
        edit::apply(&mut after, &Edit::Batch(all)).unwrap();
        let complaints = lint::lint(&after);
        assert!(
            complaints.iter().any(|f| f.subject == Id::new("r-1")),
            "契約還在卻沒人抱怨 prod 沒實現它：{complaints:?}"
        );
    }

    #[test]
    fn a_seed_that_does_not_exist_fails_instead_of_returning_an_empty_plan() {
        // 「按了沒反應」是最糟的失敗方式：使用者以為刪掉了。
        let p = project_of();
        let ghost = Edit::DeleteResource(Resource::Container(Container {
            id: Id::new("根本沒這個"),
            slug: "ghost".into(),
            name: "".into(),
            system: Id::new("sys-shop"),
            endpoints: vec![],
        }));
        assert!(plan(&p, &[ghost]).is_err());
    }
}
