//! 新增連線：L001／L002 的修法。
//!
//! # 為什麼這是最後一塊
//!
//! L001（這個環境沒實現這條契約）與 L002（走不通）是 lint 裡**最重要**的兩條，
//! 因為它們正對應「怕漏」。但它們也是唯二沒辦法填一格就修好的——
//! 要新增一條連線，而連線有兩端、每端有好幾種可能。
//!
//! 在這之前，畫面上它們只寫「要改連線」。工具指著問題叫，卻沒給任何辦法，
//! 使用者只能回去改 Excel 再匯一次。
//!
//! # 工具其實知道答案的一大半
//!
//! 「邏輯層是母版，環境層是分身」——L001 的意思就是**母版說這裡該有一條，
//! 而這個環境沒有**。母版上已經寫了誰連誰、連到哪個 Endpoint，
//! 缺的只是「在這個環境對應到哪幾台機器」，那查得出來。
//!
//! 所以流程不是「請你從頭填一張表」，而是 [`propose`] 先擬一份，
//! 使用者看過、必要時用 [`choices`] 換掉某一端，再送出。
//!
//! # 工具**不**知道的那一半
//!
//! 中間要不要經過 F5、走不走備援——那是人的決定，母版上沒有。
//! 所以 [`propose`] 只提直達的那條，並且在 [`Proposal::notes`] 說清楚
//! 它做了什麼假設。**寧可講出來，也不要猜得像真的。**

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::{
    ContainerInstance, Endpointing, Environment, InstanceRef, SoftwareSystemInstance,
};
use crate::id::Id;
use crate::logical::{Relationship, RelationshipEnd};
use crate::table::SideKind;

/// 這個環境裡，連線可以接上去的一個地方。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub kind: SideKind,
    /// 「redis-01 : client-port」
    pub label: String,
    /// 下拉選單分組用：「服務」「服務（整群）」「設備」「外部系統」「人」。
    pub group: String,
    /// 直接可以塞進 [`crate::edit::Edit::AddConnection`] 的東西。
    ///
    /// 前端**把它當不透明值原樣帶回來**，不需要看懂裡面是什麼。
    /// 這是「規則在 Rust」的同一條線：哪些東西接得上、接上去長什麼樣，
    /// 是模型知識，不是畫面知識。
    pub endpointing: Endpointing,
}

/// 照契約擬一條連線。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    /// 先在這裡發好 id，套用時就不必再產一個。
    ///
    /// 這樣 `apply` 是決定性的：預覽算出來的結果跟真正套用的完全一樣，
    /// 復原重做也會重播出同一條連線而不是每次換一個 UUID。
    pub id: Id,
    pub serves: Id,
    pub purpose: String,
    /// 擬不出來時是 `None`（例如來源在這個環境根本沒有實體）。
    pub from: Option<Endpointing>,
    pub to: Option<Endpointing>,
    /// 這份提案做了哪些假設、哪裡擬不出來。**一定要顯示給使用者看。**
    pub notes: Vec<String>,
}

impl Proposal {
    /// 兩端都擬得出來，可以直接送出。
    pub fn is_complete(&self) -> bool {
        self.from.is_some() && self.to.is_some()
    }
}

/// 照契約擬一條「這個環境應該要有」的連線。
pub fn propose(project: &Project, env: &Environment, relationship: &Id) -> Option<Proposal> {
    let rel = project
        .logical
        .relationships
        .iter()
        .find(|r| &r.id == relationship)?;

    let mut notes = Vec::new();
    let from = resolve_end(project, env, &rel.from, None, &mut notes, "來源");
    let to = resolve_end(
        project,
        env,
        &rel.to,
        Some(&rel.to_endpoint),
        &mut notes,
        "目標",
    );

    if from.is_some() && to.is_some() {
        notes.push("擬的是直達的一段。若這條流量其實會經過 F5 之類的設備，請改成兩段。".into());
    }

    Some(Proposal {
        id: Id::generate(),
        serves: rel.id.clone(),
        purpose: purpose_of(rel),
        from,
        to,
        notes,
    })
}

/// 環境層的連線需要自己的用途說明；契約的用途是最好的起點，
/// 直接沿用比留白好——留白只會換來一個 L007。
fn purpose_of(rel: &Relationship) -> String {
    rel.purpose.trim().to_string()
}

