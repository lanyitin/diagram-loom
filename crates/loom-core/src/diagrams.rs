//! 圖的存檔：一個環境一張自動產生的詳圖，加上使用者自己建的其他圖。
//!
//! ```text
//! diagrams/
//! ├── .catalog.yaml                        有哪些圖、各自照著哪一版模型畫的
//! ├── prod/
//! │   ├── diagram-loom-deployment.drawio   App 產的部署圖，一個環境一張
//! │   ├── diagram-loom-context.drawio      App 產的 context 圖，一個環境一張
//! │   └── 我的簡報圖.drawio                 使用者自己建的
//! └── test/…
//! ```
//!
//! # 為什麼要有目錄檔
//!
//! 跟 `project.yaml` 列出環境是同一個理由：**不靠掃描資料夾**。檔案系統的
//! 排序因平台而異，清單順序會在不同機器上不一樣。
//!
//! 而且目錄檔還得記一件掃描問不出來的事——**每張圖是照著哪一版模型畫的**。
//!
//! # 存檔時模型變了就不存
//!
//! 圖畫的是「當時的模型」。模型後來改了，這張圖上的名字、位址、有哪些框
//! 就都是舊的——存下去等於把一份過期的資料蓋成最新的。
//!
//! 所以規矩是：**存檔時發現模型已經變了，就不存，並且告訴使用者。**
//! 要怎麼處理由人決定（重畫會用新模型重產一張，代價是手工排的版面不見）。
//!
//! 這是階段 7 對帳面板出現之前的**保守作法**：對帳能逐項裁決差異，
//! 在那之前，寧可不存也不要默默存下一張說謊的圖。
//!
//! # 為什麼指紋含整個邏輯層
//!
//! 圖上畫的不只有環境層：服務實體的副標是邏輯層的 Container 名字、線上的
//! 文字是連線的用途。所以「模型」指的是**這個環境 + 整份邏輯層**。
//!
//! 這確實偏嚴——改了另一條契約的用途，這個環境的圖也會被擋下來。但反過來
//! （漏掉一種變更）的代價是存下一張沒有人知道它過期的圖，那個錯誤沒有人會發現。

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::environment::Environment;
use crate::slug;
use crate::store::FileStore;

/// App 自動產生的**部署圖**。畫站點、機器、服務實體與位址。
///
/// 一個形狀剛好一個環境層元素，所以它是唯一的對帳對象（[`DiagramKind::Detail`]）。
pub const DEPLOYMENT: &str = "diagram-loom-deployment";

/// App 自動產生的 **context 圖**。只畫人與系統，一個系統一個框。
///
/// 兩個系統之間的線是**收攏過的**——A 對 B 有三條契約也只畫一條。所以它
/// 是 [`DiagramKind::Simple`]，不做完整對帳（理由見 [`kind_of`]）。
pub const CONTEXT: &str = "diagram-loom-context";

/// [`DEPLOYMENT`] 改名前叫這個。只有 [`migrate`] 還會提到它。
pub const LEGACY_DEPLOYMENT: &str = "diagram-loom-details";

/// App 自動產生的圖有哪幾張。**使用者自己建的圖不可以用這些名字**：
/// 它們每次重畫都會被整份重產，撞名等於給了一個會被無聲蓋掉的名字。
pub const GENERATED: [&str; 2] = [DEPLOYMENT, CONTEXT];

/// 這個名字是不是 App 自己產的那幾張之一。
pub fn is_generated(name: &str) -> bool {
    GENERATED.contains(&name)
}

/// 目錄檔的位置。點開頭：這是給機器記帳用的，不是給人讀的圖。
pub const CATALOG_PATH: &str = "diagrams/.catalog.yaml";

