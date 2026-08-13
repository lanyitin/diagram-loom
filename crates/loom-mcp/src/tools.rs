//! Agent 能做的事。
//!
//! # 設計原則：**每個回應都要能讓 Agent 自己往下走**
//!
//! 這裡的工具不是給程式呼叫的 API，是給一個會犯錯、看得懂中文、
//! 但看不到畫面的東西用的。所以：
//!
//! - **錯誤一律附上候選。** 「找不到 redis」沒用，它會再猜一次；
//!   「找不到 redis，有的是：redis-01、redis-02」它就會自己修正。
//! - **每次修改之後回報 lint 的變化。** 建了東西卻多冒出三個錯誤，
//!   要當下就講，不要等它全部做完才發現。
//! - **回的是人看得懂的文字，不是 JSON。** Agent 讀文字比讀巢狀
//!   結構準，而且這些字最後會被貼給使用者看。
//!
//! # 為什麼沒有「存檔」這個工具
//!
//! 刻意的。Agent 改完，App 的標題列亮起「未儲存」，由**人**看過再決定。
//! 讓 Agent 直接寫進磁碟，等於把這個工具最後一道人工檢查拿掉——
//! 而它的整個賣點就是「怕漏」。

use serde_json::{Value, json};

use loom_core::batch::{BatchSpec, EndpointPlan};
use loom_core::edit::Edit;
use loom_core::environment::{ConnectionKind, NodeKind};
use loom_core::id::Id;
use loom_core::lint::{Rule, Severity, lint};
use loom_core::logical::{Protocol, RelationshipEnd};
use loom_core::resource::{Kind, Resource, blank};
use loom_core::{Project, connect, inventory};

use crate::Workspace;
use crate::refs;

