//! Lint：找出「這個環境還缺什麼」。
//!
//! 這是本工具的靈魂。痛點是數百條連線怕漏掉，lint 就是防漏機制。
//!
//! 規則代號與 `docs/lint-rules.md` 對應。目前實作 L001–L008 與 L012–L014；
//! 只差 L009–L011，那三條與 draw.io 有關，等圖的部分做好再補。
//!
//! **底下的 [`Rule`] 才是唯一的名冊。** 這段話是給人看的摘要，會過期；
//! `mise run check:rules` 認的是那個 enum，程式碼裡別處提到的代號都要在裡面。

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{ConnectionEnd, ContainerInstance, Endpointing, Environment, InstanceRef};
use crate::id::Id;
use crate::index::EnvIndex;
use crate::logical::RelationshipEnd;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// 規則代號。序列化後就是 `"L001"` 這種字串，跟文件一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Rule {
    /// 邏輯層元素在某環境沒有任何實現。
    L001,
    /// 某條 Relationship 在某環境走不通（可達性斷裂）。
    L002,
    /// 連線指向不存在的 Instance、設備或 Endpoint。
    L003,
    /// 萬用字元的實際數量與 `expect` 不符。
    L004,
    /// 萬用字元沒有註明 `expect`。
    L005,
    /// Endpoint 缺少實際位址。
    L006,
    /// 連線沒有填用途。
    L007,
    /// 某 Instance 沒有被任何連線碰到。
    L008,
    /// 模型元素指向一個不存在的模型元素。
    ///
    /// # 為什麼需要一條獨立的規則
    ///
    /// L003 管的是「**連線**指到不存在的東西」。但元素之間也互相指：
    /// 服務實體指著服務、契約指著服務與接點、接點指著接點定義。
    ///
    /// 沒有這條的話，刪掉一個服務會**安靜地**留下一批指著空氣的服務實體——
    /// lint 一句話都不說，而那正是這個工具存在要抓的東西。
    /// 所以它是「可以刪除邏輯層元素」的前置條件，不是加分項。
    ///
    /// 代號接在 draw.io 那三條（L009–L011）後面，是因為那三條先被寫進文件。
    L012,
    /// 契約的**目標端**是人。
    ///
    /// # 為什麼這需要一條規則
    ///
    /// `RelationshipEnd::Person` 的註解寫著「**只該出現在來源端**」——
    /// 「使用者連上系統」是 C4 Context 圖最常見的關係，但反過來沒有意義：
    /// 人沒有接點、沒有位址、也不會被部署。
    ///
    /// 而那句話一直只是註解。在這條規則之前，把人放到目標端是**完全安靜**的：
    ///
    /// - `missing_relationship_end` 只檢查那個人存在，他確實存在
    /// - `target_has_endpoint` 對人直接回 `true`，因為人本來就沒有接點
    /// - 可達性 BFS 把人當成葉節點，走到他就停了，不算斷裂
    ///
    /// 於是模型裡留下一條永遠不可能被實現的契約，而工具一句話都不說。
    /// 對一個賣點是「怕漏」的工具，「說沒問題但東西是錯的」是最糟的狀態。
    L013,
    /// 同一個環境裡，兩個不同種類的東西叫同一個名字。
    ///
    /// # 為什麼這值得叫
    ///
    /// 新增時的唯一性檢查是**分開的**：服務實體只跟服務實體比、設備只跟
    /// 設備比、外部系統實體只跟外部系統實體比。所以一台叫 `pay-01` 的
    /// 服務實體與一台叫 `pay-01` 的設備可以同時存在，兩邊的檢查都會過。
    ///
    /// 但 Agent 是**用名字**指涉東西的（`loom-mcp` 的 `refs`），而它依序找
    /// 服務實體 → 設備 → 外部系統實體，**先找到的贏**。於是「連到 pay-01」
    /// 會安靜地接到服務實體上，而使用者要的是那台設備——連線看起來完全正常。
    ///
    /// `refs` 的註解一直寫著「名字撞在一起本來就是該被 lint 抓的問題」。
    /// 這條就是那個 lint。
    ///
    /// # 為什麼是警告不是錯誤
    ///
    /// 資料本身沒有壞，壞的是「用名字指涉」這一條路。而現實中 VIP 跟它
    /// 服務的那個東西同名是有可能的，用錯誤會變成一個拿不掉的紅字。
    L014,
}