fn resolve_end(
    project: &Project,
    env: &Environment,
    end: &RelationshipEnd,
    endpoint: Option<&Id>,
    notes: &mut Vec<String>,
    which_end: &str,
) -> Option<Endpointing> {
    match end {
        RelationshipEnd::Person(person) => Some(Endpointing::Person {
            person: person.clone(),
        }),

        RelationshipEnd::System(system) => {
            let instance_of: Vec<&SoftwareSystemInstance> =
                env.systems.iter().filter(|s| &s.system == system).collect();
            match instance_of.as_slice() {
                [] => {
                    notes.push(format!(
                        "{which_end}的外部系統在 {} 沒有指定位址，要先補上。",
                        env.slug
                    ));
                    None
                }
                [one] => Some(Endpointing::System {
                    instance: one.id.clone(),
                    endpoint: endpoint.cloned(),
                }),
                many => {
                    // 一個外部系統在一個環境就是一個實體，多個代表資料有問題。
                    notes.push(format!(
                        "{which_end}的外部系統在 {} 有 {} 個服務實體，先挑了第一個。",
                        env.slug,
                        many.len()
                    ));
                    Some(Endpointing::System {
                        instance: many[0].id.clone(),
                        endpoint: endpoint.cloned(),
                    })
                }
            }
        }

        RelationshipEnd::Container(container) => {
            let all_of: Vec<&ContainerInstance> = env
                .instances()
                .into_iter()
                .filter(|i| &i.container == container)
                .collect();
            // **只有實現了那個接點的實體才算數。** 同一個服務的實體不一定
            // 每一台都提供每一個接點：Redis 的 master／replica 提供 6379，
            // sentinel 提供 26379，三者都是同一個 `Container` 的實體。
            //
            // 不濾的話，擬出來的萬用字元會把 sentinel 也框進去，而那三台
            // 身上根本沒有 6379——送出去就是三項 L003。工具擬了一份自己
            // 知道會壞的東西，那比擬不出來還糟。
            let instance_of: Vec<&ContainerInstance> = all_of
                .iter()
                .copied()
                .filter(|i| serves(i, endpoint))
                .collect();

            match instance_of.as_slice() {
                [] => {
                    let slug = project
                        .logical
                        .container(container)
                        .map(|c| c.slug.clone())
                        .unwrap_or_else(|| container.to_string());
                    // 「一台都沒建」與「建了但沒有那個接點」要修的地方完全不同，
                    // 講同一句話會把人送去錯的地方。
                    notes.push(match (all_of.is_empty(), endpoint) {
                        (true, _) => format!(
                            "{which_end}的服務 {slug} 在 {} 一台都還沒建，要先建機器。",
                            env.slug
                        ),
                        (false, Some(def)) => format!(
                            "{which_end}的服務 {slug} 在 {} 有 {} 台，但沒有一台實現接點 {}——要先在其中幾台補上這個接點。",
                            env.slug,
                            all_of.len(),
                            endpoint_slug(project, container, def),
                        ),
                        (false, None) => format!(
                            "{which_end}的服務 {slug} 在 {} 一台都還沒建，要先建機器。",
                            env.slug
                        ),
                    });
                    None
                }
                [one] => Some(Endpointing::Instance {
                    target: InstanceRef::One(one.id.clone()),
                    endpoint: endpoint.cloned(),
                }),
                many => Some(Endpointing::Instance {
                    target: as_pattern(project, env, container, many, notes, which_end),
                    endpoint: endpoint.cloned(),
                }),
            }
        }
    }
}

/// 這台實體有沒有實現那個接點定義。
///
/// `endpoint` 是 `None` 時全部都算——來源端可以不指定接點
/// （客戶端的 port 通常由作業系統分配）。
fn serves(instance: &ContainerInstance, endpoint: Option<&Id>) -> bool {
    match endpoint {
        None => true,
        // 比的是 `def`（邏輯層的定義），不是具體 endpoint 的 id——
        // 連線指的一直都是定義，見 `lint::check_instance_has_endpoint`。
        Some(def) => instance
            .endpoints
            .iter()
            .any(|e| e.def.as_ref() == Some(def)),
    }
}

/// 接點定義的顯示名。查不到就退回 id——訊息是給人看的，寧可醜也不要沒有。
fn endpoint_slug(project: &Project, container: &Id, def: &Id) -> String {
    project
        .logical
        .container(container)
        .and_then(|c| c.endpoints.iter().find(|e| &e.id == def))
        .map(|e| e.slug.clone())
        .unwrap_or_else(|| def.to_string())
}

