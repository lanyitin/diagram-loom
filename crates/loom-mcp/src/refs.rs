//! Agent 用的簡寫參照。
//!
//! # 為什麼不讓 Agent 直接給 UUID
//!
//! Agent 手上的原始資料長這樣：
//!
//! > 「三台 Apache 反向代理接到 Gateway，中間走 F5」
//!
//! 裡面沒有 UUID，只有名字。要求它先查一次 id 再填，等於每建一條連線
//! 都要多兩次來回，而且**它會抄錯** UUID——那種錯誤看起來像亂碼，很難查。
//!
//! 所以參照一律用名字寫，解析放在 Rust。查不到就報錯並列出附近的候選，
//! 讓 Agent 自己修正。
//!
//! # 語法
//!
//! | 寫法 | 意思 |
//! | --- | --- |
//! | `redis-01` | 那一台落地 |
//! | `redis-*` | 一整群（會自動帶上 `expect`） |
//! | `redis-* @ dc-main` | 限定在某個站點底下的那一群 |
//! | `f5-01 : vip-redis` | 設備上的某個 VIP |
//! | `person:customer` | 人（只能當來源） |
//! | `system:payment` | 外部系統的落地 |
//!
//! 沒有前綴時依序找落地、設備、外部系統落地——名字撞在一起本來就是
//! 該被 lint 抓的問題，這裡不替它遮掩。

use loom_core::Project;
use loom_core::environment::{Endpointing, Environment, InstanceRef};
use loom_core::id::Id;

/// 解析一個參照。
///
/// `endpoint_def` 是契約指定的接點定義；落地與外部系統那端會用到它，
/// 設備不用（設備指的是具體的 VIP，不是邏輯定義）。
pub fn resolve(
    project: &Project,
    env: &Environment,
    text: &str,
    endpoint_def: Option<&Id>,
) -> Result<Endpointing, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("參照是空的".into());
    }

    if let Some(slug) = text.strip_prefix("person:") {
        let slug = slug.trim();
        return project
            .logical
            .people
            .iter()
            .find(|p| p.slug == slug)
            .map(|p| Endpointing::Person {
                person: p.id.clone(),
            })
            .ok_or_else(|| {
                nearby(
                    "找不到人",
                    slug,
                    project.logical.people.iter().map(|p| &p.slug),
                )
            });
    }

    if let Some(slug) = text.strip_prefix("system:") {
        let slug = slug.trim();
        return env
            .systems
            .iter()
            .find(|s| s.slug == slug)
            .map(|s| Endpointing::System {
                instance: s.id.clone(),
                endpoint: endpoint_def.cloned(),
            })
            .ok_or_else(|| {
                nearby(
                    "這個環境裡找不到外部系統落地",
                    slug,
                    env.systems.iter().map(|s| &s.slug),
                )
            });
    }

    // `f5-01 : vip-redis` —— 設備加上它身上的某個 VIP。
    if let Some((node, ep)) = text.split_once(':') {
        let (node, ep) = (node.trim(), ep.trim());
        let found = env
            .infra
            .iter()
            .find(|n| n.slug == node)
            .ok_or_else(|| nearby("找不到設備", node, env.infra.iter().map(|n| &n.slug)))?;
        let endpoint = found
            .endpoints
            .iter()
            .find(|e| e.slug == ep)
            .ok_or_else(|| {
                nearby(
                    &format!("設備 {node} 上找不到"),
                    ep,
                    found.endpoints.iter().map(|e| &e.slug),
                )
            })?;
        return Ok(Endpointing::Infra {
            node: found.id.clone(),
            endpoint: Some(endpoint.id.clone()),
        });
    }

    // `redis-* @ dc-main` —— 限定在某個部署節點底下。
    let (pattern, within) = match text.split_once('@') {
        Some((p, site)) => {
            let site = site.trim();
            let node = find_node(&env.nodes, site).ok_or_else(|| {
                nearby("找不到節點", site, node_slugs(&env.nodes).iter().copied())
            })?;
            (p.trim(), Some(node))
        }
        None => (text, None),
    };

    if pattern.contains('*') {
        let matched = matching(env, pattern, within.as_ref());
        if matched == 0 {
            return Err(format!(
                "樣式 {pattern} 在 {} 一台都沒對到。已經有的落地：{}",
                env.slug,
                list(env.instances().iter().map(|i| &i.slug))
            ));
        }
        return Ok(Endpointing::Instance {
            target: InstanceRef::Pattern {
                slug_pattern: pattern.to_string(),
                within,
                // 萬用字元一定要帶 expect，否則少一台機器不會有人叫——
                // 那正是這個工具存在要抓的東西（L005）。
                expect: Some(matched),
            },
            endpoint: endpoint_def.cloned(),
        });
    }

    if let Some(instance) = env.instances().into_iter().find(|i| i.slug == pattern) {
        return Ok(Endpointing::Instance {
            target: InstanceRef::One(instance.id.clone()),
            endpoint: endpoint_def.cloned(),
        });
    }
    // 設備與外部系統落地不加前綴也認得——名字通常已經夠獨特了。
    if let Some(node) = env.infra.iter().find(|n| n.slug == pattern) {
        return Ok(Endpointing::Infra {
            node: node.id.clone(),
            endpoint: None,
        });
    }
    if let Some(s) = env.systems.iter().find(|s| s.slug == pattern) {
        return Ok(Endpointing::System {
            instance: s.id.clone(),
            endpoint: endpoint_def.cloned(),
        });
    }

    Err(nearby(
        &format!("環境 {} 裡找不到", env.slug),
        pattern,
        env.instances()
            .iter()
            .map(|i| &i.slug)
            .chain(env.infra.iter().map(|n| &n.slug))
            .chain(env.systems.iter().map(|s| &s.slug)),
    ))
}