/// 工具清單。內容是靜態的，所以描述文字就是 Agent 唯一的說明書——
/// 寫不清楚它就會用錯，而用錯的成本是使用者的專案被弄髒。
pub fn list() -> Value {
    json!([
        tool("describe", "\
看目前這份專案裡已經有什麼：系統、服務、接點定義、契約、環境、機器、設備。
**動手之前先叫這個。** 不然你會重複建立已經存在的東西，而那會撞名失敗。",
            json!({"type": "object", "properties": {}})),

        tool("lint", "\
看這份專案現在還缺什麼。

**這是你自我檢查的方式。** 從一段文字建模型一定會漏東西——漏接點、
漏某個環境、漏一段路徑。每建完一批就叫一次，照著它說的補。
回應會告訴你每一項該怎麼修。",
            json!({"type": "object", "properties": {}})),

        tool("create", "\
建立一個模型元素。

順序有差，照這個來（前面的是後面的前提）：
  1. system        系統。服務要掛在它底下
  2. container     服務（Redis、Consul 這種會接收請求的程序）
  3. endpoint_def  接點定義。契約要指定連到哪一個
  4. relationship  契約：誰連誰。這是母版，每個環境都必須實現
  5. environment   環境（prod / uat / dev…）
  6. node / infra / system_instance   環境裡的機器、F5、外部系統落地

各種 kind 要填的欄位：
  system            slug, name, external(bool)
  container         slug, name, system(系統的 slug)
  endpoint_def      owner(服務或系統的 slug), slug, protocol(tcp/udp/unix-socket/jdbc/file)
  relationship      slug, purpose, from, to, to_endpoint
                    from/to 寫 `container:名字`、`system:名字` 或 `person:名字`
                    to_endpoint 寫目標身上那個接點定義的 slug
  person            slug, name
  environment       slug, name
  node              environment, slug, kind(site/physical/virtual-machine/linux-container), within(選填)
  infra             environment, slug
  infra_endpoint    environment, owner(設備的 slug), slug, address
  system_instance   environment, slug, system(系統的 slug)

要一次建很多台機器請改用 create_nodes。",
            json!({
                "type": "object",
                "required": ["kind", "fields"],
                "properties": {
                    "kind": {"type": "string", "enum": [
                        "system", "container", "endpoint_def", "relationship", "person",
                        "environment", "node", "infra", "infra_endpoint", "system_instance"]},
                    "fields": {"type": "object", "description": "見上面各 kind 的欄位"}
                }
            })),

        tool("create_nodes", "\
一次建立多台機器，每台上面跑一個指定的服務。

六台 Redis 手打六次會打錯，而**打錯的通常是 IP，錯了不會有人發現**。
所以用樣板：`{n}` 是序號、`{ip}` 是位址序號，兩個分開數
（現實中 redis-01…redis-06 常常對到 10.0.1.11…10.0.1.16）。

撞名會整批擋下來，不會建一半。",
            json!({
                "type": "object",
                "required": ["environment", "container", "count", "address_template"],
                "properties": {
                    "environment": {"type": "string", "description": "環境的 slug"},
                    "container": {"type": "string", "description": "服務的 slug"},
                    "count": {"type": "integer", "minimum": 1},
                    "name_template": {"type": "string", "description": "落地名稱，預設 `<服務>-{n}`"},
                    "node_template": {"type": "string", "description": "機器名稱，預設 `vm-<服務>-{n}`"},
                    "address_template": {"type": "string", "description": "例如 `10.0.1.{ip}:6379`"},
                    "endpoint": {"type": "string", "description": "用服務身上的哪個接點定義。只有一個時可省略"},
                    "start": {"type": "integer", "description": "`{n}` 從幾號開始，預設 1"},
                    "pad": {"type": "integer", "description": "`{n}` 補零幾位，預設 2"},
                    "ip_start": {"type": "integer", "description": "`{ip}` 從幾號開始，預設 1"},
                    "node_kind": {"type": "string", "enum": ["physical", "virtual-machine", "linux-container"]},
                    "within": {"type": "string", "description": "放在哪個節點（通常是站點）底下"}
                }
            })),

        tool("propose_connection", "\
照契約擬一條「這個環境應該要有」的連線，但**不建立**。

先看它擬成什麼再決定。它只擬直達的一段——中間要不要經過 F5 是人的決定，
母版上沒有那個資訊。",
            json!({
                "type": "object",
                "required": ["environment", "relationship"],
                "properties": {
                    "environment": {"type": "string"},
                    "relationship": {"type": "string", "description": "契約的 slug"}
                }
            })),

        tool("add_connection", "\
在某個環境實現一條契約。

`from` / `to` 用名字寫，不要用 id：
  redis-01              那一台
  redis-*               一整群（會自動幫你算 expect）
  redis-* @ dc-main     限定在某個站點底下的那一群
  f5-01 : vip-redis     設備上的某個 VIP
  person:customer       人（只能當來源）
  system:payment        外部系統的落地

兩個都省略的話就照提案建（直達的一段）。

**經過 F5 的流量要拆成兩段**，兩段都填同一個 relationship：
  第一段  from: apache-*   to: f5-01 : vip-gateway
  第二段  from: f5-01      to: gateway-*
少了第二段的話 lint 會說走不通（L002）——那正是它該抓的東西。",
            json!({
                "type": "object",
                "required": ["environment", "relationship"],
                "properties": {
                    "environment": {"type": "string"},
                    "relationship": {"type": "string", "description": "契約的 slug"},
                    "from": {"type": "string"},
                    "to": {"type": "string"},
                    "purpose": {"type": "string", "description": "省略時沿用契約的用途"},
                    "fallback": {"type": "boolean", "description": "只在故障時才走的備援路徑"}
                }
            })),

        tool("update", "\
改一個既有元素的欄位。`id` 從 describe 或 lint 的輸出拿。
只填要改的欄位，沒填的不動。",
            json!({
                "type": "object",
                "required": ["id", "fields"],
                "properties": {"id": {"type": "string"}, "fields": {"type": "object"}}
            })),

        tool("delete", "\
刪掉一個元素。

**不會連帶刪除**：指著它的東西會變成懸空，由 lint（L012）叫出來。
所以刪之前先想清楚，刪之後一定要跑一次 lint。",
            json!({
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}}
            })),
    ])
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