/// 這張圖是拿來對帳的，還是拿來給人看的。
///
/// # 為什麼要分
///
/// 人畫圖給人看的時候會**刻意簡化**：Container Diagram 上「Channel 連 Redis」
/// 是一個框連一個框，不會把 12 個服務實體兩兩畫出來。
///
/// 但簡化跟「怕漏」天生是敵人——**把 12 條線收成 1 條，就是把「其中一條
/// 不見了」藏起來**。解法是分成兩種圖，而不是讓同一張圖兩者兼顧
/// （見 `docs/drawio-integration.md`）。
///
/// | | [`Detail`](DiagramKind::Detail) | [`Simple`](DiagramKind::Simple) |
/// | --- | --- | --- |
/// | 一個形狀代表 | **剛好一個**模型元素 | 一個或一組 |
/// | 對帳 | 唯一的對帳對象 | **不對帳**，只做弱檢查 |
/// | 標註選單 | 只有環境層 | 也給邏輯層（人、系統、服務、契約） |
///
/// 分開之後，`reconcile` 的「一個 `loomId` ↔ 一個元素」就**不必放寬**。
/// 那條規矩守住了，簡化才永遠沒有機會變成謊言。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DiagramKind {
    /// 詳圖：一個形狀剛好一個模型元素。App 產的那張永遠是這種。
    Detail,
    /// 簡圖：給讀者、簡報、文件看的。**預設**——使用者自己開一張圖，
    /// 十次有九次是要簡化的；當成詳圖的話對帳會噴出一堆他沒有畫錯的差異。
    #[default]
    Simple,
}

/// 一張圖在目錄裡的紀錄。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    /// 上次存檔時的模型指紋。用來判斷「這張圖是不是舊模型畫的」。
    pub model: String,
    /// 詳圖還是簡圖。
    ///
    /// `default` 是為了讀得動這個欄位出現之前存的目錄檔。那時候還沒有這個
    /// 概念，而使用者自己建的圖本來就多半是簡圖——當成詳圖的話，
    /// 他打開對帳會看到一整頁「模型有、圖上沒有」，而那些都不是他的錯。
    #[serde(default)]
    pub kind: DiagramKind,
}

/// 有哪些圖。以環境 slug 為鍵，跟資料夾的分法一致。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(default)]
    pub environments: BTreeMap<String, Vec<Entry>>,
}

impl Catalog {
    /// 讀目錄檔。
    ///
    /// **只有「檔案不存在」才當成空的**——還沒存過任何圖是正常的。其他失敗
    /// （權限不足、檔案毀損）一律回報：當成空的話，使用者會看到「一張圖都沒有」，
    /// 然後存一張新的把舊的蓋掉。
    pub fn load(store: &impl FileStore) -> Result<Self> {
        match store.read(CATALOG_PATH) {
            Ok(text) => yaml_serde::from_str(&text).map_err(|e| DiagramError::Yaml(e.to_string())),
            Err(e) if e.is_not_found() => Ok(Self::default()),
            Err(e) => Err(DiagramError::Store(e.to_string())),
        }
    }

    pub fn save(&self, store: &mut impl FileStore) -> Result<()> {
        let text = yaml_serde::to_string(self).map_err(|e| DiagramError::Yaml(e.to_string()))?;
        store
            .write(CATALOG_PATH, &text)
            .map_err(|e| DiagramError::Store(e.to_string()))
    }

    pub fn of(&self, env_slug: &str) -> &[Entry] {
        self.environments
            .get(env_slug)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn remember(&mut self, env_slug: &str, name: &str, model: &str) {
        let entries = self.environments.entry(env_slug.to_string()).or_default();
        match entries.iter_mut().find(|e| e.name == name) {
            // **只更新指紋。** 種類是使用者設的，存一次檔就把它洗掉的話，
            // 他會發現自己標成簡圖的那張又被當成詳圖在對帳——而且不會有訊息。
            Some(entry) => entry.model = model.to_string(),
            None => entries.push(Entry {
                name: name.to_string(),
                model: model.to_string(),
                kind: kind_of(name),
            }),
        }
    }
}

/// 給畫面看的一張圖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiagramInfo {
    pub name: String,
    /// 是不是 App 產的那一張。畫面要標出來：它會被重畫整份蓋掉。
    pub generated: bool,
    /// 存檔之後模型又變了。這張圖畫的是舊的。
    pub stale: bool,
    /// 詳圖還是簡圖。**畫面一定要說出來**——簡圖不對帳，而
    /// 「沒說 = 沒問題」正是這個專案最怕的那種誤會：有人把簡圖貼進文件，
    /// 同事看到它從這個工具長出來，理所當然以為它被檢查過了。
    pub kind: DiagramKind,
}

