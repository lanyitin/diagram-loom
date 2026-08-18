//! Agent 怎麼**看**這份專案：`describe` 與 `lint` 的參數與輸出。
//!
//! # 為什麼讀取要能篩、能挑詳細度
//!
//! Agent 每讀一次就付一次 token。而它想知道的事幾乎都很小——
//! 「prod 的 redis 建好了沒」「還剩幾個錯誤」——但在這之前它只有
//! 「把整份專案倒出來」一種問法。數百條連線的專案，每問一次是幾萬字，
//! 而它要的是一行。
//!
//! 所以讀取工具一律 `{scope, select, detail}`：
//!
//! - `scope`  哪個專案、哪個環境（**環境只有這一個地方可以寫**）
//! - `select` 篩什麼
//! - `detail` 要多細：[`Detail::Count`] / [`Detail::Brief`] / [`Detail::Full`]
//!
//! 三個欄位在 `describe` 與 `lint` 上是同一個意思，所以 Agent 學一次用兩處。
//!
//! # 預設是 [`Detail::Brief`]，而 brief **不含 id**
//!
//! 這在以前會是災難：`update` 與 `delete` 只吃 id，拿不到 id 就什麼都改不了。
//! 現在兩者都吃名字（見 [`crate::refs`]），於是 id 從「每次都要」變成
//! 「幾乎不用」——一列省下來的那 36 個字元乘上幾百列，是這一層最大的一筆。
//!
//! 真的要 id 就 `detail: "full"`，工具描述裡有寫。

use serde_json::Value;

use loom_core::id::Id;
use loom_core::inventory;
use loom_core::lint::{Finding, Rule, Severity, lint};
use loom_core::resource::Kind;
use loom_core::table;
use loom_core::{Project, pattern};

// ── kind 的叫法 ─────────────────────────────────────────────────

/// MCP 這一層對每一種東西的叫法。**唯一的一份。**
///
/// `create` 的 `kind`、`select.kinds`、以及錯誤訊息裡列出來的候選全部照這裡。
/// 抄成好幾份的話，遲早有一份會漏掉新加的那一種——而症狀是「這個名字
/// 建得出來卻篩不到」，Agent 只會覺得莫名其妙。
///
/// 順序照 [`inventory::tables`] 的相依順序：從頭讀到尾就是一份
/// 「從零開始怎麼建」的說明書。`connection` 排最後，因為它要等兩端都在。
pub const KINDS: &[&str] = &[
    "person",
    "system",
    "container",
    "endpoint_def",
    "relationship",
    "environment",
    "node",
    "infra",
    "infra_endpoint",
    "instance",
    "system_instance",
    "connection",
];

/// 核心的 [`Kind`] 在這一層叫什麼。
pub fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::EndpointDef => "endpoint_def",
        Kind::Relationship => "relationship",
        Kind::Environment => "environment",
        Kind::Node => "node",
        Kind::Infra => "infra",
        Kind::InfraEndpoint => "infra_endpoint",
        Kind::Instance => "instance",
        Kind::SystemInstance => "system_instance",
    }
}

/// 反過來：Agent 寫的名字 → 核心的 [`Kind`]。
///
/// `connection` 認得，但會被擋下來並指路——連線不是 [`Resource`] 家族的一員，
/// 它由 `add_connection` 建。回一句「認不得的 kind」會讓 Agent 以為自己
/// 打錯字，然後換個拼法再試一次。
///
/// [`Resource`]: loom_core::resource::Resource
pub fn kind_of(name: &str) -> Result<Kind, String> {
    match name {
        "person" => Ok(Kind::Person),
        "system" => Ok(Kind::System),
        "container" => Ok(Kind::Container),
        "endpoint_def" => Ok(Kind::EndpointDef),
        "relationship" => Ok(Kind::Relationship),
        "environment" => Ok(Kind::Environment),
        "node" => Ok(Kind::Node),
        "infra" => Ok(Kind::Infra),
        "infra_endpoint" => Ok(Kind::InfraEndpoint),
        "instance" => Ok(Kind::Instance),
        "system_instance" => Ok(Kind::SystemInstance),
        "connection" => Err("連線不是用 create 建的，用 add_connection——\
             它的兩端要用名字解析，跟其他資源不一樣。"
            .into()),
        other => Err(format!(
            "認不得的 kind：{other}。可以用：{}",
            KINDS.join("、")
        )),
    }
}