/// 多台就用萬用字元，而不是列出每一台。
///
/// 列出每一台會產生 N 條連線，之後每加一台機器都要記得補一條——
/// 那正是「怕漏」要防的事。萬用字元加 `expect` 才會在少一台時叫。
fn as_pattern(
    project: &Project,
    env: &Environment,
    container: &Id,
    instance_of: &[&ContainerInstance],
    notes: &mut Vec<String>,
    which_end: &str,
) -> InstanceRef {
    let slug_pattern = pattern_for(project, env, container, instance_of);
    let actually_matched = env.instances_matching(&slug_pattern).len();

    // 樣式抓到的若不是這群，就會安靜地把別的機器也算進去。講出來，
    // 而且 `expect` 仍然寫使用者要的數量——讓 lint 去叫，不要自己吞掉。
    if actually_matched != instance_of.len() {
        notes.push(format!(
            "{which_end}的樣式 {slug_pattern} 在 {} 會抓到 {actually_matched} 台，但這個服務只有 {} 台。請改寫樣式。",
            env.slug,
            instance_of.len()
        ));
    }

    InstanceRef::Pattern {
        slug_pattern,
        within: None,
        expect: Some(instance_of.len() as u32),
    }
}

/// 先試「服務名 + `-*`」，抓不準就退回這幾台 slug 的共同前綴。
///
/// # 「抓得到這群」還不夠，要「**只**抓到這群」
///
/// 兩個候選都可能框到不該框的機器。Redis 是最好的例子：`redis-*` 抓得到
/// 三台 sentinel，但 sentinel 沒有 6379。所以兩個候選都先拿去實際比一次，
/// **剛好等於這群的優先**——`redis-sentinel-*` 才是那三台要的樣式。
///
/// 兩個都不剛好時退回第一個抓得到的，並由 [`as_pattern`] 把差距講出來。
/// 硬擠一個「精確」的樣式出來是做不到的（glob 只有 `*`），
/// 而安靜地挑一個會多抓的，就是把問題往後推給 lint。
fn pattern_for(
    project: &Project,
    env: &Environment,
    container: &Id,
    instance_of: &[&ContainerInstance],
) -> String {
    let by_container = project
        .logical
        .container(container)
        .map(|c| format!("{}-*", c.slug));
    let by_prefix = format!("{}*", common_prefix(instance_of));

    let mut fallback = None;
    for candidate in by_container.into_iter().chain([by_prefix.clone()]) {
        if !instance_of
            .iter()
            .all(|i| crate::pattern::matches(&candidate, &i.slug))
        {
            continue;
        }
        if env.instances_matching(&candidate).len() == instance_of.len() {
            return candidate;
        }
        fallback.get_or_insert(candidate);
    }
    fallback.unwrap_or(by_prefix)
}