/// 讀回來的一張圖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Loaded {
    pub xml: String,
    /// 這張圖畫的是哪一版模型。存檔時要拿它跟當下的比。
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramError {
    Store(String),
    Yaml(String),
    /// 名字正規化之後是空的。
    EmptyName,
    /// 想用 App 保留的那個名字。
    Reserved,
    /// 這個環境裡已經有同名的圖。
    Duplicate(String),
    NotFound(String),
    /// 存檔時模型已經跟畫這張圖的時候不一樣了。
    ModelChanged,
}

impl fmt::Display for DiagramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagramError::Store(e) => write!(f, "讀寫失敗：{e}"),
            DiagramError::Yaml(e) => write!(f, "解析 {CATALOG_PATH} 失敗：{e}"),
            DiagramError::EmptyName => write!(f, "圖的名字不能空白"),
            DiagramError::Reserved => write!(
                f,
                "{DEPLOYMENT}、{CONTEXT}、{LEGACY_DEPLOYMENT} 是 App 自動產生的圖，\
                 每個環境各一張，請換個名字"
            ),
            DiagramError::Duplicate(name) => write!(f, "這個環境已經有一張圖叫 {name} 了"),
            DiagramError::NotFound(name) => write!(f, "找不到圖 {name}"),
            DiagramError::ModelChanged => write!(
                f,
                "模型已經變了，這張圖畫的是舊的模型，所以沒有存檔。\
                 重畫會用現在的模型重產一張（手工排的版面會不見）"
            ),
        }
    }
}

impl std::error::Error for DiagramError {}

type Result<T> = std::result::Result<T, DiagramError>;

/// 一張圖存在哪。
///
/// 名字一律先過 [`slug::slugify`]，所以**跳不出這個資料夾**：正規 slug 只含
/// 文數字、連字號與中日韓文字，`../` 之類的東西在這一步就沒了。
/// 這跟環境檔名走的是同一條規矩（見 [`crate::repository`]）。
pub fn path_of(env_slug: &str, name: &str) -> String {
    format!("diagrams/{env_slug}/{name}.drawio")
}