// ── scope ───────────────────────────────────────────────────────

/// 「動哪一個專案、哪一個環境」。
///
/// # 為什麼環境只能寫在這裡
///
/// 以前它有三個家：`create` 藏在每一項的 `fields` 裡、`add_connection` 與
/// `create_nodes` 放頂層、`describe` 根本不收。於是 Agent 每碰一支沒用過的
/// 工具，都得先讀完整段描述才知道這一支是哪一種寫法。
///
/// 收斂成一個地方之後，「環境寫哪裡」這個問題只有一個答案。
///
/// 舊的寫法照樣收（見 [`Scope::of`]），但工具描述只教新的——
/// 兩種都寫上去只會讓模型猶豫。
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// 環境的 slug。`None` 代表「全部環境」（讀）或「每一項自己講」（寫）。
    pub environment: Option<String>,
}

impl Scope {
    /// 從參數裡挖出 scope。
    ///
    /// 順序是 `scope.environment` → 頂層 `environment`。後者是舊形狀，
    /// 收它的理由跟 [`crate::tools`] 收單數 `items` 一樣：模型有時會憑印象
    /// 送出舊的寫法，那時候回一個它得重試才懂的錯誤，不如照做。
    pub fn of(args: &Value) -> Scope {
        let from_scope = args
            .get("scope")
            .and_then(|s| s.get("environment"))
            .and_then(Value::as_str);
        let legacy = args.get("environment").and_then(Value::as_str);
        Scope {
            environment: from_scope
                .or(legacy)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        }
    }
}