impl Rule {
    pub fn code(self) -> &'static str {
        match self {
            Rule::L001 => "L001",
            Rule::L002 => "L002",
            Rule::L003 => "L003",
            Rule::L004 => "L004",
            Rule::L005 => "L005",
            Rule::L006 => "L006",
            Rule::L007 => "L007",
            Rule::L008 => "L008",
            Rule::L012 => "L012",
            Rule::L013 => "L013",
            Rule::L014 => "L014",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            Rule::L001
            | Rule::L002
            | Rule::L003
            | Rule::L004
            | Rule::L006
            | Rule::L012
            | Rule::L013 => Severity::Error,
            Rule::L005 | Rule::L007 | Rule::L008 | Rule::L014 => Severity::Warning,
        }
    }
}

/// 一項發現。
///
/// 欄位順序即排序順序，讓 lint 的輸出穩定可比對。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule: Rule,
    /// 發生在哪個環境；邏輯層本身的問題為 `None`。
    pub environment: Option<Id>,
    /// 出問題的元素。
    pub subject: Id,
    /// 問題出在連線的哪一端。
    ///
    /// # 為什麼不能省
    ///
    /// 一條連線的兩端都可能是萬用字元（`apigw-* → common-*`），
    /// 於是同一條連線會產生**兩項 rule 與 subject 都相同的 L004**。
    /// 少了這個欄位，「照著發現去修」就只能猜是哪一端——
    /// 而猜錯會安靜地改到另一端，使用者按了套用卻什麼都沒發生。
    ///
    /// 跟連線的端點無關的規則（L001／L006／L007／L008）是 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<ConnectionEnd>,
    pub detail: String,
}

impl Finding {
    pub fn severity(&self) -> Severity {
        self.rule.severity()
    }
}

/// 連線圖上的一個點。可達性檢查就在這種點之間走。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GraphNode {
    Instance(Id),
    Infra(Id),
    System(Id),
    /// 真人。只會是路徑的起點，沒有下一跳。
    Person(Id),
}

/// 檢查整個專案，回傳排序後的發現清單。
///
/// 回傳結果**已排序且去重**，因此可以直接整份比對，
/// 多冒出一項非預期的發現就會被測試抓到。
pub fn lint(project: &Project) -> Vec<Finding> {
    let mut findings = Vec::new();

    for rel in &project.logical.relationships {
        if rel.purpose.trim().is_empty() {
            findings.push(Finding {
                rule: Rule::L007,
                environment: None,
                subject: rel.id.clone(),
                end: None,
                detail: format!("邏輯連線 {} 沒有填用途", rel.slug),
            });
        }
    }

    check_logical_references(project, &mut findings);

    for env in &project.environments {
        lint_environment(project, env, &mut findings);
    }

    findings.sort();
    findings.dedup();
    findings
}

fn lint_environment(project: &Project, env: &Environment, findings: &mut Vec<Finding>) {
    let index = EnvIndex::build(project, env);

    check_logical_realized(project, env, &index.all, findings);
    check_connections(env, &index, findings);
    check_endpoint_addresses(env, &index, findings);
    check_relationships_reachable(project, env, &index, findings);
    check_orphan_instances(env, &index, findings);
    check_environment_references(project, env, findings);
    check_name_collisions(env, findings);
}

/// L014：同一個環境裡兩個不同種類的東西叫同一個名字。
///
/// # 順序就是 `loom-mcp` 解析名字的順序
///
/// 服務實體 → 設備 → 外部系統實體，**先找到的贏**。所以報在**輸的那個**
/// 身上：贏的那個用名字還指得到，輸的那個指不到——需要改名的是它。
///
/// 名字相同但**同一種**的情況不會走到這裡：那在新增時就被
/// `resource::check_slug_unique` 擋掉了。
fn check_name_collisions(env: &Environment, findings: &mut Vec<Finding>) {
    // (名字, 種類, id)，照解析順序排。
    let mut seen: std::collections::HashMap<&str, &'static str> = std::collections::HashMap::new();
    let ordered = env
        .instances()
        .into_iter()
        .map(|i| (i.slug.as_str(), "服務實體", i.id.clone()))
        .chain(
            env.infra
                .iter()
                .map(|n| (n.slug.as_str(), "設備", n.id.clone())),
        )
        .chain(
            env.systems
                .iter()
                .map(|s| (s.slug.as_str(), "外部系統實體", s.id.clone())),
        )
        .collect::<Vec<_>>();

    for (slug, kind, id) in ordered {
        match seen.get(slug) {
            None => {
                seen.insert(slug, kind);
            }
            Some(winner) => findings.push(Finding {
                rule: Rule::L014,
                environment: Some(env.id.clone()),
                subject: id,
                end: None,
                detail: format!(
                    "環境 {} 裡有兩個東西叫 {slug}：這個{kind}，以及一個{winner}。\
                     用名字指涉時會指到那個{winner}，這個{kind}指不到——改一個名字。",
                    env.slug
                ),
            }),
        }
    }
}

