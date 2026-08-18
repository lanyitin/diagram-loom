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
use loom_core::cascade;
use loom_core::edit::Edit;
use loom_core::environment::{ConnectionEnd, ConnectionKind, NodeKind};
use loom_core::id::Id;
use loom_core::lint::lint;
use loom_core::logical::{Protocol, RelationshipEnd};
use loom_core::resource::{Kind, Resource, blank};
use loom_core::{Project, connect};

use crate::Workspace;
use crate::query::{self, Detail, Scope, Select};
use crate::refs;
use crate::target::{self, Target};

/// 工具清單。說明書在 [`crate::manual`]——它是 Agent 唯一讀得到的東西，
/// 所以獨立成一個模組，改描述不必捲過幾百行 `match`。
pub fn list() -> Value {
    crate::manual::list()
}

/// Agent 說要動哪一個專案。呼叫端要先拿它去 [`crate::pick::resolve`]。
///
/// 挑選的規則不放在這裡，因為挑選要看**全部**開著的專案，而 `ws` 只代表
/// 已經挑好的那一個。分開之後，呼叫端才有辦法先解析、再只鎖那一個專案。
pub fn wanted(args: &Value) -> Option<&str> {
    query::wanted_project(args)
}

/// 這個工具需不需要先挑一個專案。
///
/// 只有 `projects` 不需要——它問的正是「有哪些可以挑」。少了這個判斷，
/// 開著兩個專案時它會被「你沒說要動哪一個」擋下來，
/// 於是 Agent 唯一能問路的工具剛好是唯一問不到的那個。
pub fn needs_project(name: &str) -> bool {
    name != "projects"
}

/// 呼叫一個工具。回的是給 Agent 讀的文字。
///
/// `ws` 是**已經挑好的那一個專案**；`open` 是全部開著的，只給 `projects`
/// 與錯誤訊息用——工具本身不該有辦法碰到別的專案。
///
/// # 為什麼參數在這裡就解析完
///
/// `scope` / `select` / `detail` 每一支的意思都一樣，所以在分派之前解析完，
/// 八支工具就不可能對同一個欄位有八種理解。認不得的值也在這裡一次擋掉，
/// 錯誤訊息因此只有一份。
pub fn call(
    ws: &mut dyn Workspace,
    open: &[crate::pick::Open],
    name: &str,
    args: &Value,
) -> Result<String, String> {
    if name == "projects" {
        return Ok(crate::pick::listing(open));
    }
    if ws.project().is_none() {
        return Err(crate::pick::NOTHING_OPEN.into());
    }

    let scope = Scope::of(args);
    match name {
        "describe" => {
            let project = ws.project().unwrap();
            Ok(query::describe(
                project,
                &scope,
                &Select::of(args)?,
                Detail::of(args)?,
            ))
        }
        "lint" => {
            let project = ws.project().unwrap();
            Ok(query::report(
                project,
                &scope,
                &Select::of(args)?,
                Detail::of(args)?,
            ))
        }
        "create" => create(ws, &scope, args),
        "create_nodes" => create_nodes(ws, &scope, args),
        "add_connection" => add_connection(ws, &scope, args),
        "update" => update(ws, &scope, args),
        "delete" => delete(ws, &scope, args),
        // 併掉的那一支。回一句指路，不是「沒有這個工具」——舊的設定檔與
        // 舊的對話裡還留著這個名字，而它想做的事現在有更好的做法。
        "propose_connection" => Err("propose_connection 已經併進 add_connection 了：\
             送同一批 items 並加上 `dry_run: true`，它會擬給你看而且一個都不建。"
            .into()),
        _ => Err(format!(
            "沒有這個工具：{name}。有的是：projects、describe、lint、create、\
             create_nodes、add_connection、update、delete"
        )),
    }
}

// ── 寫 ──────────────────────────────────────────────────────────

/// 每次修改都回報 lint 的變化。
///
/// 「建好了」對 Agent 沒有資訊量；「建好了，但多出兩個錯誤」它才知道
/// 要不要繼續往下走。這跟刪除確認框問「會弄壞什麼」是同一個道理。
///
/// # `dry_run` 為什麼每一支寫入工具都收
///
/// 「我想先看看」以前只有 `delete` 答得出來，而 `propose_connection` 是
/// 專門為了回答這件事而存在的**第九支工具**——只服務一種操作。
///
/// 現在它是每一支的同一個欄位，所以 Agent 學一次用四處，
/// 也就不會為了看一眼而真的建下去。算的是同一份 [`loom_core::edit::preview`]，
/// 跟畫面上的刪除確認框看到的是同一個「會弄壞什麼」。
fn with_lint_delta(
    ws: &mut dyn Workspace,
    edit: &Edit,
    done: &str,
    dry_run: bool,
) -> Result<String, String> {
    if dry_run {
        let project = ws.project().unwrap();
        let impact = loom_core::edit::preview(project, edit).map_err(|e| e.to_string())?;
        let mut out = format!("**預覽，什麼都還沒有動。**\n{done}");
        if impact.introduced.is_empty() {
            out.push_str("\n這樣做不會多出任何問題。");
        } else {
            out.push_str(&format!(
                "\n這樣做會多出 {} 項問題：",
                impact.introduced.len()
            ));
            for f in &impact.introduced {
                out.push_str(&format!(
                    "\n- {} {}{}",
                    f.rule.code(),
                    f.detail,
                    query::how_to_fix(project, f)
                ));
            }
        }
        out.push_str("\n\n確定的話，把 `dry_run` 拿掉再送一次同一組參數。");
        return Ok(out);
    }

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
        // 用改完之後的專案分類——這些發現描述的就是改完的狀態。
        let now = ws.project().unwrap();
        for f in introduced {
            out.push_str(&format!(
                "\n- {} {}{}",
                f.rule.code(),
                f.detail,
                query::how_to_fix(now, f)
            ));
        }
    }
    out.push_str("\n（尚未存檔——使用者會在畫面上看過再決定。）");
    Ok(out)
}