/// Agent 說要動哪一個專案。跟 [`Scope`] 分開是因為挑專案要看**全部**開著的，
/// 那發生在工具被呼叫之前（見 [`crate::pick`]）。
pub fn wanted_project(args: &Value) -> Option<&str> {
    args.get("scope")
        .and_then(|s| s.get("project"))
        .and_then(Value::as_str)
        .or_else(|| args.get("project").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

// ── detail ──────────────────────────────────────────────────────

/// 要多細。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// 只有數字。「改完了嗎」用這一檔——一行字取代整份報告。
    Count,
    /// 看得出是什麼、缺什麼，但**不含 id**，也不展開萬用字元的每個位址。
    /// 預設。
    Brief,
    /// 全部：id、位址、每一項的 subject。要拿 id 的時候用。
    Full,
}

impl Detail {
    pub fn of(args: &Value) -> Result<Detail, String> {
        match args.get("detail").and_then(Value::as_str) {
            None | Some("") | Some("brief") => Ok(Detail::Brief),
            Some("count") => Ok(Detail::Count),
            Some("full") => Ok(Detail::Full),
            Some(other) => Err(format!(
                "認不得的 detail：{other}。可以用：count（只有數字）、\
                 brief（預設，不含 id）、full（含 id 與位址）"
            )),
        }
    }

    fn wants_id(self) -> bool {
        self == Detail::Full
    }
}

// ── select ──────────────────────────────────────────────────────

/// 篩什麼。兩個條件都省略就是全部。
#[derive(Debug, Clone, Default)]
pub struct Select {
    /// 只要這幾種。名字照 [`KINDS`]。
    kinds: Option<Vec<String>>,
    /// slug 要符合的樣式，`*` 可比對零到多個字元——跟連線兩端用的是
    /// **同一套語法**（[`loom_core::pattern`]），Agent 不必學第二種。
    pattern: Option<String>,
    /// 只要這幾條規則。只有 `lint` 會用到。
    rules: Option<Vec<Rule>>,
    /// 至少這麼嚴重。只有 `lint` 會用到。
    least: Option<Severity>,
}

impl Select {
    pub fn of(args: &Value) -> Result<Select, String> {
        let select = match args.get("select") {
            None | Some(Value::Null) => return Ok(Select::default()),
            Some(Value::Object(_)) => args.get("select").unwrap(),
            Some(_) => return Err("select 要是一個物件".into()),
        };

        let kinds = match select.get("kinds") {
            None | Some(Value::Null) => None,
            Some(Value::Array(list)) => {
                let mut out = Vec::with_capacity(list.len());
                for v in list {
                    let name = v.as_str().ok_or("select.kinds 裡面要放字串")?;
                    if !KINDS.contains(&name) {
                        return Err(format!(
                            "select.kinds 認不得 {name}。可以用：{}",
                            KINDS.join("、")
                        ));
                    }
                    out.push(name.to_string());
                }
                Some(out)
            }
            Some(_) => return Err("select.kinds 要是一個陣列".into()),
        };

        let rules = match select.get("rules") {
            None | Some(Value::Null) => None,
            Some(Value::Array(list)) => {
                let mut out = Vec::with_capacity(list.len());
                for v in list {
                    let code = v.as_str().ok_or("select.rules 裡面要放字串")?;
                    out.push(rule_of(code)?);
                }
                Some(out)
            }
            Some(_) => return Err("select.rules 要是一個陣列".into()),
        };

        let least = match select.get("severity").and_then(Value::as_str) {
            None | Some("") => None,
            Some("error") => Some(Severity::Error),
            Some("warning") => Some(Severity::Warning),
            Some("info") => Some(Severity::Info),
            Some(other) => {
                return Err(format!(
                    "認不得的 severity：{other}。可以用：error、warning、info"
                ));
            }
        };

        Ok(Select {
            kinds,
            pattern: select
                .get("match")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            rules,
            least,
        })
    }

    fn wants_kind(&self, key: &str) -> bool {
        match &self.kinds {
            None => true,
            Some(list) => list.iter().any(|k| k == key),
        }
    }

    fn wants_slug(&self, slug: &str) -> bool {
        match &self.pattern {
            None => true,
            Some(p) => pattern::matches(p, slug),
        }
    }

    fn wants_finding(&self, f: &Finding) -> bool {
        if let Some(rules) = &self.rules
            && !rules.contains(&f.rule)
        {
            return false;
        }
        if let Some(least) = self.least
            && f.severity() < least
        {
            return false;
        }
        true
    }

    /// 篩到零筆時要說「是篩掉的，還是真的沒有」。
    ///
    /// 少了這一句，Agent 看到空的結果會直接下結論「這個環境什麼都沒有」，
    /// 然後開始重建已經存在的東西。
    fn narrowed(&self) -> bool {
        self.kinds.is_some()
            || self.pattern.is_some()
            || self.rules.is_some()
            || self.least.is_some()
    }
}

fn rule_of(code: &str) -> Result<Rule, String> {
    const ALL: &[Rule] = &[
        Rule::L001,
        Rule::L002,
        Rule::L003,
        Rule::L004,
        Rule::L005,
        Rule::L006,
        Rule::L007,
        Rule::L008,
        Rule::L012,
        Rule::L013,
        Rule::L014,
    ];
    ALL.iter()
        .copied()
        .find(|r| r.code() == code)
        .ok_or_else(|| {
            format!(
                "認不得的規則代號 {code}。有的是：{}",
                ALL.iter().map(|r| r.code()).collect::<Vec<_>>().join("、")
            )
        })
}

// ── describe ────────────────────────────────────────────────────

/// 這份專案裡有什麼。
///
/// # 為什麼要跑遍每一個環境
///
/// 這裡原本寫死 `environments.first()`，於是三個環境的專案，Agent 只看得到
/// 第一個的機器與服務實體。而 `find_resource` 是走**全部**環境找的——
/// 所以它建得出 uat 的東西、改得動 uat 的東西，就是**看不到**。
///
/// 那種缺口最難發現：沒有錯誤訊息，Agent 只會以為 uat 是空的，
/// 然後把已經存在的東西再建一次，直到撞名才知道出事。
pub fn describe(project: &Project, scope: &Scope, select: &Select, detail: Detail) -> String {
    let mut out = format!("專案：{}（{}）\n", project.name, project.slug);

    let envs = chosen_environments(project, scope);
    if let Err(e) = &envs {
        return e.clone();
    }
    let envs = envs.unwrap();

    if project.environments.is_empty() {
        out.push_str("環境：一個都還沒有。沒有環境的話 lint 不會有話說——邏輯層自己不會缺什麼。\n");
    }

    let mut shown = 0usize;

    // 邏輯層（母版）只有一份，所以在環境的迴圈外面印一次。
    for table in inventory::tables(project, None) {
        shown += write_table(&mut out, &table, None, select, detail);
    }

    for env in &envs {
        out.push_str(&format!("\n# 環境 {}（{}）\n", env.slug, env.name));
        // 每個環境各叫一次。裡面會重跑一次 lint，所以環境很多時這裡會變慢——
        // 但正確性優先，而 `select.kinds` 已經給了 Agent 一條省下來的路。
        for table in inventory::tables(project, Some(&env.id)) {
            if table.environment.is_none() {
                continue; // 邏輯層與專案層的表上面印過了。
            }
            shown += write_table(&mut out, &table, Some(&env.slug), select, detail);
        }
        shown += write_connections(&mut out, project, env, select, detail);
    }

    if shown == 0 && select.narrowed() {
        out.push_str(
            "\n（沒有任何東西符合 select。**這不代表專案是空的**——把 select 拿掉再看一次。）\n",
        );
    }
    out
}

/// 要看哪幾個環境。`scope.environment` 省略時是全部。
fn chosen_environments<'a>(
    project: &'a Project,
    scope: &Scope,
) -> Result<Vec<&'a loom_core::environment::Environment>, String> {
    match &scope.environment {
        None => Ok(project.environments.iter().collect()),
        Some(slug) => project
            .environments
            .iter()
            .find(|e| &e.slug == slug)
            .map(|e| vec![e])
            .ok_or_else(|| {
                let mut all: Vec<&str> = project
                    .environments
                    .iter()
                    .map(|e| e.slug.as_str())
                    .collect();
                all.sort();
                format!(
                    "找不到環境 {slug}。有的是：{}",
                    if all.is_empty() {
                        "（一個都沒有）".to_string()
                    } else {
                        all.join("、")
                    }
                )
            }),
    }
}