/// L012（邏輯層）：契約與服務指到的東西要真的存在。
///
/// 這一整組是「刪除」的安全網。使用者刪掉一個服務之後，指著它的東西
/// 不會自己消失——但至少要**叫出來**，而不是安靜地留在專案裡。
fn check_logical_references(project: &Project, findings: &mut Vec<Finding>) {
    let logical = &project.logical;

    for c in &logical.containers {
        if !logical.systems.iter().any(|s| s.id == c.system) {
            findings.push(Finding {
                rule: Rule::L012,
                environment: None,
                subject: c.id.clone(),
                end: None,
                detail: format!("服務 {} 屬於一個不存在的系統 {}", c.slug, c.system),
            });
        }
    }

    for rel in &logical.relationships {
        for (end, which) in [
            (&rel.from, ConnectionEnd::From),
            (&rel.to, ConnectionEnd::To),
        ] {
            if let Some(missing) = missing_relationship_end(logical, end) {
                findings.push(Finding {
                    rule: Rule::L012,
                    environment: None,
                    subject: rel.id.clone(),
                    end: Some(which),
                    detail: format!("契約 {} 的{which}端指向不存在的{missing}", rel.slug),
                });
            }
        }

        // L013：人只能當來源。
        //
        // 放在 `to_endpoint` 那條前面，因為它是更根本的錯——目標是人的時候，
        // 「目標身上有沒有那個接點」根本是個沒有意義的問題。
        if let RelationshipEnd::Person(person) = &rel.to {
            findings.push(Finding {
                rule: Rule::L013,
                environment: None,
                subject: rel.id.clone(),
                end: Some(ConnectionEnd::To),
                detail: format!(
                    "契約 {} 的目標端是人（{}）。人沒有接點也不會被部署，\
                     只能當來源——把兩端對調，或改成連到對方的服務。",
                    rel.slug,
                    logical
                        .people
                        .iter()
                        .find(|p| &p.id == person)
                        .map_or(person.to_string(), |p| p.slug.clone()),
                ),
            });
        }

        // `to_endpoint` 必須是**目標那一端身上**的接點定義。指到別人身上的
        // 一樣算壞掉——連線展開時會找不到對應的實際 endpoint。
        if !target_has_endpoint(logical, rel) {
            findings.push(Finding {
                rule: Rule::L012,
                environment: None,
                subject: rel.id.clone(),
                end: Some(ConnectionEnd::To),
                detail: format!(
                    "契約 {} 的目標身上沒有接點定義 {}",
                    rel.slug, rel.to_endpoint
                ),
            });
        }
    }
}

fn missing_relationship_end(
    logical: &crate::logical::Logical,
    end: &RelationshipEnd,
) -> Option<String> {
    match end {
        RelationshipEnd::Container(id) => {
            (!logical.containers.iter().any(|c| &c.id == id)).then(|| format!("服務 {id}"))
        }
        RelationshipEnd::System(id) => {
            (!logical.systems.iter().any(|s| &s.id == id)).then(|| format!("系統 {id}"))
        }
        RelationshipEnd::Person(id) => {
            (!logical.people.iter().any(|p| &p.id == id)).then(|| format!("人 {id}"))
        }
    }
}

