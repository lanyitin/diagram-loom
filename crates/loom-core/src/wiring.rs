//! 一條實際連線在圖上要畫成幾條線。
//!
//! # 為什麼萬用字元是 N×M 而不是一條
//!
//! `apache-*`（3 台）連 `gateway-*`（3 台）在模型裡是**一條** [`Connection`]，
//! 但它主張的是「每一台 apache 都連得到每一台 gateway」——[`crate::lint`] 的
//! 可達性 BFS 就是這樣建圖的（`edges[from] += 所有 to`）。
//!
//! 所以圖上畫 9 條線不是無中生有，**畫 1 條才是**：那會讓讀圖的人以為
//! 只有一對在通。既然 lint 用 9 條在算，圖就該畫 9 條。
//!
//! # 為什麼這件事在 Rust
//!
//! 「`apache-*` 現在指到哪幾台」要走萬用字元比對加 `within` 的祖先鏈判斷，
//! 那是模型知識不是轉接。放前端就是把 [`crate::index::EnvIndex`] 再寫一遍，
//! 而兩份「什麼叫比對得上」遲早會漂移——漂移的症狀是圖跟 lint 各說各話。
//!
//! # 這裡不判斷缺漏
//!
//! 只回答「現在連到哪些」。少一台、沒填 `expect` 是 [`crate::lint`] 的事。
//! 兩邊都算的話會養出兩套「什麼叫缺」。

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{Connection, ConnectionKind, Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;

/// 圖上的一條線。
///
/// `connection` **會重複**——一條萬用字元連線長出好幾條線。圖上的形狀
/// 靠 `loomId` 唯一，線不靠。對帳那邊在收下之前會依 `connection` 去重。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// 這條線屬於哪條實際連線。
    pub connection: Id,
    /// 起點的形狀 id。
    pub from: Id,
    /// 終點的形狀 id。
    pub to: Id,
    /// 起點是不是「人」。人不住在環境裡，圖上要另外補一個形狀。
    pub from_person: bool,
    /// 備援線在圖上要看得出來——它一樣要建、防火牆一樣要開，
    /// 但把它跟平常的資料流畫成一樣重，圖會很吵而且讀不出主路徑。
    pub kind: ConnectionKind,
    /// 線上的字。空白會被 L007 叫。
    pub purpose: String,
}

/// 展開一個環境裡所有的連線。
pub fn links(project: &Project, environment: &Environment) -> Vec<Link> {
    let index = EnvIndex::build(project, environment);
    let mut out = Vec::new();

    for conn in &environment.connections {
        let from = ends(&index, &conn.from);
        let to = ends(&index, &conn.to);

        for f in &from {
            for t in &to {
                out.push(link(conn, f, t));
            }
        }
    }

    out
}

fn link(conn: &Connection, from: &End, to: &End) -> Link {
    Link {
        connection: conn.id.clone(),
        from: from.id.clone(),
        to: to.id.clone(),
        from_person: from.person,
        kind: conn.kind,
        purpose: conn.purpose.clone(),
    }
}

/// 一端指到的一個形狀。
struct End {
    id: Id,
    person: bool,
}

impl End {
    fn shape(id: &Id) -> Self {
        End {
            id: id.clone(),
            person: false,
        }
    }
}