/// 印一張表，回傳實際印出幾列。
///
/// 標題帶上環境（`## 機器 @ prod`）是必要的：全部環境一起看的時候，
/// 光看「機器」那一段分不出是誰的。
fn write_table(
    out: &mut String,
    table: &inventory::Table,
    env_slug: Option<&str>,
    select: &Select,
    detail: Detail,
) -> usize {
    let key = kind_key(table.kind);
    if !select.wants_kind(key) {
        return 0;
    }

    let rows: Vec<&inventory::ResourceRow> = table
        .rows
        .iter()
        .filter(|r| select.wants_slug(r.resource.slug()))
        .collect();

    let title = match env_slug {
        Some(env) => format!("{} @ {env}", table.title),
        None => table.title.clone(),
    };
    out.push_str(&format!("\n## {title}（{}）\n", rows.len()));

    if detail == Detail::Count {
        return rows.len();
    }
    if rows.is_empty() {
        // 篩掉的跟本來就沒有是兩件事。前者給提示只會誤導。
        if select.narrowed() {
            out.push_str("（沒有符合的）\n");
        } else {
            out.push_str(&format!("（還沒有）{}\n", table.empty_hint));
        }
        return 0;
    }

    let head = if detail.wants_id() {
        format!("| id | {} |\n", table.columns.join(" | "))
    } else {
        format!("| {} |\n", table.columns.join(" | "))
    };
    out.push_str(&head);
    for row in &rows {
        if detail.wants_id() {
            out.push_str(&format!("| {} | {} |\n", row.id, row.cells.join(" | ")));
        } else {
            out.push_str(&format!("| {} |\n", row.cells.join(" | ")));
        }
    }
    rows.len()
}