fn target_has_endpoint(
    logical: &crate::logical::Logical,
    rel: &crate::logical::Relationship,
) -> bool {
    match &rel.to {
        RelationshipEnd::Container(id) => logical
            .containers
            .iter()
            .find(|c| &c.id == id)
            // 服務本身就不存在的話，已經有另一項發現在講了，這裡不重複叫。
            .is_none_or(|c| c.endpoints.iter().any(|e| e.id == rel.to_endpoint)),
        RelationshipEnd::System(id) => logical
            .systems
            .iter()
            .find(|s| &s.id == id)
            .is_none_or(|s| s.endpoints.iter().any(|e| e.id == rel.to_endpoint)),
        // 人沒有接點。那是模型層級的錯，由 L013 負責叫——這裡不重複。
        RelationshipEnd::Person(_) => true,
    }
}

/// L012（環境層）：服務實體與接點指到的邏輯層元素要真的存在。
fn check_environment_references(project: &Project, env: &Environment, findings: &mut Vec<Finding>) {
    let logical = &project.logical;

    for instance in env.instances() {
        let container_of = logical.container(&instance.container);
        if container_of.is_none() {
            findings.push(Finding {
                rule: Rule::L012,
                environment: Some(env.id.clone()),
                subject: instance.id.clone(),
                end: None,
                detail: format!(
                    "服務實體 {} 指向不存在的服務 {}",
                    instance.slug, instance.container
                ),
            });
        }
        for ep in &instance.endpoints {
            check_endpoint_def(
                ep,
                container_of.map(|c| c.endpoints.as_slice()),
                &instance.slug,
                env,
                findings,
            );
        }
    }

    for si in &env.systems {
        let system_of = logical.systems.iter().find(|s| s.id == si.system);
        if system_of.is_none() {
            findings.push(Finding {
                rule: Rule::L012,
                environment: Some(env.id.clone()),
                subject: si.id.clone(),
                end: None,
                detail: format!("服務實體 {} 指向不存在的外部系統 {}", si.slug, si.system),
            });
        }
        for ep in &si.endpoints {
            check_endpoint_def(
                ep,
                system_of.map(|s| s.endpoints.as_slice()),
                &si.slug,
                env,
                findings,
            );
        }
    }
}

/// 設備的 endpoint 沒有 `def`（設備不對應任何邏輯層元素），所以只查有填的。
fn check_endpoint_def(
    ep: &crate::environment::Endpoint,
    defs: Option<&[crate::logical::EndpointDef]>,
    owner: &str,
    env: &Environment,
    findings: &mut Vec<Finding>,
) {
    let Some(def) = &ep.def else { return };
    // 擁有者本身就不存在的話已經報過了，不重複叫。
    let Some(defs) = defs else { return };

    if !defs.iter().any(|d| &d.id == def) {
        findings.push(Finding {
            rule: Rule::L012,
            environment: Some(env.id.clone()),
            subject: ep.id.clone(),
            end: None,
            detail: format!("{owner}／{} 指向不存在的接點定義 {def}", ep.slug),
        });
    }
}

/// L001：邏輯層的東西在每個環境都要服務實體。
///
/// - 每個 Container 至少要有一個 ContainerInstance
/// - 每個**外部** SoftwareSystem 至少要有一個 SoftwareSystemInstance
///   （自家系統靠自己的 Container 服務實體，不另外檢查）
///
/// 判定是「至少一個」而非「數量相同」：prod 12 台、test 6 台本來就正常，
/// 數量的把關交給 `expect`（L004）。
fn check_logical_realized(
    project: &Project,
    env: &Environment,
    instances: &[&ContainerInstance],
    findings: &mut Vec<Finding>,
) {
    for container in &project.logical.containers {
        if !instances.iter().any(|i| i.container == container.id) {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: container.id.clone(),
                end: None,
                detail: format!(
                    "服務 {} 在環境 {} 沒有任何 Instance",
                    container.slug, env.slug
                ),
            });
        }
    }

    for system in project.logical.external_systems() {
        if !env.systems.iter().any(|s| s.system == system.id) {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: system.id.clone(),
                end: None,
                detail: format!("外部系統 {} 在環境 {} 沒有指定位址", system.slug, env.slug),
            });
        }
    }
}