/// 這一趟要不要真的動手。
fn dry_run(args: &Value) -> bool {
    args.get("dry_run").and_then(Value::as_bool) == Some(true)
}

fn create(ws: &mut dyn Workspace, scope: &Scope, args: &Value) -> Result<String, String> {
    let items = items_of(args, &["kind"])?;
    let env = scope.environment.as_deref();

    // 邊建邊套用在一份複本上，理由是**同一批裡後面的要指名前面的**：
    // 先建系統再建掛在它底下的服務，若整批都對著「還沒動過的專案」解名字，
    // 第二項就會說「找不到那個系統」——而那正是逼 Agent 一個一個送的原因。
    let mut scratch = ws.project().unwrap().clone();
    let mut edits = Vec::with_capacity(items.len());
    let mut made: Vec<(String, String, Id)> = Vec::with_capacity(items.len());

    for (i, item) in items.iter().enumerate() {
        let at = |e: String| in_batch(i, items.len(), &e);
        let resource = build_resource(&scratch, item, env).map_err(at)?;
        made.push((
            str_field(item, "kind").map_err(at)?.to_string(),
            resource.slug().to_string(),
            resource.id().clone(),
        ));

        let edit = Edit::AddResource(resource);
        // 在複本上先套一次：撞名之類的問題會在這裡被抓到，而且**知道是第幾項**。
        // 留到最後整批送才發現的話，錯誤訊息裡只有一個 slug，Agent 得自己去比對。
        loom_core::edit::apply(&mut scratch, &edit)
            .map_err(|e| in_batch(i, items.len(), &e.to_string()))?;
        edits.push(edit);
    }

    with_lint_delta(ws, &Edit::Batch(edits), &made_summary(&made), dry_run(args))
}

/// 建好之後回報什麼。
///
/// # 為什麼超過一定數量就不列 id
///
/// 三百個 id 是三百行雜訊，而 Agent 當下多半用不到——它是照 slug 在思考的，
/// 要 id 的時候（update／delete）再 describe 一次就有。
/// 少數幾個的時候倒是直接給，省它一趟。
const LIST_IDS_UP_TO: usize = 20;

fn made_summary(made: &[(String, String, Id)]) -> String {
    if made.len() <= LIST_IDS_UP_TO {
        let lines: Vec<String> = made
            .iter()
            .map(|(kind, slug, id)| format!("  {kind} {slug}（id: {id}）"))
            .collect();
        return format!("建好了 {} 個：\n{}", made.len(), lines.join("\n"));
    }

    // 照 kind 收攏。順序照第一次出現，因為那就是 Agent 送進來的順序，
    // 它比字母序好對照。
    let mut order: Vec<&str> = Vec::new();
    let mut count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (kind, _, _) in made {
        let slot = count.entry(kind.as_str()).or_insert_with(|| {
            order.push(kind);
            0
        });
        *slot += 1;
    }
    let parts: Vec<String> = order.iter().map(|k| format!("{k} {}", count[k])).collect();
    format!(
        "建好了 {} 個：{}。\n（數量太多就不列 id 了，要用 id 的時候 describe 查得到。）",
        made.len(),
        parts.join("、")
    )
}

/// 把一項 `{kind, fields}` 變成一個 [`Resource`]，名字全部對著 `project` 解。
///
/// `env_default` 是 `scope.environment`。每一項的 `fields.environment` 是舊寫法，
/// 收但不教——理由見 [`crate::manual`]。
fn build_resource(
    project: &Project,
    item: &Value,
    env_default: Option<&str>,
) -> Result<Resource, String> {
    let kind_name = str_field(item, "kind")?;
    let fields = item.get("fields").cloned().unwrap_or_else(|| json!({}));

    // 環境層的那幾種都要它，所以只解析一次。
    let env_slug = || -> Result<&str, String> {
        opt_str(&fields, "environment")
            .or(env_default)
            .ok_or_else(|| {
                format!(
                    "{kind_name} 是環境層的東西，要說清楚是哪一個環境。\
                 加上 `scope`: {{\"environment\": \"…\"}}。有的是：{}",
                    slugs(project.environments.iter().map(|e| &e.slug))
                )
            })
    };

    let (kind, environment, owner) = match query::kind_of(kind_name)? {
        Kind::System => (Kind::System, None, None),
        Kind::Person => (Kind::Person, None, None),
        Kind::Environment => (Kind::Environment, None, None),
        Kind::Relationship => (Kind::Relationship, None, None),
        Kind::Container => (
            Kind::Container,
            None,
            Some(system_id(project, str_field(&fields, "system")?)?),
        ),
        Kind::EndpointDef => (
            Kind::EndpointDef,
            None,
            Some(owner_id(project, str_field(&fields, "owner")?)?),
        ),
        Kind::Node => {
            let env = env_slug()?;
            (
                Kind::Node,
                Some(env_id(project, env)?),
                opt_str(&fields, "within")
                    .map(|s| node_id(project, env, s))
                    .transpose()?,
            )
        }
        Kind::Infra => (Kind::Infra, Some(env_id(project, env_slug()?)?), None),
        Kind::InfraEndpoint => {
            let env = env_slug()?;
            (
                Kind::InfraEndpoint,
                Some(env_id(project, env)?),
                Some(infra_id(project, env, str_field(&fields, "owner")?)?),
            )
        }
        Kind::SystemInstance => (
            Kind::SystemInstance,
            Some(env_id(project, env_slug()?)?),
            Some(system_id(project, str_field(&fields, "system")?)?),
        ),
        // 服務實體。`node` 不在這裡解，交給 `fill`——那樣「放到哪台機器上」
        // 建立與修改走的是同一行程式碼，也就不會有一邊做得到、一邊做不到。
        // 這裡只把「少了 node」講清楚，不然錯誤會晚到 `write_into` 才發生，
        // 而那時它只說得出「找不到 」。
        Kind::Instance => {
            let env = env_slug()?;
            str_field(&fields, "node")?;
            (Kind::Instance, Some(env_id(project, env)?), None)
        }
    };

    let mut resource = blank(kind, environment, owner);
    fill(&mut resource, &fields, project, true)?;
    Ok(resource)
}