/// 呼叫一個工具。回的是給 Agent 讀的文字。
pub fn call(ws: &mut dyn Workspace, name: &str, args: &Value) -> Result<String, String> {
    if ws.project().is_none() {
        return Err(
            "使用者還沒有開啟任何專案。請他在 diagram-loom 裡開一個，或用「新專案…」建一個。"
                .into(),
        );
    }
    match name {
        "describe" => Ok(describe(ws.project().unwrap())),
        "lint" => Ok(report(ws.project().unwrap())),
        "create" => create(ws, args),
        "create_nodes" => create_nodes(ws, args),
        "propose_connection" => propose_connection(ws, args),
        "add_connection" => add_connection(ws, args),
        "update" => update(ws, args),
        "delete" => delete(ws, args),
        _ => Err(format!("沒有這個工具：{name}")),
    }
}

// ── 讀 ──────────────────────────────────────────────────────────

fn describe(project: &Project) -> String {
    let mut out = format!("專案：{}（{}）\n", project.name, project.slug);

    for env in &project.environments {
        out.push_str(&format!("環境 {} ({})\n", env.slug, env.name));
    }
    if project.environments.is_empty() {
        out.push_str("環境：一個都還沒有。沒有環境的話 lint 不會有話說——邏輯層自己不會缺什麼。\n");
    }

    // 每種資源一段。用 inventory 那份，跟畫面上看到的完全一樣——
    // Agent 與使用者看到的不一致的話，兩邊會講不到同一件事。
    for table in inventory::tables(project, project.environments.first().map(|e| &e.id)) {
        out.push_str(&format!("\n## {}（{}）\n", table.title, table.rows.len()));
        if table.rows.is_empty() {
            out.push_str(&format!("（還沒有）{}\n", table.empty_hint));
            continue;
        }
        out.push_str(&format!("| id | {} |\n", table.columns.join(" | ")));
        for row in &table.rows {
            out.push_str(&format!("| {} | {} |\n", row.id, row.cells.join(" | ")));
        }
    }
    out
}

fn report(project: &Project) -> String {
    let findings = lint(project);
    if findings.is_empty() {
        return "沒有發現任何缺漏。".into();
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity() == Severity::Error)
        .count();
    let mut out = format!("{} 個錯誤、{} 個警告：\n", errors, findings.len() - errors);
    for f in &findings {
        let env = f
            .environment
            .as_ref()
            .and_then(|id| project.environment(id))
            .map(|e| e.slug.as_str())
            .unwrap_or("邏輯層");
        out.push_str(&format!(
            "- {} [{env}] {}（subject: {}）{}\n",
            f.rule.code(),
            f.detail,
            f.subject,
            how_to_fix(f.rule)
        ));
    }
    out
}

/// 每一項都附上「這種問題怎麼修」。
///
/// 少了這一句，Agent 看到 L002 只會知道「有東西壞了」，
/// 然後開始亂試——最常見的亂試是把契約刪掉，那完全是反效果。
fn how_to_fix(rule: Rule) -> &'static str {
    match rule {
        Rule::L001 => " → 用 create_nodes 建機器，或用 add_connection 補連線",
        Rule::L002 => " → 路徑斷了。用 add_connection 補上缺的那一段（經過 F5 要拆兩段）",
        Rule::L003 => " → 指到了不存在的東西。用 update 改掉那個參照",
        Rule::L004 => " → 用 update 改 expect，或補上少的那幾台機器",
        Rule::L005 => " → 萬用字元要填 expect",
        Rule::L006 => " → 用 update 補上位址",
        Rule::L007 => " → 用 update 補上用途",
        Rule::L008 => " → 沒有連線碰到它。補一條連線，或它真的是冷備機就 update standalone=true",
        Rule::L012 => " → 參照壞了。用 update 改掉，或把懸空的那個元素 delete",
    }
}

// ── 寫 ──────────────────────────────────────────────────────────