/// 把一端展開成圖上的點。萬用字元會展開成多個。
///
/// 這裡刻意跟 [`crate::lint`] 的 `resolve` 展開方式一致——不一致的話
/// 「圖上畫得出來」與「lint 說走得通」就會是兩回事。
fn ends(index: &EnvIndex<'_>, side: &Endpointing) -> Vec<End> {
    match side {
        Endpointing::Instance { target, .. } => match target {
            InstanceRef::One(id) => vec![End::shape(id)],
            InstanceRef::Pattern {
                slug_pattern,
                within,
                ..
            } => index
                .matching_within(slug_pattern, within.as_ref())
                .into_iter()
                .map(|i| End::shape(&i.id))
                .collect(),
        },
        Endpointing::Infra { node, .. } => vec![End::shape(node)],
        Endpointing::System { instance, .. } => vec![End::shape(instance)],
        Endpointing::Person { person } => vec![End {
            id: person.clone(),
            person: true,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{ContainerInstance, DeploymentNode, NodeKind};
    use crate::logical::{Container, Logical, Person, Relationship, RelationshipEnd};

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

    fn node(slug: &str, instances: Vec<ContainerInstance>) -> DeploymentNode {
        DeploymentNode {
            id: Id::from(format!("n-{slug}")),
            slug: slug.into(),
            kind: NodeKind::VirtualMachine,
            children: vec![],
            instances,
            memo: String::new(),
        }
    }

    fn pattern(p: &str) -> Endpointing {
        Endpointing::Instance {
            target: InstanceRef::Pattern {
                slug_pattern: p.into(),
                within: None,
                expect: None,
            },
            endpoint: None,
        }
    }

    fn conn(id: &str, from: Endpointing, to: Endpointing) -> Connection {
        Connection {
            id: Id::from(id),
            serves: Id::from("r-1"),
            purpose: "測試".into(),
            kind: ConnectionKind::Primary,
            from,
            to,
            memo: String::new(),
        }
    }

    fn project_with(env: Environment) -> Project {
        Project {
            id: Id::from("p"),
            slug: "p".into(),
            name: "p".into(),
            logical: Logical {
                people: vec![Person {
                    id: Id::from("per-1"),
                    slug: "客戶".into(),
                    name: "客戶".into(),
                    memo: String::new(),
                }],
                systems: vec![],
                containers: vec![Container {
                    id: Id::from("c-app"),
                    slug: "app".into(),
                    name: "app".into(),
                    system: Id::from("s-1"),
                    endpoints: vec![],
                    memo: String::new(),
                }],
                relationships: vec![Relationship {
                    id: Id::from("r-1"),
                    slug: "r-1".into(),
                    purpose: String::new(),
                    from: RelationshipEnd::Container(Id::from("c-app")),
                    to: RelationshipEnd::Container(Id::from("c-app")),
                    to_endpoint: Id::from("ep-1"),
                    memo: String::new(),
                }],
            },
            environments: vec![env],
            memo: String::new(),
        }
    }

    fn env(nodes: Vec<DeploymentNode>, connections: Vec<Connection>) -> Environment {
        Environment {
            id: Id::from("e-1"),
            slug: "prod".into(),
            name: "prod".into(),
            nodes,
            infra: vec![],
            systems: vec![],
            connections,
            memo: String::new(),
        }
    }

    #[test]
    fn a_named_pair_is_one_line() {
        let e = env(
            vec![node("vm", vec![instance("a-01"), instance("b-01")])],
            vec![conn(
                "c-1",
                Endpointing::Instance {
                    target: InstanceRef::One(Id::from("i-a-01")),
                    endpoint: None,
                },
                Endpointing::Instance {
                    target: InstanceRef::One(Id::from("i-b-01")),
                    endpoint: None,
                },
            )],
        );
        let p = project_with(e.clone());
        assert_eq!(links(&p, &e).len(), 1);
    }

    #[test]
    fn a_wildcard_on_both_ends_becomes_every_pair() {
        // 3 台連 3 台是 9 條，不是 1 條。lint 的可達性就是這樣建圖的，
        // 畫 1 條會讓人以為只有一對在通。
        let e = env(
            vec![node(
                "vm",
                vec![
                    instance("apache-01"),
                    instance("apache-02"),
                    instance("apache-03"),
                    instance("gateway-01"),
                    instance("gateway-02"),
                    instance("gateway-03"),
                ],
            )],
            vec![conn("c-1", pattern("apache-*"), pattern("gateway-*"))],
        );
        let p = project_with(e.clone());
        let out = links(&p, &e);
        assert_eq!(out.len(), 9);
        assert!(out.iter().all(|l| l.connection == Id::from("c-1")));
    }

    #[test]
    fn every_line_of_one_connection_is_a_distinct_pair() {
        // 重複的一對會在圖上疊成一條粗線，看起來像少畫了。
        let e = env(
            vec![node(
                "vm",
                vec![instance("a-01"), instance("a-02"), instance("b-01")],
            )],
            vec![conn("c-1", pattern("a-*"), pattern("b-*"))],
        );
        let p = project_with(e.clone());
        let out = links(&p, &e);
        let pairs: std::collections::HashSet<_> = out.iter().map(|l| (&l.from, &l.to)).collect();
        assert_eq!(pairs.len(), out.len());
    }

    #[test]
    fn a_wildcard_that_matches_nothing_draws_nothing() {
        // 沒有線比畫一條指向空氣的線好。缺了幾台是 lint 的事（L004），
        // 在這裡也算一次就會變成兩套說法。
        let e = env(
            vec![node("vm", vec![instance("a-01")])],
            vec![conn("c-1", pattern("a-*"), pattern("nobody-*"))],
        );
        let p = project_with(e.clone());
        assert!(links(&p, &e).is_empty());
    }

    #[test]
    fn within_narrows_which_machines_the_line_reaches() {
        // 站點限定是這個模型防「機器搬家但總數不變」的招式。
        // 圖上如果不跟著限定，搬家後的圖看起來會完全正常。
        let mut main = node("main", vec![instance("redis-01")]);
        main.kind = NodeKind::Site;
        main.children = vec![node("vm-main", vec![instance("app-01")])];

        let mut away = node("away", vec![instance("redis-02")]);
        away.kind = NodeKind::Site;

        let e = env(
            vec![main, away],
            vec![conn(
                "c-1",
                pattern("app-*"),
                Endpointing::Instance {
                    target: InstanceRef::Pattern {
                        slug_pattern: "redis-*".into(),
                        within: Some(Id::from("n-main")),
                        expect: None,
                    },
                    endpoint: None,
                },
            )],
        );
        let p = project_with(e.clone());
        let out = links(&p, &e);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, Id::from("i-redis-01"));
    }

    #[test]
    fn a_person_end_is_flagged_because_people_do_not_live_in_the_environment() {
        // 人沒有實體。圖上不另外補一個形狀的話，這條線會指向不存在的東西。
        let e = env(
            vec![node("vm", vec![instance("a-01")])],
            vec![conn(
                "c-1",
                Endpointing::Person {
                    person: Id::from("per-1"),
                },
                pattern("a-*"),
            )],
        );
        let p = project_with(e.clone());
        let out = links(&p, &e);
        assert_eq!(out.len(), 1);
        assert!(out[0].from_person);
        assert_eq!(out[0].from, Id::from("per-1"));
    }

    #[test]
    fn a_fallback_line_keeps_its_kind() {
        // 備援線一樣要建、防火牆一樣要開，但畫成一樣重的話圖會很吵。
        let mut c = conn(
            "c-1",
            pattern("a-*"),
            Endpointing::Instance {
                target: InstanceRef::One(Id::from("i-b-01")),
                endpoint: None,
            },
        );
        c.kind = ConnectionKind::Fallback;
        let e = env(
            vec![node("vm", vec![instance("a-01"), instance("b-01")])],
            vec![c],
        );
        let p = project_with(e.clone());
        assert_eq!(links(&p, &e)[0].kind, ConnectionKind::Fallback);
    }
}