/// 改一批既有的東西。
///
/// # 為什麼它以前是單數
///
/// 沒有理由，只是先寫了。而代價是：Agent 要補三十個位址就得走三十趟，
/// 每一趟都回一份完整的 lint 變化——那是三十份幾乎一樣的報告。
///
/// 現在跟 `create` 同一個形狀：一批一步，⌘Z 一次退掉。
///
/// # 為什麼要在複本上邊改邊解名字
///
/// 同 `create`。改了 slug 之後，同一批裡後面那一項指名的可能就是**新名字**
/// ——不在複本上跑的話，那一項會說找不到。
fn update(ws: &mut dyn Workspace, scope: &Scope, args: &Value) -> Result<String, String> {
    let items = items_of(args, &["target", "id"])?;
    let mut scratch = ws.project().unwrap().clone();
    let envs = target::environments(&scratch, scope)?;

    let mut edits = Vec::with_capacity(items.len());
    let mut changed = Vec::with_capacity(items.len());

    for (i, item) in items.iter().enumerate() {
        let at = |e: String| in_batch(i, items.len(), &e);
        // `id` 是舊寫法，照樣收。
        let text = opt_str(item, "target")
            .or_else(|| opt_str(item, "id"))
            .ok_or_else(|| at("少了必填欄位 target".into()))?;
        let fields = item.get("fields").cloned().unwrap_or_else(|| json!({}));

        let found = target::resolve(&scratch, &envs, text).map_err(at)?;
        changed.push(found.what(&scratch));

        let edit = match found {
            Target::Resource(resource) => {
                let mut resource = *resource;
                fill(&mut resource, &fields, &scratch, false).map_err(at)?;
                Edit::UpdateResource(resource)
            }
            Target::Connection(row) => connection_edits(&row, &fields).map_err(at)?,
        };

        loom_core::edit::apply(&mut scratch, &edit)
            .map_err(|e| in_batch(i, items.len(), &e.to_string()))?;
        edits.push(edit);
    }

    let done = format!("改好了 {} 個：\n  {}", changed.len(), changed.join("\n  "));
    with_lint_delta(ws, &Edit::Batch(edits), &done, dry_run(args))
}

/// 連線收得到的欄位。**唯一的一份**——[`connection_edits`] 照著擋，
/// 錯誤訊息照著列，說明書也照著印。
///
/// `memo` 放最後，跟每一張表把備註放最後一欄是同一個理由：
/// 它是自由文字，長度不受控，不該把有結構的欄位擠掉。
pub(crate) const CONNECTION_FIELDS: &[&str] =
    &["purpose", "kind", "from_expect", "to_expect", "memo"];

/// 改一條連線。
///
/// # 為什麼它不是 `UpdateResource`
///
/// 連線不是 [`Resource`]——它住在環境的 `connections` 裡，兩端是已經解析過的
/// [`Endpointing`](loom_core::environment::Endpointing)，沒有 slug 也沒有
/// 統一的編輯表單。所以核心給的是四支各自負責一件事的 `Edit`。
///
/// 這四支**本來就都在**（畫面上的 lint 面板按鈕用的就是它們），
/// 只是 MCP 沒接出來。於是 Agent 想把 `expect` 從 3 改成 6，唯一的做法是
/// 把整條連線刪掉重建——而重建會換一個新的 id，圖上綁著它的標註就斷了。
///
/// 一項可以同時改好幾個欄位，所以回的是一個 [`Edit::Batch`]。
fn connection_edits(row: &loom_core::table::Row, fields: &Value) -> Result<Edit, String> {
    let map = fields.as_object().ok_or("fields 要是一個物件")?;
    let unknown: Vec<&str> = map
        .keys()
        .map(String::as_str)
        .filter(|k| !CONNECTION_FIELDS.contains(k))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "連線認不得這些欄位：{}。它收的是：{}",
            unknown.join("、"),
            CONNECTION_FIELDS.join("、"),
        ));
    }

    let mut edits = Vec::new();

    // 備註每一種東西都收，所以在攤開連線自己那幾個欄位**之前**就先處理掉。
    // 理由同 `fill`：漏掉的那一種就是「寫得進、寫不進去」，而備註沒有規則
    // 會回報，所以那種缺口只會以「我明明寫了」的形式冒出來。
    if let Some(memo) = map.get("memo").and_then(Value::as_str) {
        edits.push(Edit::SetConnectionMemo {
            environment: row.environment.clone(),
            connection: row.id.clone(),
            memo: memo.to_string(),
        });
    }

    if let Some(purpose) = map.get("purpose").and_then(Value::as_str) {
        edits.push(Edit::SetPurpose {
            environment: Some(row.environment.clone()),
            subject: row.id.clone(),
            purpose: purpose.to_string(),
        });
    }

    if let Some(kind) = map.get("kind").and_then(Value::as_str) {
        edits.push(Edit::SetConnectionKind {
            environment: row.environment.clone(),
            connection: row.id.clone(),
            kind: match kind {
                "primary" => ConnectionKind::Primary,
                "fallback" => ConnectionKind::Fallback,
                other => {
                    return Err(format!(
                        "認不得的連線種類 {other}。可以用：primary（平常走的）、\
                         fallback（只在故障時走的）"
                    ));
                }
            },
        });
    }

    // 兩端分開講。一個 `expect` 猜不出是哪一端，而猜錯會安靜地改到另一端——
    // 那正是 `Finding::end` 存在的理由（一條連線的兩端都可能是萬用字元）。
    for (key, side) in [
        ("from_expect", ConnectionEnd::From),
        ("to_expect", ConnectionEnd::To),
    ] {
        let Some(v) = map.get(key) else { continue };
        let expect = match v {
            Value::Null => None,
            Value::Number(n) => Some(n.as_u64().ok_or_else(|| format!("{key} 要是正整數"))? as u32),
            _ => return Err(format!("{key} 要是數字，或 null（清掉）")),
        };
        edits.push(Edit::SetExpect {
            environment: row.environment.clone(),
            connection: row.id.clone(),
            side,
            expect,
        });
    }

    if edits.is_empty() {
        return Err(format!(
            "沒有指定要改什麼。連線收的是：{}",
            CONNECTION_FIELDS.join("、")
        ));
    }
    Ok(Edit::Batch(edits))
}