/// 每次修改都回報 lint 的變化。
///
/// 「建好了」對 Agent 沒有資訊量；「建好了，但多出兩個錯誤」它才知道
/// 要不要繼續往下走。這跟刪除確認框問「會弄壞什麼」是同一個道理。
fn with_lint_delta(ws: &mut dyn Workspace, edit: &Edit, done: &str) -> Result<String, String> {
    let before = lint(ws.project().unwrap());
    ws.edit(edit)?;
    let after = lint(ws.project().unwrap());

    let introduced: Vec<_> = after.iter().filter(|f| !before.contains(f)).collect();
    let resolved = before.iter().filter(|f| !after.contains(f)).count();

    let mut out = done.to_string();
    if resolved > 0 {
        out.push_str(&format!("\n解掉了 {resolved} 項既有的問題。"));
    }
    if introduced.is_empty() {
        out.push_str("\n沒有多出任何問題。");
    } else {
        out.push_str(&format!("\n但多出 {} 項問題：", introduced.len()));
        for f in introduced {
            out.push_str(&format!(
                "\n- {} {}{}",
                f.rule.code(),
                f.detail,
                how_to_fix(f.rule)
            ));
        }
    }
    out.push_str("\n（尚未存檔——使用者會在畫面上看過再決定。）");
    Ok(out)
}

fn create(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let kind_name = str_field(args, "kind")?;
    let fields = args.get("fields").cloned().unwrap_or_else(|| json!({}));
    let project = ws.project().unwrap();

    let (kind, environment, owner) = match kind_name {
        "system" => (Kind::System, None, None),
        "person" => (Kind::Person, None, None),
        "environment" => (Kind::Environment, None, None),
        "relationship" => (Kind::Relationship, None, None),
        "container" => (
            Kind::Container,
            None,
            Some(system_id(project, str_field(&fields, "system")?)?),
        ),
        "endpoint_def" => (
            Kind::EndpointDef,
            None,
            Some(owner_id(project, str_field(&fields, "owner")?)?),
        ),
        "node" => (
            Kind::Node,
            Some(env_id(project, str_field(&fields, "environment")?)?),
            opt_str(&fields, "within")
                .map(|s| node_id(project, str_field(&fields, "environment")?, s))
                .transpose()?,
        ),
        "infra" => (
            Kind::Infra,
            Some(env_id(project, str_field(&fields, "environment")?)?),
            None,
        ),
        "infra_endpoint" => {
            let env = str_field(&fields, "environment")?;
            (
                Kind::InfraEndpoint,
                Some(env_id(project, env)?),
                Some(infra_id(project, env, str_field(&fields, "owner")?)?),
            )
        }
        "system_instance" => (
            Kind::SystemInstance,
            Some(env_id(project, str_field(&fields, "environment")?)?),
            Some(system_id(project, str_field(&fields, "system")?)?),
        ),
        other => return Err(format!("認不得的 kind：{other}")),
    };

    let mut resource = blank(kind, environment, owner);
    fill(&mut resource, &fields, project)?;
    let slug = resource.slug().to_string();
    let id = resource.id().clone();
    with_lint_delta(
        ws,
        &Edit::AddResource(resource),
        &format!("建好了：{kind_name} {slug}（id: {id}）"),
    )
}

fn update(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let id = Id::new(str_field(args, "id")?);
    let fields = args.get("fields").cloned().unwrap_or_else(|| json!({}));
    let project = ws.project().unwrap();

    let mut resource = find_resource(project, &id)?;
    fill(&mut resource, &fields, project)?;
    let slug = resource.slug().to_string();
    with_lint_delta(
        ws,
        &Edit::UpdateResource(resource),
        &format!("改好了：{slug}"),
    )
}

fn delete(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let id = Id::new(str_field(args, "id")?);
    let resource = find_resource(ws.project().unwrap(), &id)?;
    let what = format!("{} {}", resource.kind_name(), resource.slug());
    with_lint_delta(
        ws,
        &Edit::DeleteResource(resource),
        &format!("刪掉了：{what}"),
    )
}