fn common_prefix(instance_of: &[&ContainerInstance]) -> String {
    let mut prefix: Vec<char> = instance_of
        .first()
        .map(|i| i.slug.chars().collect())
        .unwrap_or_default();
    for i in instance_of.iter().skip(1) {
        let this_one: Vec<char> = i.slug.chars().collect();
        prefix.truncate(
            prefix
                .iter()
                .zip(&this_one)
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    prefix.into_iter().collect()
}

/// 這個環境裡所有接得上的地方，給使用者換掉提案的某一端用。
///
/// # 為什麼來源與目標的清單不一樣
///
/// 來源端的 endpoint 通常由作業系統分配，所以來源多一個「不指定接點」的選項；
/// 而「人」只會是流量的起點，不會出現在目標端。這些是模型規則，
/// 所以由這裡決定，不是前端各自記得。
pub fn choices(
    project: &Project,
    env: &Environment,
    end: crate::environment::ConnectionEnd,
) -> Vec<Choice> {
    use crate::environment::ConnectionEnd;
    let source = end == ConnectionEnd::From;
    let mut out = Vec::new();

    for instance in env.instances() {
        let container_of = project.logical.container(&instance.container);

        if source {
            out.push(Choice {
                kind: SideKind::Instance,
                label: format!("{}（不指定接點）", instance.slug),
                group: "服務".into(),
                endpointing: Endpointing::Instance {
                    target: InstanceRef::One(instance.id.clone()),
                    endpoint: None,
                },
            });
        }

        // 接點列的是**邏輯層的定義**，不是這一台的具體 endpoint——
        // 萬用字元會展開成多台，只有定義才是它們共通的東西。
        //
        // 但只列**這一台真的實現了的**那幾個。同一個服務的實體不保證每一台
        // 都提供每一個接點（Redis 的 sentinel 沒有 6379），列出來的話使用者
        // 挑得到一個一送出就變成 L003 的組合。
        for def in container_of
            .map(|c| c.endpoints.as_slice())
            .unwrap_or_default()
            .iter()
            .filter(|def| serves(instance, Some(&def.id)))
        {
            out.push(Choice {
                kind: SideKind::Instance,
                label: format!("{} : {}", instance.slug, def.slug),
                group: "服務".into(),
                endpointing: Endpointing::Instance {
                    target: InstanceRef::One(instance.id.clone()),
                    endpoint: Some(def.id.clone()),
                },
            });
        }
    }

    // 整群：同一個服務有多台時才有意義。
    //
    // # 「幾台」要一個接點一個接點地算
    //
    // 原本這裡先數「這個服務有幾台」，再把那個數字套到它的每一個接點上。
    // 對 Redis 那種一個 `Container` 底下混著不同角色的服務就是錯的：
    // 5 台 Redis 實體裡只有 master 與 replica 提供 6379，另外三台是
    // sentinel，只提供 26379。於是選單寫著「redis-* : redis-tcp（5 台）」，
    // 而使用者只有 2 台在跑 6379。
    //
    // 挑下去的後果不只是數字難看：那條連線會展開到三台沒有 6379 的機器上，
    // 換來三項 L003。**選單先騙了人，lint 才在後面收拾。**
    for container in &project.logical.containers {
        for def in &container.endpoints {
            let serving: Vec<&ContainerInstance> = env
                .instances()
                .into_iter()
                .filter(|i| i.container == container.id && serves(i, Some(&def.id)))
                .collect();
            if serving.len() < 2 {
                continue;
            }

            let mut notes = Vec::new();
            let target = as_pattern(project, env, &container.id, &serving, &mut notes, "");
            let InstanceRef::Pattern { slug_pattern, .. } = &target else {
                continue;
            };

            // 樣式抓得到的不一定就是這群（`redis-*` 也會抓到 sentinel）。
            // 這種時候**照 `as_pattern` 的規矩講出來，不要安靜地拿掉**：
            // 一個消失的選項跟「工具壞了」長得一樣，而寫在標籤上的數字
            // 使用者當場看得見。`expect` 仍然是他要的數量，剩下的讓 lint 叫。
            let caught = env.instances_matching(slug_pattern).len();
            let label = if caught == serving.len() {
                format!("{slug_pattern} : {}（{} 台）", def.slug, serving.len())
            } else {
                format!(
                    "{slug_pattern} : {}（{} 台，但這個樣式會抓到 {caught} 台）",
                    def.slug,
                    serving.len()
                )
            };

            out.push(Choice {
                kind: SideKind::Instance,
                label,
                group: "服務（整群）".into(),
                endpointing: Endpointing::Instance {
                    target: target.clone(),
                    endpoint: Some(def.id.clone()),
                },
            });
        }
    }

    for node in &env.infra {
        // 設備這端指的是**具體的 Endpoint**，不是邏輯定義——設備不對應
        // 任何邏輯層元素，沒有 def 可指。這個不對稱是刻意的。
        for ep in &node.endpoints {
            out.push(Choice {
                kind: SideKind::Infra,
                label: format!("{} : {}", node.slug, ep.slug),
                group: "設備".into(),
                endpointing: Endpointing::Infra {
                    node: node.id.clone(),
                    endpoint: Some(ep.id.clone()),
                },
            });
        }
    }

    for system in &env.systems {
        let defs = project
            .logical
            .systems
            .iter()
            .find(|s| s.id == system.system)
            .map(|s| s.endpoints.as_slice())
            .unwrap_or_default();
        for def in defs {
            out.push(Choice {
                kind: SideKind::System,
                label: format!("{} : {}", system.slug, def.slug),
                group: "外部系統".into(),
                endpointing: Endpointing::System {
                    instance: system.id.clone(),
                    endpoint: Some(def.id.clone()),
                },
            });
        }
    }

    // 人只會是流量的起點，所以目標端不列。
    if source {
        for person in &project.logical.people {
            out.push(Choice {
                kind: SideKind::Person,
                label: person.slug.clone(),
                group: "人".into(),
                endpointing: Endpointing::Person {
                    person: person.id.clone(),
                },
            });
        }
    }

    out
}