/// L003 / L004 / L005 / L007：逐條連線檢查。
fn check_connections(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    for conn in &env.connections {
        if conn.purpose.trim().is_empty() {
            findings.push(Finding {
                rule: Rule::L007,
                environment: Some(env.id.clone()),
                subject: conn.id.clone(),
                end: None,
                detail: "連線沒有填用途".into(),
            });
        }

        if !index.knows_relationship(&conn.serves) {
            findings.push(Finding {
                rule: Rule::L003,
                environment: Some(env.id.clone()),
                subject: conn.id.clone(),
                end: None,
                detail: format!("連線指向不存在的邏輯連線 {}", conn.serves),
            });
        }

        for (end, side) in [
            (ConnectionEnd::From, &conn.from),
            (ConnectionEnd::To, &conn.to),
        ] {
            // 人住在邏輯層，`check_endpointing` 手上只有環境，所以在這裡查。
            if let Endpointing::Person { person } = side
                && !index.knows_person(person)
            {
                findings.push(Finding {
                    rule: Rule::L003,
                    environment: Some(env.id.clone()),
                    subject: conn.id.clone(),
                    end: Some(end),
                    detail: format!("連線指向不存在的 Person {person}"),
                });
            }

            check_endpointing(env, index, conn.id.clone(), end, side, findings);
        }
    }
}

/// 一端的檢查。`end` 只是原樣帶進每一項發現裡——兩端都可能是萬用字元，
/// 不指名的話「照著發現去修」就得用猜的。
fn check_endpointing(
    env: &Environment,
    index: &EnvIndex<'_>,
    conn_id: Id,
    end: ConnectionEnd,
    side: &Endpointing,
    findings: &mut Vec<Finding>,
) {
    let env_id = Some(env.id.clone());
    let end = Some(end);

    match side {
        Endpointing::Instance { target, endpoint } => match target {
            InstanceRef::One(id) => match index.instance(id) {
                None => findings.push(Finding {
                    rule: Rule::L003,
                    environment: env_id,
                    subject: conn_id,
                    end,
                    detail: format!("連線指向不存在的 Instance {id}"),
                }),
                Some(found) => {
                    check_instance_has_endpoint(
                        found,
                        endpoint.as_ref(),
                        env,
                        conn_id,
                        end,
                        findings,
                    );
                }
            },
            InstanceRef::Pattern {
                slug_pattern,
                within,
                expect,
            } => {
                // 限定範圍卻指向不存在的節點——這比數量不對更嚴重，
                // 因為它會讓 expect 永遠是 0，看起來像「一台都沒建」。
                if let Some(node) = within
                    && !index.knows_node(node)
                {
                    findings.push(Finding {
                        rule: Rule::L003,
                        environment: env_id.clone(),
                        subject: conn_id.clone(),
                        end,
                        detail: format!("within 指向不存在的部署節點 {node}"),
                    });
                }

                let matched = index.matching_within(slug_pattern, within.as_ref());

                match expect {
                    None => findings.push(Finding {
                        rule: Rule::L005,
                        environment: env_id.clone(),
                        subject: conn_id.clone(),
                        end,
                        detail: format!("萬用字元 {slug_pattern} 沒有註明 expect 期望數量"),
                    }),
                    Some(want) if *want as usize != matched.len() => findings.push(Finding {
                        rule: Rule::L004,
                        environment: env_id.clone(),
                        subject: conn_id.clone(),
                        end,
                        detail: match within {
                            Some(node) => format!(
                                "萬用字元 {slug_pattern}（限定在 {node} 底下）期望 {want} 個，實際符合 {} 個",
                                matched.len()
                            ),
                            None => format!(
                                "萬用字元 {slug_pattern} 期望 {want} 個，實際符合 {} 個",
                                matched.len()
                            ),
                        },
                    }),
                    Some(_) => {}
                }

                for found in matched {
                    check_instance_has_endpoint(
                        found,
                        endpoint.as_ref(),
                        env,
                        conn_id.clone(),
                        end,
                        findings,
                    );
                }
            }
        },
        Endpointing::Infra { node, endpoint } => match env.infra_node(node) {
            None => findings.push(Finding {
                rule: Rule::L003,
                environment: env_id,
                subject: conn_id,
                end,
                detail: format!("連線指向不存在的設備 {node}"),
            }),
            Some(found) => {
                if let Some(want) = endpoint
                    && !found.endpoints.iter().any(|e| &e.id == want)
                {
                    findings.push(Finding {
                        rule: Rule::L003,
                        environment: env_id,
                        subject: conn_id,
                        end,
                        detail: format!("設備 {} 上沒有 Endpoint {want}", found.slug),
                    });
                }
            }
        },
        // 人沒有實體也沒有位址，能檢查的只有「這個 Person 真的存在」——
        // 那要看邏輯層，`check_endpointing` 手上只有環境，所以放在
        // `check_connections` 裡做。這裡什麼都不用查。
        Endpointing::Person { .. } => {}
        Endpointing::System { instance, endpoint } => match env.system_instance(instance) {
            None => findings.push(Finding {
                rule: Rule::L003,
                environment: env_id,
                subject: conn_id,
                end,
                detail: format!("連線指向不存在的外部系統實體 {instance}"),
            }),
            Some(found) => {
                if let Some(want) = endpoint
                    && !found.endpoints.iter().any(|e| e.def.as_ref() == Some(want))
                {
                    findings.push(Finding {
                        rule: Rule::L003,
                        environment: env_id,
                        subject: conn_id,
                        end,
                        detail: format!(
                            "外部系統 {} 上沒有對應 EndpointDef {want} 的 Endpoint",
                            found.slug
                        ),
                    });
                }
            }
        },
    }
}

