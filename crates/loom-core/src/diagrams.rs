//! 圖的存檔：一個環境一張自動產生的詳圖，加上使用者自己建的其他圖。
//!
//! ```text
//! diagrams/
//! ├── .catalog.yaml                        有哪些圖、各自照著哪一版模型畫的
//! ├── prod/
//! │   ├── diagram-loom-details.drawio      App 產的，一個環境就這一張
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

/// App 自動產生的那張詳圖的名字。**一個環境就這一張。**
///
/// 使用者自己建的圖不可以叫這個名字：那張是每次重畫都會被整份重產的，
/// 讓使用者的手繪圖跟它撞名，等於給了一個會被無聲蓋掉的名字。
pub const DETAILS: &str = "diagram-loom-details";

/// 目錄檔的位置。點開頭：這是給機器記帳用的，不是給人讀的圖。
pub const CATALOG_PATH: &str = "diagrams/.catalog.yaml";

/// 一張圖在目錄裡的紀錄。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    /// 上次存檔時的模型指紋。用來判斷「這張圖是不是舊模型畫的」。
    pub model: String,
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
            Some(entry) => entry.model = model.to_string(),
            None => entries.push(Entry {
                name: name.to_string(),
                model: model.to_string(),
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
                "{DETAILS} 是 App 自動產生的那張詳圖，一個環境只有一張，請換個名字"
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
            generated: e.name == DETAILS,
            stale: e.model != now,
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
/// 名字唯一的限制是**不能叫 [`DETAILS`]**，以及正規化之後不能是空的。
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
    if name == DETAILS {
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

    Ok(DiagramInfo {
        name,
        generated: false,
        stale: false,
    })
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
        }
    }

    fn saved() -> (MemoryStore, Project, Environment) {
        let (mut store, project, env) = (MemoryStore::new(), project(), environment());
        let model = fingerprint(&project, &env);
        save(&mut store, &project, &env, DETAILS, "<mxfile/>", &model).unwrap();
        (store, project, env)
    }

    #[test]
    fn a_diagram_lands_in_its_environments_folder() {
        let (store, ..) = saved();
        assert!(
            store
                .paths()
                .contains(&"diagrams/prod/diagram-loom-details.drawio")
        );
    }

    #[test]
    fn what_was_saved_reads_back() {
        let (store, _, env) = saved();
        assert_eq!(read(&store, &env, DETAILS).unwrap().xml, "<mxfile/>");
    }

    #[test]
    fn a_saved_diagram_remembers_which_model_it_was_drawn_from() {
        let (store, project, env) = saved();
        assert_eq!(
            read(&store, &env, DETAILS).unwrap().model,
            fingerprint(&project, &env)
        );
    }

    #[test]
    fn saving_is_refused_when_the_model_has_moved_on() {
        // 圖畫的是當時的模型。存下去等於把一份過期的資料蓋成最新的。
        let (mut store, project, env) = saved();
        let err = save(&mut store, &project, &env, DETAILS, "<新的/>", "別版").unwrap_err();
        assert_eq!(err, DiagramError::ModelChanged);
    }

    #[test]
    fn a_refused_save_leaves_the_file_untouched() {
        // 「不存」要真的不存。半套的存檔比擋下來更糟。
        let (mut store, project, env) = saved();
        let _ = save(&mut store, &project, &env, DETAILS, "<新的/>", "別版");
        assert_eq!(read(&store, &env, DETAILS).unwrap().xml, "<mxfile/>");
    }

    #[test]
    fn saving_again_right_after_saving_works() {
        // 存檔會回傳新的指紋。沒接住的話，第二次存檔會被自己擋下來。
        let (mut store, project, env) = saved();
        let model = read(&store, &env, DETAILS).unwrap().model;
        assert!(save(&mut store, &project, &env, DETAILS, "<新的/>", &model).is_ok());
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
        let err = create(&mut store, &project, &env, DETAILS, "<mxfile/>").unwrap_err();
        assert_eq!(err, DiagramError::Reserved);
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
            vec![(DETAILS.to_string(), true), ("簡報".to_string(), false)]
        );
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