/// 刪除。**預設只給預覽，帶了 `confirm` 才真的動手。**
///
/// # 為什麼刪除是唯一要確認兩次的操作
///
/// 其他工具都在往專案裡加東西，看錯了大不了再刪掉。刪除相反，
/// 而 `cascade` 更是這個專案裡唯一一個「按一下消失幾十個元素」的操作。
///
/// 對一個賣點是「怕漏」的工具，讓 Agent 一句話掃掉半個模型是說不過去的。
fn delete(ws: &mut dyn Workspace, scope: &Scope, args: &Value) -> Result<String, String> {
    let project = ws.project().unwrap();
    let envs = target::environments(project, scope)?;

    // `targets` 是現在的寫法；`ids` 與單數的 `id` 是舊的，照樣收。
    let texts: Vec<String> = match args.get("targets").or_else(|| args.get("ids")) {
        Some(Value::Array(list)) if !list.is_empty() => list
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "targets 裡面要放字串".to_string())
            })
            .collect::<Result<_, _>>()?,
        _ => vec![
            opt_str(args, "target")
                .or_else(|| opt_str(args, "id"))
                .ok_or("少了必填欄位 targets（要是一個陣列）")?
                .to_string(),
        ],
    };

    let mut seeds = Vec::with_capacity(texts.len());
    let mut named = Vec::with_capacity(texts.len());
    for text in &texts {
        // 解析與「怎麼刪」分開：前者知道名字怎麼對到東西，後者知道刪一個
        // 東西要下什麼 Edit。`cascade::removal` 已經兩種都認得，所以這裡
        // 只負責把名字換成 id。
        let found = target::resolve(project, &envs, text)?;
        let r = cascade::removal(project, found.id())
            .ok_or_else(|| format!("{text} 對到的東西刪不掉。用 describe 看目前有什麼。"))?;
        named.push(r.what);
        seeds.push(r.edit);
    }

    let wants_cascade = args.get("cascade").and_then(Value::as_bool) == Some(true);
    let plan = if wants_cascade {
        cascade::plan(project, &seeds).map_err(|e| e.to_string())?
    } else {
        cascade::Plan::default()
    };

    let all: Vec<Edit> = seeds
        .iter()
        .cloned()
        .chain(plan.cascaded.iter().map(|r| r.edit.clone()))
        .collect();
    let batch = Edit::Batch(all);

    // 刪除的預覽不叫 `dry_run`，因為它的**預設方向相反**：其他工具預設動手，
    // 這一支預設只看。兩邊用同一個字的話，Agent 會以為預設也一樣，
    // 然後在其中一邊猜錯——而猜錯的那一邊是刪除。
    if args.get("confirm").and_then(Value::as_bool) != Some(true) {
        return Ok(delete_preview(
            project,
            &named,
            &plan,
            &batch,
            wants_cascade,
        ));
    }

    let mut done = format!("刪掉了 {} 個：\n  {}", named.len(), named.join("\n  "));
    if !plan.cascaded.is_empty() {
        done.push_str(&format!(
            "\n連帶刪掉 {} 個：\n  {}",
            plan.cascaded.len(),
            plan.cascaded
                .iter()
                .map(|r| r.what.as_str())
                .collect::<Vec<_>>()
                .join("\n  ")
        ));
    }
    with_lint_delta(ws, &batch, &done, false)
}