fn create_nodes(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let project = ws.project().unwrap();
    let env_slug = str_field(args, "environment")?;
    let env = project
        .environment(&env_id(project, env_slug)?)
        .ok_or("找不到環境")?
        .clone();

    let container_slug = str_field(args, "container")?;
    let container = project
        .logical
        .containers
        .iter()
        .find(|c| c.slug == container_slug)
        .ok_or_else(|| {
            format!(
                "找不到服務 {container_slug}。有的是：{}",
                slugs(project.logical.containers.iter().map(|c| &c.slug))
            )
        })?;

    // 接點只有一個時不必指定——那是最常見的情況，多問一次只是噪音。
    let endpoint = match opt_str(args, "endpoint") {
        Some(want) => container
            .endpoints
            .iter()
            .find(|e| e.slug == want)
            .ok_or_else(|| {
                format!(
                    "服務 {container_slug} 上沒有接點 {want}。有的是：{}",
                    slugs(container.endpoints.iter().map(|e| &e.slug))
                )
            })?,
        None => match container.endpoints.as_slice() {
            [only] => only,
            [] => {
                return Err(format!(
                    "服務 {container_slug} 還沒有任何接點定義。先 create endpoint_def。"
                ));
            }
            many => {
                return Err(format!(
                    "服務 {container_slug} 有 {} 個接點，要指定哪一個：{}",
                    many.len(),
                    slugs(many.iter().map(|e| &e.slug))
                ));
            }
        },
    };

    let spec = BatchSpec {
        count: u32_field(args, "count")?,
        name_template: opt_str(args, "name_template")
            .unwrap_or(&format!("{container_slug}-{{n}}"))
            .to_string(),
        node_template: opt_str(args, "node_template")
            .unwrap_or(&format!("vm-{container_slug}-{{n}}"))
            .to_string(),
        start: u32_or(args, "start", 1),
        pad: u32_or(args, "pad", 2),
        address_template: str_field(args, "address_template")?.to_string(),
        ip_start: u32_or(args, "ip_start", 1),
        node_kind: match opt_str(args, "node_kind") {
            Some("physical") => NodeKind::Physical,
            Some("linux-container") => NodeKind::LinuxContainer,
            _ => NodeKind::VirtualMachine,
        },
        container: container.id.clone(),
        endpoint: EndpointPlan {
            def: endpoint.id.clone(),
            slug: endpoint.slug.clone(),
            protocol: endpoint.protocol,
        },
    };

    let within = opt_str(args, "within")
        .map(|s| node_id(project, env_slug, s))
        .transpose()?;
    let plan = loom_core::batch::plan(project, &env, &spec).map_err(|e| e.to_string())?;
    let listing = plan.preview.join("\n  ");

    with_lint_delta(
        ws,
        &Edit::AddInstances {
            environment: env.id.clone(),
            within,
            nodes: plan.nodes,
        },
        &format!("建好了 {} 台：\n  {listing}", spec.count),
    )
}

fn propose_connection(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let project = ws.project().unwrap();
    let (env, rel) = env_and_relationship(project, args)?;
    let p = connect::propose(project, &env, &rel).ok_or("找不到契約")?;

    let mut out = format!(
        "契約 {} 在 {} 的提案：\n",
        str_field(args, "relationship")?,
        env.slug
    );
    out.push_str(&format!("  用途：{}\n", p.purpose));
    out.push_str(&format!(
        "  完整：{}\n",
        if p.is_complete() { "是" } else { "否" }
    ));
    for n in &p.notes {
        out.push_str(&format!("  ⚠ {n}\n"));
    }
    out.push_str("直接呼叫 add_connection（不填 from/to）就會照這份建。");
    Ok(out)
}