/// 連線表。
///
/// # 為什麼以前沒有
///
/// `inventory::tables` 產出十張表，沒有連線——那份是給畫面的**資源**分頁用的，
/// 而連線在畫面上是另一個檢視（[`table::rows`]）。MCP 只接了前者。
///
/// 後果是 Agent 看不到專案裡數量最多、也最是這個工具賣點的那種東西。
/// 它想改一條既有連線，唯一的入口是 lint 報出來的那幾條。
///
/// 這裡直接接 [`table::rows`]，不自己再走一次萬用字元展開——那是規則，
/// 抄第二份就會跟畫面漂移。
fn write_connections(
    out: &mut String,
    project: &Project,
    env: &loom_core::environment::Environment,
    select: &Select,
    detail: Detail,
) -> usize {
    if !select.wants_kind("connection") {
        return 0;
    }

    // `rows` 一次算完所有環境，這裡只挑這一個的。
    let rows: Vec<table::Row> = table::rows(project)
        .into_iter()
        .filter(|r| r.environment == env.id)
        .filter(|r| {
            // 連線沒有自己的 slug，所以 `match` 比對的是它兩端的名字——
            // 「redis 那幾條」正是 Agent 會想問的問法。
            select.wants_slug(&r.from.label) || select.wants_slug(&r.to.label)
        })
        .collect();

    out.push_str(&format!("\n## 連線 @ {}（{}）\n", env.slug, rows.len()));
    if detail == Detail::Count {
        return rows.len();
    }
    if rows.is_empty() {
        if select.narrowed() {
            out.push_str("（沒有符合的）\n");
        } else {
            out.push_str(
                "（還沒有）連線是「這個環境真的接起來的那一段」。\
                 契約在這裡沒有連線的話，lint 會報 L001。\n",
            );
        }
        return 0;
    }

    // 備註放最後一欄，跟每一張資源表一樣（見 `inventory::tables`）：
    // 它是自由文字，長度不受控，不該把有結構的欄位擠出去。
    //
    // **而且它一定要印。** 備註是這個模型裡唯一沒有規則會回報的欄位，
    // 所以這裡是 Agent 唯一看得到「我剛剛寫的那句話存進去了」的地方。
    let head = if detail.wants_id() {
        "| id | 契約 | 用途 | 種類 | 來源 | 目標 | 狀態 | 備註 |\n"
    } else {
        "| 契約 | 用途 | 種類 | 來源 | 目標 | 狀態 | 備註 |\n"
    };
    out.push_str(head);

    for row in &rows {
        // 找不到對應契約本身就是 L003，要看得出來——印一個空格的話，
        // Agent 會以為那一欄只是沒填。
        let serves = row.serves_slug.as_deref().unwrap_or("⚠ 契約不見了");
        let kind = match row.kind {
            loom_core::environment::ConnectionKind::Primary => "正常",
            loom_core::environment::ConnectionKind::Fallback => "備援",
        };
        let state = if row.rules.is_empty() {
            String::new()
        } else {
            row.rules
                .iter()
                .map(|r| r.code())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let body = format!(
            "{serves} | {} | {kind} | {} | {} | {state} | {}",
            row.purpose,
            side_text(&row.from, detail),
            side_text(&row.to, detail),
            row.memo,
        );
        if detail.wants_id() {
            out.push_str(&format!("| {} | {body} |\n", row.id));
        } else {
            out.push_str(&format!("| {body} |\n"));
        }
    }
    rows.len()
}

/// 連線的一端印成一格。
///
/// 萬用字元要印出「展開成幾台、期望幾台」——那正是 L004 在看的東西，
/// 少了它 Agent 得再跑一次 lint 才知道這一條夠不夠。
///
/// 位址只在 [`Detail::Full`] 印：一個 `redis-*` 可以展開成六個位址，
/// 而 Agent 讀連線表時要的是「接得上嗎」，不是每一台的 IP。
fn side_text(side: &table::Side, detail: Detail) -> String {
    let mut out = side.label.clone();
    if let Some(ep) = &side.endpoint {
        out.push_str(&format!(" : {ep}"));
    }
    if side.label.contains('*') {
        match side.expect {
            Some(n) => out.push_str(&format!("（{} 台／期望 {n}）", side.matched)),
            None => out.push_str(&format!("（{} 台／沒填期望）", side.matched)),
        }
    }
    if detail.wants_id() && !side.addresses.is_empty() {
        out.push_str(&format!(" [{}]", side.addresses.join(" ")));
    }
    out
}

// ── lint ────────────────────────────────────────────────────────

/// 這份專案還缺什麼。
pub fn report(project: &Project, scope: &Scope, select: &Select, detail: Detail) -> String {
    let envs = match chosen_environments(project, scope) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // `scope.environment` 有指定時，只留那個環境的發現。邏輯層的發現
    // （`environment` 為 `None`）**不受環境篩選影響**——它對每個環境都成立，
    // 藏起來的話 Agent 會在某個環境裡找不到病因。
    let only: Option<Vec<&Id>> = scope
        .environment
        .as_ref()
        .map(|_| envs.iter().map(|e| &e.id).collect());

    let findings: Vec<Finding> = lint(project)
        .into_iter()
        .filter(|f| match (&only, &f.environment) {
            (Some(ids), Some(env)) => ids.contains(&env),
            _ => true,
        })
        .filter(|f| select.wants_finding(f))
        .collect();

    if findings.is_empty() {
        return if select.narrowed() || scope.environment.is_some() {
            "沒有符合的發現。（有篩選條件——這不代表整份專案是乾淨的。）".into()
        } else {
            "沒有發現任何缺漏。".into()
        };
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity() == Severity::Error)
        .count();
    let head = format!("{} 個錯誤、{} 個警告", errors, findings.len() - errors);
    if detail == Detail::Count {
        return head;
    }

    let mut out = format!("{head}：\n");
    for f in &findings {
        let env = f
            .environment
            .as_ref()
            .and_then(|id| project.environment(id))
            .map(|e| e.slug.as_str())
            .unwrap_or("邏輯層");
        // subject 是 UUID，一列 36 個字元。`update` 與 `delete` 都吃名字之後
        // 它幾乎沒人要用了，所以只在 full 才印。
        let subject = if detail.wants_id() {
            format!("（subject: {}）", f.subject)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "- {} [{env}] {}{subject}{}\n",
            f.rule.code(),
            f.detail,
            how_to_fix(project, f)
        ));
    }
    out
}