/// 這個環境（加上整份邏輯層）現在長什麼樣。
///
/// 只用來比對「一不一樣」，所以是什麼演算法不重要，重要的是**同樣的模型要
/// 得到同樣的值**——包括關掉 App 再打開。因此自己寫一個 FNV-1a，
/// 不用 `DefaultHasher`：那個明講了跨版本可能改變，換一版 Rust 就會讓所有
/// 存過的指紋全部對不上，症狀是「每一張圖都突然說模型變了」。
pub fn fingerprint(project: &Project, env: &Environment) -> String {
    // 序列化失敗在這裡等於「算不出指紋」。回一個固定的字串會讓它跟自己相等，
    // 也就是**默默放行**——所以改回一個一定對不上的值，寧可擋下來。
    let logical = yaml_serde::to_string(&project.logical).unwrap_or_else(|e| format!("錯誤 {e}"));
    let environment = yaml_serde::to_string(env).unwrap_or_else(|e| format!("錯誤 {e}"));

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in logical
        .bytes()
        .chain(b"\n".iter().copied())
        .chain(environment.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 把改名前存下來的圖接過來。**開專案的時候跑一次。**
///
/// `diagram-loom-details` 改叫 [`DEPLOYMENT`] 之後，舊專案的資料夾裡還躺著
/// 一個舊名字的檔案。什麼都不做的話，使用者會看到清單上多一張叫舊名字的
/// 「使用者的圖」，而 App 在旁邊重產一張空的新圖——他手工排的版面看起來
/// 就像不見了。
///
/// 所以連檔案帶目錄檔一起改名。回傳有沒有真的動到東西。
///
/// # 已經有新名字的話就不動
///
/// 那表示使用者手上已經有一張他認得的部署圖。用舊的蓋掉它，等於把他排好的
/// 版面換成另一張圖，而且沒有任何提示。寧可留著舊的那個檔案。
pub fn migrate(store: &mut impl FileStore) -> Result<bool> {
    let mut catalog = Catalog::load(store)?;
    let mut changed = false;

    for (env_slug, entries) in catalog.environments.iter_mut() {
        if entries.iter().any(|e| e.name == DEPLOYMENT) {
            continue;
        }
        let Some(entry) = entries.iter_mut().find(|e| e.name == LEGACY_DEPLOYMENT) else {
            continue;
        };

        let from = path_of(env_slug, LEGACY_DEPLOYMENT);
        let to = path_of(env_slug, DEPLOYMENT);
        match store.read(&from) {
            Ok(xml) => {
                store
                    .write(&to, &xml)
                    .map_err(|e| DiagramError::Store(e.to_string()))?;
                store
                    .remove(&from)
                    .map_err(|e| DiagramError::Store(e.to_string()))?;
            }
            // 目錄檔有、檔案不見了。**還是要改目錄檔**：留著舊名字的話，
            // 下次開專案又會試一次，而使用者看到的是一張永遠讀不出來的圖。
            Err(e) if e.is_not_found() => {}
            Err(e) => return Err(DiagramError::Store(e.to_string())),
        }

        entry.name = DEPLOYMENT.to_string();
        changed = true;
    }

    if changed {
        catalog.save(store)?;
    }
    Ok(changed)
}

/// 這個環境有哪些圖。
pub fn list(
    store: &impl FileStore,
    project: &Project,
    env: &Environment,
) -> Result<Vec<DiagramInfo>> {
    let now = fingerprint(project, env);
    Ok(Catalog::load(store)?
        .of(&env.slug)
        .iter()
        .map(|e| DiagramInfo {
            name: e.name.clone(),
            generated: is_generated(&e.name),
            stale: e.model != now,
            // 名字決定的優先於存下來的，理由見 `kind_of`。
            kind: if is_generated(&e.name) {
                kind_of(&e.name)
            } else {
                e.kind
            },
        })
        .collect())
}

/// 讀一張圖，連同它是照哪一版模型畫的。
pub fn read(store: &impl FileStore, env: &Environment, name: &str) -> Result<Loaded> {
    let entry = Catalog::load(store)?
        .of(&env.slug)
        .iter()
        .find(|e| e.name == name)
        .cloned()
        .ok_or_else(|| DiagramError::NotFound(name.to_string()))?;

    let xml = store
        .read(&path_of(&env.slug, name))
        .map_err(|e| match e.is_not_found() {
            // 目錄裡有、檔案卻不見了。當成「沒有這張圖」而不是空白圖：
            // 空白圖存回去就把「檔案不見了」這件事變成「這張圖本來就是空的」。
            true => DiagramError::NotFound(name.to_string()),
            false => DiagramError::Store(e.to_string()),
        })?;

    Ok(Loaded {
        xml,
        model: entry.model,
    })
}

/// 使用者自己建一張新圖。
///
/// 名字唯一的限制是**不能用 App 保留的那幾個**（見 [`GENERATED`]），
/// 以及正規化之後不能是空的。
/// 其他隨他取——這是他自己的圖。
pub fn create(
    store: &mut impl FileStore,
    project: &Project,
    env: &Environment,
    name: &str,
    xml: &str,
) -> Result<DiagramInfo> {
    let name = slug::slugify(name);
    if name.is_empty() {
        return Err(DiagramError::EmptyName);
    }
    // 舊名字也一起擋。[`migrate`] 會把它改成新名字，這時候讓使用者建一張
    // 同名的圖，等於埋一個「下次開這個專案就被改名」的地雷。
    if is_generated(&name) || name == LEGACY_DEPLOYMENT {
        return Err(DiagramError::Reserved);
    }

    let mut catalog = Catalog::load(store)?;
    if catalog.of(&env.slug).iter().any(|e| e.name == name) {
        return Err(DiagramError::Duplicate(name));
    }

    let model = fingerprint(project, env);
    write_file(store, env, &name, xml)?;
    catalog.remember(&env.slug, &name, &model);
    catalog.save(store)?;

    let kind = kind_of(&name);
    Ok(DiagramInfo {
        name,
        generated: false,
        stale: false,
        kind,
    })
}

/// 這個名字**一定**是哪一種。
///
/// App 產的圖是程式照模型畫的，是詳圖還是簡圖由它的**產生方式**決定，
/// 不是偏好。所以它不吃目錄檔裡存的值：存壞了、手改壞了，都不該讓唯一的
/// 對帳對象變成不對帳。
///
/// # 為什麼 context 圖是簡圖
///
/// 因為它的線是**收攏過的**：A 系統對 B 系統有三條契約，圖上只有一條線。
/// 一條線代表一群契約，就撐不住「一個 `loomId` ↔ 一個元素」那條規矩——
/// 而那條規矩正是對帳不會說謊的原因。
///
/// 與其為了 context 圖放寬它，不如承認 context 圖本來就是給人看的：
/// **它不對帳，只做弱檢查**（見 [`crate::reconcile::dangling`]）。
/// 框還是帶 `loomId`（一個系統就是一個系統），所以「圖上畫著一個已經被
/// 刪掉的系統」還是叫得出來。
fn kind_of(name: &str) -> DiagramKind {
    match name {
        DEPLOYMENT => DiagramKind::Detail,
        CONTEXT => DiagramKind::Simple,
        _ => DiagramKind::Simple,
    }
}

/// 改一張圖是詳圖還是簡圖。
///
/// App 產的那張改不了，理由見 [`kind_of`]。
pub fn set_kind(
    store: &mut impl FileStore,
    env: &Environment,
    name: &str,
    kind: DiagramKind,
) -> Result<()> {
    if is_generated(name) {
        return Err(DiagramError::Reserved);
    }
    let mut catalog = Catalog::load(store)?;
    let entry = catalog
        .environments
        .get_mut(&env.slug)
        .and_then(|list| list.iter_mut().find(|e| e.name == name))
        .ok_or_else(|| DiagramError::NotFound(name.to_string()))?;
    entry.kind = kind;
    catalog.save(store)
}

/// 存檔。
///
/// `drawn_from` 是這張圖畫的時候模型的指紋（從 [`read`] 或 [`fingerprint`] 拿到的）。
/// 跟現在對不上就**不存**，回 [`DiagramError::ModelChanged`]。
///
/// 回傳存進去的指紋，讓畫面接著拿它當新的 `drawn_from`——不然存完一次之後
/// 手上那個就過期了，下一次存檔會被自己擋下來。
pub fn save(
    store: &mut impl FileStore,
    project: &Project,
    env: &Environment,
    name: &str,
    xml: &str,
    drawn_from: &str,
) -> Result<String> {
    let now = fingerprint(project, env);
    if drawn_from != now {
        return Err(DiagramError::ModelChanged);
    }

    let mut catalog = Catalog::load(store)?;
    write_file(store, env, name, xml)?;
    catalog.remember(&env.slug, name, &now);
    catalog.save(store)?;

    Ok(now)
}

fn write_file(store: &mut impl FileStore, env: &Environment, name: &str, xml: &str) -> Result<()> {
    store
        .write(&path_of(&env.slug, name), xml)
        .map_err(|e| DiagramError::Store(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Id;
    use crate::logical::Logical;
    use crate::store::MemoryStore;

    fn environment() -> Environment {
        Environment {
            id: Id::from("e"),
            slug: "prod".into(),
            name: "prod".into(),
            nodes: vec![],
            infra: vec![],
            systems: vec![],
            connections: vec![],
            memo: String::new(),
        }
    }

    fn project() -> Project {
        Project {
            id: Id::from("p"),
            slug: "shop".into(),
            name: "shop".into(),
            logical: Logical {
                people: vec![],
                systems: vec![],
                containers: vec![],
                relationships: vec![],
            },
            environments: vec![environment()],
            memo: String::new(),
        }
    }

    fn saved() -> (MemoryStore, Project, Environment) {
        let (mut store, project, env) = (MemoryStore::new(), project(), environment());
        let model = fingerprint(&project, &env);
        save(&mut store, &project, &env, DEPLOYMENT, "<mxfile/>", &model).unwrap();
        (store, project, env)
    }

    #[test]
    fn a_diagram_lands_in_its_environments_folder() {
        let (store, ..) = saved();
        assert!(
            store
                .paths()
                .contains(&"diagrams/prod/diagram-loom-deployment.drawio")
        );
    }

    #[test]
    fn what_was_saved_reads_back() {
        let (store, _, env) = saved();
        assert_eq!(read(&store, &env, DEPLOYMENT).unwrap().xml, "<mxfile/>");
    }

    #[test]
    fn a_saved_diagram_remembers_which_model_it_was_drawn_from() {
        let (store, project, env) = saved();
        assert_eq!(
            read(&store, &env, DEPLOYMENT).unwrap().model,
            fingerprint(&project, &env)
        );
    }

    #[test]
    fn saving_is_refused_when_the_model_has_moved_on() {
        // 圖畫的是當時的模型。存下去等於把一份過期的資料蓋成最新的。
        let (mut store, project, env) = saved();
        let err = save(&mut store, &project, &env, DEPLOYMENT, "<新的/>", "別版").unwrap_err();
        assert_eq!(err, DiagramError::ModelChanged);
    }

    #[test]
    fn a_refused_save_leaves_the_file_untouched() {
        // 「不存」要真的不存。半套的存檔比擋下來更糟。
        let (mut store, project, env) = saved();
        let _ = save(&mut store, &project, &env, DEPLOYMENT, "<新的/>", "別版");
        assert_eq!(read(&store, &env, DEPLOYMENT).unwrap().xml, "<mxfile/>");
    }

    #[test]
    fn saving_again_right_after_saving_works() {
        // 存檔會回傳新的指紋。沒接住的話，第二次存檔會被自己擋下來。
        let (mut store, project, env) = saved();
        let model = read(&store, &env, DEPLOYMENT).unwrap().model;
        assert!(save(&mut store, &project, &env, DEPLOYMENT, "<新的/>", &model).is_ok());
    }

    #[test]
    fn changing_the_environment_changes_the_fingerprint() {
        let (project, mut env) = (project(), environment());
        let before = fingerprint(&project, &env);
        env.name = "改了名字".into();
        assert_ne!(fingerprint(&project, &env), before);
    }

    #[test]
    fn changing_the_logical_layer_changes_the_fingerprint() {
        // 圖上的副標與線上的文字都來自邏輯層，所以它也算模型的一部分。
        let (mut project, env) = (project(), environment());
        let before = fingerprint(&project, &env);
        project.logical.people.push(crate::logical::Person {
            id: Id::from("per"),
            slug: "客戶".into(),
            name: "客戶".into(),
            memo: String::new(),
        });
        assert_ne!(fingerprint(&project, &env), before);
    }

    #[test]
    fn the_same_model_always_gives_the_same_fingerprint() {
        // 關掉 App 再打開也要對得上，否則每一張圖都會突然說模型變了。
        assert_eq!(
            fingerprint(&project(), &environment()),
            fingerprint(&project(), &environment())
        );
    }

    #[test]
    fn a_user_cannot_take_the_generated_diagrams_name() {
        // 那張是每次重畫都會被整份重產的。撞名等於給了一個會被無聲蓋掉的名字。
        let (mut store, project, env) = saved();
        for name in [DEPLOYMENT, CONTEXT, LEGACY_DEPLOYMENT] {
            let err = create(&mut store, &project, &env, name, "<mxfile/>").unwrap_err();
            assert_eq!(err, DiagramError::Reserved, "{name} 應該是保留字");
        }
    }

    #[test]
    fn the_deployment_diagram_is_a_detail_one_and_the_context_diagram_is_not() {
        // context 圖把 A 對 B 的三條契約收成一條線。一條線代表一群契約，
        // 就撐不住「一個 loomId 一個元素」——所以它不對帳。
        assert_eq!(kind_of(DEPLOYMENT), DiagramKind::Detail);
        assert_eq!(kind_of(CONTEXT), DiagramKind::Simple);
    }

    #[test]
    fn the_kind_of_a_generated_diagram_cannot_be_changed() {
        // 它是程式照模型畫的，是哪一種由產生方式決定，不是偏好。
        let (mut store, _, env) = saved();
        for name in [DEPLOYMENT, CONTEXT] {
            assert_eq!(
                set_kind(&mut store, &env, name, DiagramKind::Detail).unwrap_err(),
                DiagramError::Reserved
            );
        }
    }

    #[test]
    fn the_old_name_is_carried_over_file_and_all() {
        // 舊專案的資料夾裡躺著 diagram-loom-details.drawio。什麼都不做的話，
        // 使用者手排的版面看起來就像不見了。
        let (mut store, project, env) = (MemoryStore::new(), project(), environment());
        let model = fingerprint(&project, &env);
        save(
            &mut store,
            &project,
            &env,
            LEGACY_DEPLOYMENT,
            "<我排好的/>",
            &model,
        )
        .unwrap();

        assert!(migrate(&mut store).unwrap(), "有東西要改名");

        assert_eq!(read(&store, &env, DEPLOYMENT).unwrap().xml, "<我排好的/>");
        assert!(
            !store
                .paths()
                .contains(&"diagrams/prod/diagram-loom-details.drawio"),
            "舊檔案要跟著消失，不然資料夾裡會留下一個沒有人管的 .drawio"
        );
    }

    #[test]
    fn migrating_twice_does_nothing_the_second_time() {
        let (mut store, project, env) = (MemoryStore::new(), project(), environment());
        let model = fingerprint(&project, &env);
        save(
            &mut store,
            &project,
            &env,
            LEGACY_DEPLOYMENT,
            "<我排好的/>",
            &model,
        )
        .unwrap();

        assert!(migrate(&mut store).unwrap());
        assert!(!migrate(&mut store).unwrap(), "第二次應該什麼都不做");
        assert_eq!(read(&store, &env, DEPLOYMENT).unwrap().xml, "<我排好的/>");
    }

    #[test]
    fn migration_never_overwrites_a_diagram_that_already_has_the_new_name() {
        // 使用者手上已經有一張他認得的部署圖。用舊的蓋掉它，等於把他排好的
        // 版面換成另一張圖，而且沒有任何提示。
        let (mut store, project, env) = (MemoryStore::new(), project(), environment());
        let model = fingerprint(&project, &env);
        save(&mut store, &project, &env, DEPLOYMENT, "<新的/>", &model).unwrap();
        save(
            &mut store,
            &project,
            &env,
            LEGACY_DEPLOYMENT,
            "<舊的/>",
            &model,
        )
        .unwrap();

        assert!(!migrate(&mut store).unwrap());
        assert_eq!(read(&store, &env, DEPLOYMENT).unwrap().xml, "<新的/>");
    }

    #[test]
    fn a_user_diagram_can_be_called_anything_else() {
        let (mut store, project, env) = saved();
        let made = create(&mut store, &project, &env, "我的簡報圖", "<mxfile/>").unwrap();
        assert_eq!(made.name, "我的簡報圖");
        assert!(!made.generated);
    }

    #[test]
    fn a_name_that_would_escape_the_folder_is_flattened() {
        // 名字會變成檔名，所以它是最典型會被 `../` 打穿的地方。
        let (mut store, project, env) = saved();
        let made = create(&mut store, &project, &env, "../../etc/passwd", "<mxfile/>").unwrap();
        assert_eq!(
            path_of(&env.slug, &made.name),
            "diagrams/prod/etc-passwd.drawio"
        );
    }

    #[test]
    fn two_diagrams_cannot_share_a_name() {
        let (mut store, project, env) = saved();
        create(&mut store, &project, &env, "簡報", "<mxfile/>").unwrap();
        let err = create(&mut store, &project, &env, "簡報", "<mxfile/>").unwrap_err();
        assert_eq!(err, DiagramError::Duplicate("簡報".into()));
    }

    #[test]
    fn a_blank_name_is_refused() {
        let (mut store, project, env) = saved();
        assert_eq!(
            create(&mut store, &project, &env, "  ///  ", "<mxfile/>").unwrap_err(),
            DiagramError::EmptyName
        );
    }

    #[test]
    fn the_list_says_which_one_the_app_generates() {
        let (mut store, project, env) = saved();
        create(&mut store, &project, &env, "簡報", "<mxfile/>").unwrap();
        let names: Vec<(String, bool)> = list(&store, &project, &env)
            .unwrap()
            .into_iter()
            .map(|d| (d.name, d.generated))
            .collect();
        assert_eq!(
            names,
            vec![(DEPLOYMENT.to_string(), true), ("簡報".to_string(), false)]
        );
        assert!(is_generated(CONTEXT), "context 圖也是 App 產的");
    }

    #[test]
    fn the_list_says_which_ones_are_drawn_from_an_old_model() {
        let (store, project, mut env) = saved();
        assert!(!list(&store, &project, &env).unwrap()[0].stale);

        env.name = "改了名字".into();
        assert!(
            list(&store, &project, &env).unwrap()[0].stale,
            "模型變了就要說這張圖是舊的"
        );
    }

    #[test]
    fn each_environment_keeps_its_own_diagrams() {
        // 一張圖只屬於一個環境。混在一起的話，prod 的圖會出現在 test 的清單裡。
        let (mut store, project, env) = saved();
        let other = Environment {
            slug: "test".into(),
            ..environment()
        };
        create(&mut store, &project, &other, "簡報", "<mxfile/>").unwrap();

        assert_eq!(list(&store, &project, &env).unwrap().len(), 1);
        assert!(store.paths().contains(&"diagrams/test/簡報.drawio"));
    }

    #[test]
    fn a_diagram_that_was_never_saved_is_not_found() {
        let (store, _, env) = saved();
        assert_eq!(
            read(&store, &env, "沒存過").unwrap_err(),
            DiagramError::NotFound("沒存過".into())
        );
    }

    #[test]
    fn a_broken_catalog_is_never_treated_as_empty() {
        // 當成空的話，使用者會看到「一張圖都沒有」，然後存一張新的把舊的蓋掉。
        let mut store = MemoryStore::new();
        store
            .write(CATALOG_PATH, "environments: 這不是清單")
            .unwrap();
        assert!(matches!(
            list(&store, &project(), &environment()),
            Err(DiagramError::Yaml(_))
        ));
    }
}