fn add_connection(ws: &mut dyn Workspace, args: &Value) -> Result<String, String> {
    let project = ws.project().unwrap();
    let (env, rel_id) = env_and_relationship(project, args)?;
    let relationship = project
        .logical
        .relationships
        .iter()
        .find(|r| r.id == rel_id)
        .ok_or("找不到契約")?;
    let to_endpoint = relationship.to_endpoint.clone();

    let proposal = connect::propose(project, &env, &rel_id).ok_or("找不到契約")?;

    let from = match opt_str(args, "from") {
        Some(text) => refs::resolve(project, &env, text, None)?,
        None => proposal
            .from
            .clone()
            .ok_or_else(|| format!("擬不出來源，請自己指定 from。{}", proposal.notes.join(" ")))?,
    };
    let to = match opt_str(args, "to") {
        Some(text) => refs::resolve(project, &env, text, Some(&to_endpoint))?,
        None => proposal
            .to
            .clone()
            .ok_or_else(|| format!("擬不出目標，請自己指定 to。{}", proposal.notes.join(" ")))?,
    };

    let purpose = opt_str(args, "purpose")
        .map(str::to_string)
        .unwrap_or(proposal.purpose);

    with_lint_delta(
        ws,
        &Edit::AddConnection {
            environment: env.id.clone(),
            id: proposal.id,
            serves: rel_id,
            purpose,
            kind: if args.get("fallback").and_then(Value::as_bool) == Some(true) {
                ConnectionKind::Fallback
            } else {
                ConnectionKind::Primary
            },
            from,
            to,
        },
        &format!("在 {} 建好一段連線。", env.slug),
    )
}

// ── 把 fields 填進 Resource ──────────────────────────────────────

/// 只動有填的欄位。沒填的維持原樣——`update` 就靠這個做到「只改一部分」。
fn fill(resource: &mut Resource, fields: &Value, project: &Project) -> Result<(), String> {
    let s = |k: &str| opt_str(fields, k).map(str::to_string);

    match resource {
        Resource::Person(p) => {
            if let Some(v) = s("slug") {
                p.slug = v
            }
            if let Some(v) = s("name") {
                p.name = v
            }
        }
        Resource::System(x) => {
            if let Some(v) = s("slug") {
                x.slug = v
            }
            if let Some(v) = s("name") {
                x.name = v
            }
            if let Some(v) = fields.get("external").and_then(Value::as_bool) {
                x.external = v
            }
        }
        Resource::Container(c) => {
            if let Some(v) = s("slug") {
                c.slug = v
            }
            if let Some(v) = s("name") {
                c.name = v
            }
            if let Some(v) = s("system") {
                c.system = system_id(project, &v)?
            }
        }
        Resource::EndpointDef { def, .. } => {
            if let Some(v) = s("slug") {
                def.slug = v
            }
            if let Some(v) = s("protocol") {
                def.protocol = protocol(&v)?
            }
        }
        Resource::Relationship(r) => {
            if let Some(v) = s("slug") {
                r.slug = v
            }
            if let Some(v) = s("purpose") {
                r.purpose = v
            }
            if let Some(v) = s("from") {
                r.from = relationship_end(project, &v)?
            }
            if let Some(v) = s("to") {
                r.to = relationship_end(project, &v)?
            }
            if let Some(v) = s("to_endpoint") {
                r.to_endpoint = endpoint_def_id(project, &r.to, &v)?
            }
        }
        Resource::Environment(e) => {
            if let Some(v) = s("slug") {
                e.slug = v
            }
            if let Some(v) = s("name") {
                e.name = v
            }
        }
        Resource::Node { node, .. } => {
            if let Some(v) = s("slug") {
                node.slug = v
            }
            if let Some(v) = s("kind") {
                node.kind = match v.as_str() {
                    "site" => NodeKind::Site,
                    "physical" => NodeKind::Physical,
                    "virtual-machine" => NodeKind::VirtualMachine,
                    "linux-container" => NodeKind::LinuxContainer,
                    other => {
                        return Err(format!(
                            "認不得的機器種類 {other}。可以用：site、physical、virtual-machine、linux-container"
                        ));
                    }
                }
            }
        }
        Resource::Infra { node, .. } => {
            if let Some(v) = s("slug") {
                node.slug = v
            }
        }
        Resource::InfraEndpoint { endpoint, .. } => {
            if let Some(v) = s("slug") {
                endpoint.slug = v
            }
            if let Some(v) = s("address") {
                endpoint.address = Some(v)
            }
            if let Some(v) = s("protocol") {
                endpoint.protocol = protocol(&v)?
            }
        }
        Resource::SystemInstance { instance, .. } => {
            if let Some(v) = s("slug") {
                instance.slug = v
            }
            if let Some(v) = s("system") {
                instance.system = system_id(project, &v)?
            }
            if let Some(v) = fields.get("standalone").and_then(Value::as_bool) {
                instance.standalone = v
            }
        }
    }
    Ok(())
}