/// 每一項都附上「這種問題怎麼修」。
///
/// 少了這一句，Agent 看到 L002 只會知道「有東西壞了」，
/// 然後開始亂試——最常見的亂試是把契約刪掉，那完全是反效果。
///
/// **三檔詳細度都印。** 省略它省得下幾個字，但換來的是 Agent 亂試一輪，
/// 那一輪的 token 比這句話貴得多。
///
/// # L001 要看 subject 才知道怎麼修
///
/// 同一條規則會報在三種東西上，而三種的修法完全不同：契約要補連線、
/// 服務要建機器、外部系統要建實體。原本三種給同一句「用 create_nodes
/// 建機器，或用 add_connection 補連線」——兩句話裡至少有一句是錯的，
/// 而外部系統那種**兩句都是錯的**。
///
/// 分辨的邏輯**不在這裡重寫**：`edit::fix_for` 已經為了畫面上的按鈕
/// 做過同一件事。抄一份的話，兩邊遲早會對同一項發現給出不同的建議。
pub fn how_to_fix(project: &Project, finding: &Finding) -> &'static str {
    use loom_core::edit::Fix;

    if finding.rule == Rule::L001 {
        return match loom_core::edit::fix_for(project, finding) {
            Some(Fix::AddConnection { .. }) => {
                " → 這條契約在這個環境還沒有任何連線。用 add_connection 補"
            }
            Some(Fix::AddInstances { .. }) => {
                " → 這個服務在這個環境一台都還沒建。用 create_nodes 建機器"
            }
            Some(Fix::AddResource { .. }) => {
                " → 這個外部系統在這個環境還沒有實體。用 create 建一個 \
                 system_instance，記得填 address"
            }
            _ => " → 這個邏輯層元素在這個環境還沒有實現",
        };
    }

    match finding.rule {
        // 上面已經處理掉了，這裡是為了讓 match 保持窮舉。
        Rule::L001 => unreachable!(),
        Rule::L002 => " → 路徑斷了。用 add_connection 補上缺的那一段（經過 F5 要拆兩段）",
        Rule::L003 => " → 指到了不存在的東西。用 update 改掉那個參照",
        Rule::L004 => " → 用 update 改連線的 expect，或補上少的那幾台機器",
        Rule::L005 => " → 萬用字元要填 expect。用 update 改那條連線",
        Rule::L006 => " → 用 update 補上位址",
        Rule::L007 => " → 用 update 補上用途",
        Rule::L008 => " → 沒有連線碰到它。補一條連線，或它真的是冷備機就 update standalone=true",
        Rule::L012 => " → 參照壞了。用 update 改掉，或把懸空的那個元素 delete",
        // 兩條路都要講。只說「人不能當目標」的話，Agent 最常見的亂試是
        // 把整條契約刪掉——而那條關係是真的存在的，只是方向寫反了。
        Rule::L013 => " → 人只能當來源。用 update 把兩端對調，或把目標改成對方的服務",
        // 你是用**名字**在指涉東西的，所以撞名對你的影響比對人大得多：
        // 「連到 pay-01」會安靜地接到先找到的那一個。
        Rule::L014 => " → 兩個東西同名。用 update 改掉其中一個的 slug",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_new_scope_wins_but_the_old_top_level_field_still_works() {
        // 舊形狀照樣收：模型有時會憑印象送出以前的寫法。
        assert_eq!(
            Scope::of(&json!({"environment": "prod"}))
                .environment
                .as_deref(),
            Some("prod")
        );
        assert_eq!(
            Scope::of(&json!({"scope": {"environment": "uat"}, "environment": "prod"}))
                .environment
                .as_deref(),
            Some("uat")
        );
        assert_eq!(Scope::of(&json!({})).environment, None);
    }

    #[test]
    fn an_unknown_detail_lists_the_ones_that_work() {
        let e = Detail::of(&json!({"detail": "verbose"})).unwrap_err();
        assert!(e.contains("count"), "{e}");
        assert!(e.contains("brief"), "{e}");
        assert!(e.contains("full"), "{e}");
    }

    #[test]
    fn brief_is_the_default() {
        assert_eq!(Detail::of(&json!({})).unwrap(), Detail::Brief);
    }

    #[test]
    fn an_unknown_kind_lists_the_ones_that_work() {
        let e = Select::of(&json!({"select": {"kinds": ["containers"]}})).unwrap_err();
        assert!(e.contains("container"), "{e}");
        assert!(e.contains("instance"), "{e}");
    }

    #[test]
    fn asking_to_create_a_connection_points_at_the_right_tool() {
        // 回「認不得的 kind」會讓它以為自己打錯字，然後換個拼法再試一次。
        let e = kind_of("connection").unwrap_err();
        assert!(e.contains("add_connection"), "{e}");
    }

    #[test]
    fn every_kind_name_round_trips() {
        // 兩份名單漂移的話，症狀是「建得出來卻篩不到」。
        for key in KINDS {
            if *key == "connection" {
                continue;
            }
            let kind = kind_of(key).unwrap_or_else(|e| panic!("{key}：{e}"));
            assert_eq!(kind_key(kind), *key);
        }
    }

    #[test]
    fn an_unknown_rule_code_lists_the_real_ones() {
        // 假代號故意只有兩碼：`mise run check:rules` 會把程式碼裡出現的
        // 每一個三碼規則當成「這裡宣稱它存在」，編一個三碼的會讓它紅燈。
        let e = Select::of(&json!({"select": {"rules": ["L99"]}})).unwrap_err();
        assert!(e.contains("L001"), "{e}");
    }
}