/// 「按下去會發生什麼」。什麼都不改。
fn delete_preview(
    project: &Project,
    named: &[String],
    plan: &cascade::Plan,
    batch: &Edit,
    wants_cascade: bool,
) -> String {
    let mut out = format!(
        "**預覽，還沒有刪任何東西。**\n\n指名要刪的 {} 個：\n  {}\n",
        named.len(),
        named.join("\n  ")
    );

    if wants_cascade {
        if plan.cascaded.is_empty() {
            out.push_str("\n沒有東西會連帶被刪。\n");
        } else {
            out.push_str(&format!(
                "\n會連帶刪掉的 {} 個：\n  {}\n",
                plan.cascaded.len(),
                plan.cascaded
                    .iter()
                    .map(|r| r.what.as_str())
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ));
        }
        for f in &plan.unresolved {
            out.push_str(&format!(
                "\n⚠ 掃不掉，要你自己處理：{} {}\n",
                f.rule.code(),
                f.detail
            ));
        }
    }

    // 影響分析直接問 lint，不另外寫一套判斷——「什麼叫弄壞了」只能有一份標準。
    match loom_core::edit::preview(project, batch) {
        Ok(impact) if impact.introduced.is_empty() => {
            out.push_str("\n刪完不會多出任何問題。");
        }
        Ok(impact) => {
            out.push_str(&format!(
                "\n刪完會多出 {} 項問題：",
                impact.introduced.len()
            ));
            for f in &impact.introduced {
                out.push_str(&format!(
                    "\n- {} {}{}",
                    f.rule.code(),
                    f.detail,
                    // 只有刪之前那份專案可用。分不出來時 fix_for 回 None，
                    // 退回一句通用的話——不會亂講。
                    query::how_to_fix(project, f)
                ));
            }
            if !wants_cascade {
                out.push_str(
                    "\n\n（這些多半是懸空的參照。想一次清乾淨就加 cascade: true 再預覽一次。）",
                );
            }
        }
        Err(e) => out.push_str(&format!("\n⚠ 這批刪除套用不了：{e}")),
    }

    out.push_str("\n\n確定的話，用同一組參數再送一次並加上 `confirm: true`。");
    out
}

/// 一次開好幾群機器。
///
/// # 為什麼它也要收 `items`
///
/// 六個服務各要六台，以前是六趟——每一趟都回一份完整的 lint 變化，
/// 而前五份講的都是「還有五個服務一台都沒建」。那是 Agent 已經知道的事。
///
/// 現在一批算完再回報一次，⌘Z 也是一次退掉整批。
fn create_nodes(ws: &mut dyn Workspace, scope: &Scope, args: &Value) -> Result<String, String> {
    let items = items_of(args, &["container"])?;
    let env_slug = env_from(ws.project().unwrap(), scope)?;

    let mut scratch = ws.project().unwrap().clone();
    let mut edits = Vec::with_capacity(items.len());
    let mut listing: Vec<String> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let at = |e: String| in_batch(i, items.len(), &e);
        // 每一項都對著複本解名字：第二群機器可能要放在第一群剛建好的站點底下。
        let edit = build_nodes(&scratch, &env_slug, item, &mut listing).map_err(at)?;
        loom_core::edit::apply(&mut scratch, &edit)
            .map_err(|e| in_batch(i, items.len(), &e.to_string()))?;
        edits.push(edit);
    }

    let done = format!(
        "在 {env_slug} 建好 {} 台：\n  {}",
        listing.len(),
        listing.join("\n  ")
    );
    with_lint_delta(ws, &Edit::Batch(edits), &done, dry_run(args))
}