fn relationship_end(project: &Project, text: &str) -> Result<RelationshipEnd, String> {
    let (kind, slug) = text.split_once(':').ok_or_else(|| {
        format!(
            "契約的兩端要寫成 `container:名字`、`system:名字` 或 `person:名字`，收到的是 {text}"
        )
    })?;
    let slug = slug.trim();
    match kind.trim() {
        "container" => Ok(RelationshipEnd::Container(container_id(project, slug)?)),
        "system" => Ok(RelationshipEnd::System(system_id(project, slug)?)),
        "person" => project
            .logical
            .people
            .iter()
            .find(|p| p.slug == slug)
            .map(|p| RelationshipEnd::Person(p.id.clone()))
            .ok_or_else(|| {
                format!(
                    "找不到人 {slug}。有的是：{}",
                    slugs(project.logical.people.iter().map(|p| &p.slug))
                )
            }),
        other => Err(format!(
            "認不得的端點種類 {other}，只能是 container / system / person"
        )),
    }
}

/// 接點定義必須掛在**目標那一端身上**。查錯地方的話 lint 會報 L012，
/// 而那個錯誤離現場很遠，很難追回來——在這裡擋掉比較好解釋。
fn endpoint_def_id(project: &Project, to: &RelationshipEnd, slug: &str) -> Result<Id, String> {
    let defs = match to {
        RelationshipEnd::Container(id) => project
            .logical
            .container(id)
            .map(|c| c.endpoints.as_slice())
            .unwrap_or_default(),
        RelationshipEnd::System(id) => project
            .logical
            .systems
            .iter()
            .find(|s| &s.id == id)
            .map(|s| s.endpoints.as_slice())
            .unwrap_or_default(),
        RelationshipEnd::Person(_) => return Err("目標不能是人，人沒有接點".into()),
    };
    defs.iter()
        .find(|d| d.slug == slug)
        .map(|d| d.id.clone())
        .ok_or_else(|| {
            format!(
                "目標身上沒有接點定義 {slug}。它有的是：{}",
                slugs(defs.iter().map(|d| &d.slug))
            )
        })
}

fn protocol(v: &str) -> Result<Protocol, String> {
    match v {
        "tcp" => Ok(Protocol::Tcp),
        "udp" => Ok(Protocol::Udp),
        "unix-socket" => Ok(Protocol::UnixSocket),
        "jdbc" => Ok(Protocol::Jdbc),
        "file" => Ok(Protocol::File),
        other => Err(format!(
            "認不得的協定 {other}。可以用：tcp、udp、unix-socket、jdbc、file"
        )),
    }
}

// ── 查 id ───────────────────────────────────────────────────────

fn find_resource(project: &Project, id: &Id) -> Result<Resource, String> {
    // 走 inventory 而不是自己再寫一次搜尋：那份是畫面在用的同一批資料，
    // 所以 Agent 拿到的 id 一定查得到，兩邊不會有一邊看得到一邊看不到。
    for env in project
        .environments
        .iter()
        .map(|e| Some(&e.id))
        .chain(std::iter::once(None))
    {
        for table in inventory::tables(project, env) {
            if let Some(row) = table.rows.iter().find(|r| &r.id == id) {
                return Ok(row.resource.clone());
            }
        }
    }
    Err(format!(
        "找不到 id 為 {id} 的元素。用 describe 看目前有什麼。"
    ))
}