fn matching(env: &Environment, pattern: &str, within: Option<&Id>) -> u32 {
    match within {
        None => env.instances_matching(pattern).len() as u32,
        Some(node) => {
            let Some(scope) = find_node_ref(&env.nodes, node) else {
                return 0;
            };
            scope
                .instances_recursive()
                .iter()
                .filter(|i| loom_core::pattern::matches(pattern, &i.slug))
                .count() as u32
        }
    }
}

fn find_node(nodes: &[loom_core::environment::DeploymentNode], slug: &str) -> Option<Id> {
    for n in nodes {
        if n.slug == slug {
            return Some(n.id.clone());
        }
        if let Some(found) = find_node(&n.children, slug) {
            return Some(found);
        }
    }
    None
}

fn find_node_ref<'a>(
    nodes: &'a [loom_core::environment::DeploymentNode],
    id: &Id,
) -> Option<&'a loom_core::environment::DeploymentNode> {
    for n in nodes {
        if &n.id == id {
            return Some(n);
        }
        if let Some(found) = find_node_ref(&n.children, id) {
            return Some(found);
        }
    }
    None
}

fn node_slugs(nodes: &[loom_core::environment::DeploymentNode]) -> Vec<&str> {
    let mut out = Vec::new();
    for n in nodes {
        out.push(n.slug.as_str());
        out.extend(node_slugs(&n.children));
    }
    out
}

/// 「找不到 X。有的是：a、b、c」。
///
/// **一定要附上候選。** 只說「找不到」的話，Agent 唯一能做的是猜，
/// 而它會猜到一個看起來很像但其實不存在的名字，然後再錯一次。
fn nearby<S: AsRef<str>>(what: &str, given: &str, candidates: impl Iterator<Item = S>) -> String {
    format!("{what} {given}。有的是：{}", list(candidates))
}

fn list<S: AsRef<str>>(items: impl Iterator<Item = S>) -> String {
    let mut v: Vec<String> = items.map(|s| s.as_ref().to_string()).collect();
    v.sort();
    v.dedup();
    if v.is_empty() {
        return "（一個都沒有）".into();
    }
    v.join("、")
}