/// 把一項機器規格變成一個 [`Edit::AddInstances`]。
fn build_nodes(
    project: &Project,
    env_slug: &str,
    item: &Value,
    listing: &mut Vec<String>,
) -> Result<Edit, String> {
    let env = project
        .environment(&env_id(project, env_slug)?)
        .ok_or("找不到環境")?
        .clone();

    let container_slug = str_field(item, "container")?;
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
    let endpoint = match opt_str(item, "endpoint") {
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
        count: u32_field(item, "count")?,
        name_template: opt_str(item, "name_template")
            .unwrap_or(&format!("{container_slug}-{{n}}"))
            .to_string(),
        node_template: opt_str(item, "node_template")
            .unwrap_or(&format!("vm-{container_slug}-{{n}}"))
            .to_string(),
        start: u32_or(item, "start", 1),
        pad: u32_or(item, "pad", 2),
        address_template: str_field(item, "address_template")?.to_string(),
        ip_start: u32_or(item, "ip_start", 1),
        node_kind: match opt_str(item, "node_kind") {
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

    let within = opt_str(item, "within")
        .map(|s| node_id(project, env_slug, s))
        .transpose()?;
    let plan = loom_core::batch::plan(project, &env, &spec).map_err(|e| e.to_string())?;
    listing.extend(plan.preview.iter().cloned());

    Ok(Edit::AddInstances {
        environment: env.id.clone(),
        within,
        nodes: plan.nodes,
    })
}

fn add_connection(ws: &mut dyn Workspace, scope: &Scope, args: &Value) -> Result<String, String> {
    let items = items_of(args, &["relationship"])?;
    let env_slug = env_from(ws.project().unwrap(), scope)?;

    let mut scratch = ws.project().unwrap().clone();
    let mut edits = Vec::with_capacity(items.len());
    let mut notes: Vec<String> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let at = |e: String| in_batch(i, items.len(), &e);
        let edit = build_connection(&scratch, &env_slug, item, &mut notes).map_err(at)?;
        // 同上：邊建邊套用，這樣「這一段的目標是剛剛那一段建出來的東西」也成立，
        // 而且失敗時知道是第幾項。
        loom_core::edit::apply(&mut scratch, &edit)
            .map_err(|e| in_batch(i, items.len(), &e.to_string()))?;
        edits.push(edit);
    }

    // 擬出來的東西要講清楚是**擬**的。`dry_run` 時這幾行就是以前
    // `propose_connection` 唯一的產出，只是現在一次給整批。
    let mut done = format!("在 {env_slug} 建好 {} 段連線。", edits.len());
    if !notes.is_empty() {
        done.push_str(&format!("\n擬的時候有幾句話：\n  {}", notes.join("\n  ")));
    }
    with_lint_delta(ws, &Edit::Batch(edits), &done, dry_run(args))
}

/// 環境層的寫入工具都要知道是哪個環境。
///
/// 只開著一個環境時可以省略——那是最常見的情況（大部分專案先只有 prod），
/// 多問一次只是噪音。有好幾個又不講就**不猜**：猜錯是把一整批連線建到
/// 別的環境去，而 lint 之後只會說「這個環境什麼都沒有」，
/// 完全指不回是哪一步錯了。
fn env_from(project: &Project, scope: &Scope) -> Result<String, String> {
    if let Some(slug) = &scope.environment {
        // 存不存在在這裡就確認掉，不然錯誤會晚到每一項各報一次。
        env_id(project, slug)?;
        return Ok(slug.clone());
    }
    match project.environments.as_slice() {
        [only] => Ok(only.slug.clone()),
        [] => Err("這個專案還沒有任何環境。先 create 一個 kind: \"environment\"。".into()),
        many => Err(format!(
            "有 {} 個環境，要說清楚是哪一個：加上 `scope`: {{\"environment\": \"…\"}}。有的是：{}",
            many.len(),
            slugs(many.iter().map(|e| &e.slug))
        )),
    }
}

/// 把一項連線規格變成一個 [`Edit::AddConnection`]。
fn build_connection(
    project: &Project,
    env_slug: &str,
    item: &Value,
    notes: &mut Vec<String>,
) -> Result<Edit, String> {
    let env = project
        .environment(&env_id(project, env_slug)?)
        .ok_or("找不到環境")?
        .clone();
    let rel_id = relationship_id(project, str_field(item, "relationship")?)?;
    let relationship = project
        .logical
        .relationships
        .iter()
        .find(|r| r.id == rel_id)
        .ok_or("找不到契約")?;
    let to_endpoint = relationship.to_endpoint.clone();

    let proposal = connect::propose(project, &env, &rel_id).ok_or("找不到契約")?;

    // 提案自己講的那幾句話（「目標有好幾台，用萬用字元」之類）。以前只有
    // `propose_connection` 說得出來，而那一支的存在理由就只是這幾行。
    // 現在跟著建出來的東西一起回報，`dry_run` 時它就是完整的提案。
    if opt_str(item, "from").is_none() || opt_str(item, "to").is_none() {
        notes.extend(
            proposal
                .notes
                .iter()
                .map(|n| format!("[{}] {n}", relationship.slug)),
        );
    }

    let from = match opt_str(item, "from") {
        Some(text) => refs::resolve(project, &env, text, None)?,
        None => proposal
            .from
            .clone()
            .ok_or_else(|| format!("擬不出來源，請自己指定 from。{}", proposal.notes.join(" ")))?,
    };
    let to = match opt_str(item, "to") {
        Some(text) => refs::resolve(project, &env, text, Some(&to_endpoint))?,
        None => proposal
            .to
            .clone()
            .ok_or_else(|| format!("擬不出目標，請自己指定 to。{}", proposal.notes.join(" ")))?,
    };

    Ok(Edit::AddConnection {
        environment: env.id.clone(),
        id: proposal.id,
        serves: rel_id,
        purpose: opt_str(item, "purpose")
            .map(str::to_string)
            .unwrap_or(proposal.purpose),
        kind: if item.get("fallback").and_then(Value::as_bool) == Some(true) {
            ConnectionKind::Fallback
        } else {
            ConnectionKind::Primary
        },
        from,
        to,
    })
}

// ── 把 fields 填進 Resource ──────────────────────────────────────

/// 只動有填的欄位。沒填的維持原樣——`update` 就靠這個做到「只改一部分」。
/// 每種資源收哪些欄位。**唯一的一份**——`fill` 照著擋，錯誤訊息照著列。
///
/// 順序跟 `list()` 裡的說明一致，這樣 Agent 看到的兩份是同一份。
pub(crate) fn accepted_fields(resource: &Resource) -> &'static [&'static str] {
    match resource {
        Resource::Person(_) => &["slug", "name", "memo"],
        Resource::System(_) => &["slug", "name", "external", "memo"],
        Resource::Container(_) => &["slug", "name", "system", "memo"],
        Resource::EndpointDef { .. } => &["owner", "slug", "protocol", "memo"],
        Resource::Relationship(_) => &["slug", "purpose", "from", "to", "to_endpoint", "memo"],
        Resource::Environment(_) => &["slug", "name", "memo"],
        Resource::Node { .. } => &["environment", "slug", "kind", "within", "memo"],
        Resource::Infra { .. } => &["environment", "slug", "memo"],
        Resource::InfraEndpoint { .. } => &[
            "environment",
            "owner",
            "slug",
            "address",
            "protocol",
            "memo",
        ],
        Resource::Instance { .. } => &[
            "environment",
            "node",
            "slug",
            "container",
            "standalone",
            "address",
            "endpoint",
            "memo",
        ],
        Resource::SystemInstance { .. } => &[
            "environment",
            "slug",
            "system",
            "standalone",
            "address",
            "endpoint",
            "memo",
        ],
    }
}