fn env_and_relationship(
    project: &Project,
    args: &Value,
) -> Result<(loom_core::environment::Environment, Id), String> {
    let env = project
        .environment(&env_id(project, str_field(args, "environment")?)?)
        .ok_or("找不到環境")?
        .clone();
    let want = str_field(args, "relationship")?;
    let rel = project
        .logical
        .relationships
        .iter()
        .find(|r| r.slug == want)
        .map(|r| r.id.clone())
        .ok_or_else(|| {
            format!(
                "找不到契約 {want}。有的是：{}",
                slugs(project.logical.relationships.iter().map(|r| &r.slug))
            )
        })?;
    Ok((env, rel))
}

fn env_id(project: &Project, slug: &str) -> Result<Id, String> {
    project
        .environments
        .iter()
        .find(|e| e.slug == slug)
        .map(|e| e.id.clone())
        .ok_or_else(|| {
            format!(
                "找不到環境 {slug}。有的是：{}",
                slugs(project.environments.iter().map(|e| &e.slug))
            )
        })
}

fn system_id(project: &Project, slug: &str) -> Result<Id, String> {
    project
        .logical
        .systems
        .iter()
        .find(|s| s.slug == slug)
        .map(|s| s.id.clone())
        .ok_or_else(|| {
            format!(
                "找不到系統 {slug}。有的是：{}",
                slugs(project.logical.systems.iter().map(|s| &s.slug))
            )
        })
}

fn container_id(project: &Project, slug: &str) -> Result<Id, String> {
    project
        .logical
        .containers
        .iter()
        .find(|c| c.slug == slug)
        .map(|c| c.id.clone())
        .ok_or_else(|| {
            format!(
                "找不到服務 {slug}。有的是：{}",
                slugs(project.logical.containers.iter().map(|c| &c.slug))
            )
        })
}

/// 接點定義可以掛在服務或外部系統身上，兩邊都找。
fn owner_id(project: &Project, slug: &str) -> Result<Id, String> {
    container_id(project, slug).or_else(|_| system_id(project, slug))
}

fn node_id(project: &Project, env_slug: &str, slug: &str) -> Result<Id, String> {
    let env = project
        .environment(&env_id(project, env_slug)?)
        .ok_or("找不到環境")?;
    fn walk(nodes: &[loom_core::environment::DeploymentNode], slug: &str) -> Option<Id> {
        for n in nodes {
            if n.slug == slug {
                return Some(n.id.clone());
            }
            if let Some(f) = walk(&n.children, slug) {
                return Some(f);
            }
        }
        None
    }
    walk(&env.nodes, slug).ok_or_else(|| format!("環境 {env_slug} 裡找不到節點 {slug}"))
}

fn infra_id(project: &Project, env_slug: &str, slug: &str) -> Result<Id, String> {
    let env = project
        .environment(&env_id(project, env_slug)?)
        .ok_or("找不到環境")?;
    env.infra
        .iter()
        .find(|n| n.slug == slug)
        .map(|n| n.id.clone())
        .ok_or_else(|| {
            format!(
                "環境 {env_slug} 裡找不到設備 {slug}。有的是：{}",
                slugs(env.infra.iter().map(|n| &n.slug))
            )
        })
}

// ── 讀參數 ──────────────────────────────────────────────────────

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("少了必填欄位 {key}"))
}

fn opt_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn u32_field(v: &Value, key: &str) -> Result<u32, String> {
    v.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .ok_or_else(|| format!("少了必填欄位 {key}（要是數字）"))
}

fn u32_or(v: &Value, key: &str, default: u32) -> u32 {
    v.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as u32)
        .unwrap_or(default)
}

fn slugs<S: AsRef<str>>(items: impl Iterator<Item = S>) -> String {
    let mut v: Vec<String> = items.map(|s| s.as_ref().to_string()).collect();
    v.sort();
    if v.is_empty() {
        "（一個都沒有）".into()
    } else {
        v.join("、")
    }
}