fn check_instance_has_endpoint(
    instance: &ContainerInstance,
    endpoint: Option<&Id>,
    env: &Environment,
    conn_id: Id,
    end: Option<ConnectionEnd>,
    findings: &mut Vec<Finding>,
) {
    // 來源端的 endpoint 可以是 None（由作業系統分配 ephemeral port）。
    let Some(want) = endpoint else { return };

    // `want` 指的是邏輯層的 EndpointDef，不是具體 Endpoint 的 id：
    // 萬用字元會展開成多台 Instance，各有各的具體 endpoint。
    if !instance
        .endpoints
        .iter()
        .any(|e| e.def.as_ref() == Some(want))
    {
        findings.push(Finding {
            rule: Rule::L003,
            environment: Some(env.id.clone()),
            subject: conn_id,
            end,
            detail: format!(
                "Instance {} 上沒有對應 EndpointDef {want} 的 Endpoint",
                instance.slug
            ),
        });
    }
}

/// L006：每個 Endpoint 都要有實際位址。
fn check_endpoint_addresses(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    for instance in &index.all {
        for endpoint in &instance.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    end: None,
                    detail: format!("{}／{} 缺少位址", instance.slug, endpoint.slug),
                });
            }
        }
    }

    for node in &env.infra {
        for endpoint in &node.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    end: None,
                    detail: format!("{}／{} 缺少位址", node.slug, endpoint.slug),
                });
            }
        }
    }

    for system in &env.systems {
        for endpoint in &system.endpoints {
            if endpoint.address.is_none() {
                findings.push(Finding {
                    rule: Rule::L006,
                    environment: Some(env.id.clone()),
                    subject: endpoint.id.clone(),
                    end: None,
                    detail: format!("{}／{} 缺少位址", system.slug, endpoint.slug),
                });
            }
        }
    }
}

/// L001 / L002：每條邏輯連線在每個環境都要有實現，而且要走得通。
///
/// 走迷宮：把貼同一個 `serves` 標籤的連線攤開成一張圖，
/// 從來源服務的任一 Instance 出發，看能不能走到目標服務的任一 Instance。
/// 這順便會抓到「F5 設了但後面忘了接」。
fn check_relationships_reachable(
    project: &Project,
    env: &Environment,
    index: &EnvIndex<'_>,
    findings: &mut Vec<Finding>,
) {
    for rel in &project.logical.relationships {
        let serving = index.serving(&rel.id);

        if serving.is_empty() {
            findings.push(Finding {
                rule: Rule::L001,
                environment: Some(env.id.clone()),
                subject: rel.id.clone(),
                end: None,
                detail: format!("邏輯連線 {} 在環境 {} 沒有任何實際連線", rel.slug, env.slug),
            });
            continue;
        }

        let starts = ends_to_nodes(env, &index.all, &rel.from);
        let goals: HashSet<GraphNode> = ends_to_nodes(env, &index.all, &rel.to)
            .into_iter()
            .collect();

        // 兩端的服務本身就沒實現時，L001 已經報過了，不再重複報 L002。
        if starts.is_empty() || goals.is_empty() {
            continue;
        }

        let mut edges: HashMap<GraphNode, Vec<GraphNode>> = HashMap::new();
        for conn in serving {
            let to = resolve(index, &conn.to);
            for from in resolve(index, &conn.from) {
                edges.entry(from).or_default().extend(to.iter().cloned());
            }
        }

        if !reachable(&edges, &starts, &goals) {
            findings.push(Finding {
                rule: Rule::L002,
                environment: Some(env.id.clone()),
                subject: rel.id.clone(),
                end: None,
                detail: format!(
                    "邏輯連線 {} 在環境 {} 走不通：從來源出發到不了目標",
                    rel.slug, env.slug
                ),
            });
        }
    }
}