/// 把值填進去。
///
/// # ⚠️ 不認得的欄位一定要報錯
///
/// 這裡原本只讀認得的鍵，其餘**靜靜忽略**。於是 Agent 打錯一個字
/// （`adress`、`standalon`、給機器一個 `name`）就是：欄位沒進去、
/// 沒有任何錯誤、回一句「建好了」。
///
/// 對人來說那只是要重來一次；對 Agent 來說它會**照著往下走**，
/// 而錯誤要到很後面才以「lint 說缺位址」的形式冒出來，那時它已經
/// 完全不知道是哪一步的問題了。
///
/// 這也正是這個 crate 開頭寫的原則：錯誤一律附上候選。
/// 只有 `create` 讀得到的欄位。
///
/// # 為什麼要單獨列一份
///
/// 這些欄位由 `build_resource` 消化掉，`fill` 根本沒看。於是 `update` 帶了
/// 它們就是**安靜地什麼都不做**——回一句「改好了」，而東西沒動。
///
/// 那正是 `fill` 開頭那段註解在講的坑，只是換了個位置：一個是打錯字，
/// 一個是拼對了但這條路不通。對 Agent 來說症狀一樣——它會照著往下走。
///
/// 搬家目前真的做不到（換環境要連帶處理連線、換擁有者等於換一個東西），
/// 所以這裡的答案是**講清楚**，不是假裝做得到。
pub(crate) fn create_only_fields(resource: &Resource) -> &'static [&'static str] {
    match resource {
        Resource::EndpointDef { .. } => &["owner"],
        Resource::Node { .. } => &["environment", "within"],
        Resource::Infra { .. } | Resource::Instance { .. } | Resource::SystemInstance { .. } => {
            &["environment"]
        }
        Resource::InfraEndpoint { .. } => &["environment", "owner"],
        _ => &[],
    }
}