/// 把**邏輯連線的一端**展開成圖上的點：該服務／外部系統在此環境的所有服務實體。
fn ends_to_nodes(
    env: &Environment,
    instances: &[&ContainerInstance],
    end: &RelationshipEnd,
) -> Vec<GraphNode> {
    match end {
        RelationshipEnd::Container(cid) => instances
            .iter()
            .filter(|i| &i.container == cid)
            .map(|i| GraphNode::Instance(i.id.clone()))
            .collect(),
        RelationshipEnd::System(sid) => env
            .systems
            .iter()
            .filter(|s| &s.system == sid)
            .map(|s| GraphNode::System(s.id.clone()))
            .collect(),
        // 人不需要服務實體，所以不必去環境裡找——它本身就是圖上的一個點。
        RelationshipEnd::Person(pid) => vec![GraphNode::Person(pid.clone())],
    }
}

/// 把**實際連線的一端**展開成圖上的點。萬用字元會展開成多個點。
fn resolve(index: &EnvIndex<'_>, side: &Endpointing) -> Vec<GraphNode> {
    match side {
        Endpointing::Instance { target, .. } => match target {
            InstanceRef::One(id) => vec![GraphNode::Instance(id.clone())],
            InstanceRef::Pattern {
                slug_pattern,
                within,
                ..
            } => index
                .matching_within(slug_pattern, within.as_ref())
                .into_iter()
                .map(|i| GraphNode::Instance(i.id.clone()))
                .collect(),
        },
        Endpointing::Infra { node, .. } => vec![GraphNode::Infra(node.clone())],
        Endpointing::System { instance, .. } => vec![GraphNode::System(instance.clone())],
        Endpointing::Person { person } => vec![GraphNode::Person(person.clone())],
    }
}

/// 廣度優先搜尋：從任一起點出發，能否抵達任一終點。
fn reachable(
    edges: &HashMap<GraphNode, Vec<GraphNode>>,
    starts: &[GraphNode],
    goals: &HashSet<GraphNode>,
) -> bool {
    let mut seen: HashSet<&GraphNode> = HashSet::new();
    let mut queue: VecDeque<&GraphNode> = VecDeque::new();

    for start in starts {
        if goals.contains(start) {
            return true;
        }
        if seen.insert(start) {
            queue.push_back(start);
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(next_nodes) = edges.get(current) else {
            continue;
        };
        for next in next_nodes {
            if goals.contains(next) {
                return true;
            }
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }

    false
}

/// L008：沒有被任何連線碰到的 Instance。
///
/// 冷備機是合法情境，所以可以在該 Instance 標記 `standalone` 關掉這個警告。
fn check_orphan_instances(env: &Environment, index: &EnvIndex<'_>, findings: &mut Vec<Finding>) {
    let mut touched: HashSet<Id> = HashSet::new();
    for conn in &env.connections {
        for side in [&conn.from, &conn.to] {
            for node in resolve(index, side) {
                match node {
                    GraphNode::Instance(id) | GraphNode::System(id) => {
                        touched.insert(id);
                    }
                    GraphNode::Infra(_) | GraphNode::Person(_) => {}
                }
            }
        }
    }

    for instance in &index.all {
        if instance.standalone || touched.contains(&instance.id) {
            continue;
        }
        findings.push(Finding {
            rule: Rule::L008,
            environment: Some(env.id.clone()),
            subject: instance.id.clone(),
            end: None,
            detail: format!("Instance {} 沒有被任何連線碰到", instance.slug),
        });
    }

    for system in &env.systems {
        if system.standalone || touched.contains(&system.id) {
            continue;
        }
        findings.push(Finding {
            rule: Rule::L008,
            environment: Some(env.id.clone()),
            subject: system.id.clone(),
            end: None,
            detail: format!("外部系統 {} 沒有被任何連線碰到", system.slug),
        });
    }
}