fn fill(
    resource: &mut Resource,
    fields: &Value,
    project: &Project,
    is_new: bool,
) -> Result<(), String> {
    if let Some(map) = fields.as_object() {
        if !is_new {
            let stuck: Vec<&str> = create_only_fields(resource)
                .iter()
                .copied()
                .filter(|k| map.contains_key(*k))
                .collect();
            if !stuck.is_empty() {
                return Err(format!(
                    "{}建立之後就搬不動了，改不了這些欄位：{}。\
                     要換位置的話：在新的地方 create 一個，再把舊的 delete 掉。",
                    resource.kind_name(),
                    stuck.join("、"),
                ));
            }
        }

        let accepted = accepted_fields(resource);
        // `kind` 是 create 用來挑資源種類的，不是欄位；它會跟著整包送進來。
        let unknown: Vec<&str> = map
            .keys()
            .map(String::as_str)
            .filter(|k| *k != "kind" && !accepted.contains(k))
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "{} 認不得這些欄位：{}。它收的是：{}",
                resource.kind_name(),
                unknown.join("、"),
                accepted.join("、"),
            ));
        }
    }

    let s = |k: &str| opt_str(fields, k).map(str::to_string);

    // 備註每種資源都有，所以在攤開變體之前先處理掉——
    // 不然下面十一個分支每個都要再抄一次同一段。
    if let Some(v) = s("memo") {
        *resource.memo_mut() = v;
    }

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
        Resource::Instance {
            environment,
            node,
            instance,
        } => {
            if let Some(v) = s("slug") {
                instance.slug = v
            }
            if let Some(v) = s("container") {
                instance.container = container_id(project, &v)?
            }
            if let Some(v) = fields.get("standalone").and_then(Value::as_bool) {
                instance.standalone = v
            }
            // 跑在哪台機器上。**建立與搬家是同一行。**
            //
            // 一台機器上跑好幾個服務是常態（一台 VM 上有 app 也有 agent），
            // 而在這之前 MCP 只有 create_nodes，它一定會替每個服務開新機器。
            // 於是 Agent 建得出來的模型，跟使用者的機房長得不一樣。
            if let Some(v) = s("node") {
                *node = node_id_in(project, environment, &v)?;
            }
            if let Some(address) = s("address") {
                set_instance_address(instance, project, &address, s("endpoint").as_deref())?;
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
            // 位址。**這是這種元素存在的唯一理由**——「金流在 prod 打哪個
            // 位址」。少了它，Agent 建出來的是一個空殼，而 lint 會立刻叫
            // L001，它卻沒有任何工具解得掉。
            if let Some(address) = s("address") {
                set_system_address(instance, project, &address, s("endpoint").as_deref())?;
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

/// 把位址寫到外部系統實體身上。
///
/// # 為什麼位址不是一個欄位，而要經過接點
///
/// 位址住在 `Endpoint` 上，外部系統實體與服務實體一樣。所以 `address:` 是
/// 一層方便的說法——底下要落到某一個接點。
///
/// 一個外部系統通常只有一兩個接點（一個 API 而已），所以省略 `endpoint`
/// 時就挑它唯一的那一個。有好幾個又不講的話**不猜**，把名字列出來——
/// 猜錯的話位址會落到錯的接點上，而那條連線接不上時症狀是「明明填了位址
/// 卻還是說缺位址」，非常難查。
///
/// # `def` 從對應的那個**系統**身上找
///
/// 服務實體看它對應的服務，外部系統實體看它對應的系統。這個不對稱是模型
/// 定義的（見 `docs/domain-model.md`），也是這一段最容易寫錯的地方。
fn set_system_address(
    instance: &mut loom_core::environment::SoftwareSystemInstance,
    project: &Project,
    address: &str,
    endpoint: Option<&str>,
) -> Result<(), String> {
    let system = project
        .logical
        .systems
        .iter()
        .find(|s| s.id == instance.system)
        .ok_or("這個外部系統實體還沒有對應到任何系統，先填 system")?;

    put_address(
        &mut instance.endpoints,
        &system.slug,
        &system.endpoints,
        address,
        endpoint,
    )
}

/// 把位址寫到服務實體身上。
///
/// 跟外部系統實體同一件事，只差 `def` 是從**對應的那個服務**身上找的
/// （不對稱是模型定義的，見 `docs/domain-model.md`）。
fn set_instance_address(
    instance: &mut loom_core::environment::ContainerInstance,
    project: &Project,
    address: &str,
    endpoint: Option<&str>,
) -> Result<(), String> {
    let container = project
        .logical
        .containers
        .iter()
        .find(|c| c.id == instance.container)
        .ok_or("這個服務實體還沒有對應到任何服務，先填 container")?;

    put_address(
        &mut instance.endpoints,
        &container.slug,
        &container.endpoints,
        address,
        endpoint,
    )
}

/// 兩種實體共用的那一段：挑出接點定義，然後把位址放上去。
///
/// 寫成一份是因為「有好幾個定義的時候不猜」這條規矩兩邊都要成立。
/// 抄成兩份的話，總有一天只有一邊被改到，而另一邊會安靜地開始猜。
fn put_address(
    endpoints: &mut Vec<loom_core::environment::Endpoint>,
    owner_slug: &str,
    defs: &[loom_core::logical::EndpointDef],
    address: &str,
    endpoint: Option<&str>,
) -> Result<(), String> {
    let def = match (endpoint, defs) {
        (Some(want), _) => defs.iter().find(|d| d.slug == want).ok_or_else(|| {
            format!(
                "{owner_slug} 身上沒有接點定義 {want}。有的是：{}",
                slugs(defs.iter().map(|d| &d.slug))
            )
        })?,
        (None, [only]) => only,
        (None, []) => {
            return Err(format!(
                "{owner_slug} 還沒有定義任何接點，位址無處可放。\
                 先用 create 建一個 endpoint_def（owner 填 {owner_slug}）。"
            ));
        }
        (None, many) => {
            return Err(format!(
                "{owner_slug} 有好幾個接點定義，請用 `endpoint` 指定要填哪一個：{}",
                slugs(many.iter().map(|d| &d.slug))
            ));
        }
    };

    // 已經有對應到這個定義的接點就改它，沒有才新增——不然重跑一次
    // update 會長出第二個位址一樣的接點。
    match endpoints
        .iter_mut()
        .find(|e| e.def.as_ref() == Some(&def.id))
    {
        Some(existing) => existing.address = Some(address.into()),
        None => endpoints.push(loom_core::environment::Endpoint {
            id: Id::generate(),
            slug: def.slug.clone(),
            def: Some(def.id.clone()),
            protocol: def.protocol,
            address: Some(address.into()),
            memo: String::new(),
        }),
    }
    Ok(())
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
    node_id_in(project, &env_id(project, env_slug)?, slug)
}

/// 同上，但環境已經是 id 了——`fill` 手上拿到的資源自己就帶著環境。
fn node_id_in(project: &Project, environment: &Id, slug: &str) -> Result<Id, String> {
    let env = project.environment(environment).ok_or("找不到環境")?;
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
    walk(&env.nodes, slug).ok_or_else(|| {
        // 附上候選。打錯一個字跟「那台機器還沒建」是兩件完全不同的事，
        // 而 Agent 分不出來就會走上完全不同的一條路。
        format!(
            "環境 {} 裡找不到機器 {slug}。有的是：{}",
            env.slug,
            slugs(all_node_slugs(&env.nodes).iter())
        )
    })
}

fn all_node_slugs(nodes: &[loom_core::environment::DeploymentNode]) -> Vec<&String> {
    let mut out = Vec::new();
    for n in nodes {
        out.push(&n.slug);
        out.extend(all_node_slugs(&n.children));
    }
    out
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

/// 取出 `items` 陣列。
///
/// # 為什麼還收舊的單數寫法
///
/// 工具描述只教 `items`——兩種寫法都寫上去只會讓 Agent 猶豫。
/// 但**收**還是兩種都收：模型有時會憑印象送出舊形狀，那時候與其
/// 回一個它得重試才懂的錯誤，不如照做。`markers` 是判斷的依據：
/// 頂層有其中任何一個欄位就當成「只有一項」。
///
/// 收好幾個 marker 是因為欄位自己也改過名（`update` 的 `id` → `target`）。
/// 舊 Agent 手上那份設定寫的是舊名字，而它送出來的那一趟**看起來完全正常**
/// ——少了這個的話它會拿到「少了 items」，然後把整個請求重寫一次。
/// 第一個 marker 是現在教的那個，錯誤訊息只提它。
pub(crate) fn items_of(args: &Value, markers: &[&str]) -> Result<Vec<Value>, String> {
    match args.get("items") {
        Some(Value::Array(list)) if !list.is_empty() => Ok(list.clone()),
        Some(Value::Array(_)) => Err("items 是空的，沒有東西可以做".into()),
        Some(_) => Err("items 要是一個陣列".into()),
        None if markers.iter().any(|m| args.get(m).is_some()) => Ok(vec![args.clone()]),
        None => Err(format!(
            "少了必填欄位 items（要是一個陣列，每一項至少要有 {}）",
            markers[0]
        )),
    }
}

/// 批次裡第幾項失敗了。
///
/// 序號從 1 開始數給人看。少了它，Agent 送三百項進來只會知道「撞名了」，
/// 而它得自己把三百項跟專案比對一遍才找得到是哪一個。
fn in_batch(index: usize, total: usize, cause: &str) -> String {
    if total == 1 {
        return cause.to_string();
    }
    format!(
        "第 {} 項（共 {total} 項）失敗了：{cause}\n**整批都沒有建。** 改掉那一項再整批送一次。",
        index + 1
    )
}

fn relationship_id(project: &Project, want: &str) -> Result<Id, String> {
    project
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
        })
}

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
