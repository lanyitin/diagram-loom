//! 專案資料夾的讀寫。
//!
//! 一個 Project = 一個資料夾，內含多個 YAML 檔：
//!
//! ```text
//! my-project.loom/
//! ├── project.yaml              專案基本資料
//! ├── logical/
//! │   ├── systems.yaml          Person、SoftwareSystem
//! │   ├── containers.yaml       Container + EndpointDef
//! │   └── relationships.yaml    邏輯連線（＝契約）
//! └── environments/
//!     ├── prod.yaml
//!     ├── test.yaml
//!     └── dev.yaml
//! ```
//!
//! # 為什麼拆成多個檔
//!
//! 數百條連線寫在單一檔案會有數千行，兩個人同改必然衝突。
//! 依環境切開之後，改 prod 不會動到 dev，`git diff` 一看就懂。
//!
//! # 為什麼環境檔名用 slug 而不是 UUID
//!
//! 檔名是給人看的。UUID 檔名的 `git diff` 完全讀不出改了哪個環境。
//! 環境的 UUID 仍然存在檔案內容裡，改名只會改檔名，參照不會斷。
//!
//! # 這裡不碰磁碟
//!
//! 佈局與 YAML 是領域知識，留在核心；「字串放哪裡」交給
//! [`FileStore`](crate::store::FileStore)。
//! 想直接對資料夾操作可以用 [`save_to_dir`] / [`load_from_dir`]。

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::id::Id;
use crate::logical::{Container, Logical, Person, Relationship, SoftwareSystem};
use crate::slug;
use crate::store::{FileStore, FsStore, StoreError};

const PROJECT_FILE: &str = "project.yaml";
const SYSTEMS_FILE: &str = "logical/systems.yaml";
const CONTAINERS_FILE: &str = "logical/containers.yaml";
const RELATIONSHIPS_FILE: &str = "logical/relationships.yaml";

/// `project.yaml` 的內容。環境不寫在這裡，各自一個檔。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProjectFile {
    id: Id,
    slug: String,
    name: String,
    /// 環境的 slug 清單，決定載入順序。
    ///
    /// 不靠掃描資料夾決定順序：檔案系統的排序因平台而異，
    /// 會讓 lint 結果的順序在不同機器上不一樣。
    environments: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SystemsFile {
    #[serde(default)]
    people: Vec<Person>,
    #[serde(default)]
    systems: Vec<SoftwareSystem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ContainersFile {
    #[serde(default)]
    containers: Vec<Container>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct RelationshipsFile {
    #[serde(default)]
    relationships: Vec<Relationship>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    Store(StoreError),
    Yaml {
        path: String,
        message: String,
    },
    /// 環境的 slug 不是正規寫法，無法安全地當檔名。
    BadEnvironmentSlug {
        slug: String,
        suggestion: String,
    },
    /// `project.yaml` 列出的環境有重複。
    DuplicateEnvironment(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepositoryError::Store(e) => write!(f, "讀寫失敗：{e}"),
            RepositoryError::Yaml { path, message } => {
                write!(f, "解析 {path} 失敗：{message}")
            }
            RepositoryError::BadEnvironmentSlug { slug, suggestion } => {
                write!(f, "環境名稱 {slug} 不能安全地當檔名，建議改成 {suggestion}")
            }
            RepositoryError::DuplicateEnvironment(slug) => {
                write!(f, "環境 {slug} 重複出現")
            }
        }
    }
}

impl std::error::Error for RepositoryError {}

impl From<StoreError> for RepositoryError {
    fn from(value: StoreError) -> Self {
        RepositoryError::Store(value)
    }
}

type Result<T> = std::result::Result<T, RepositoryError>;

/// 把專案寫進任意的儲存體。
pub fn save(project: &Project, store: &mut impl FileStore) -> Result<()> {
    check_environment_slugs(project)?;

    write_yaml(
        store,
        PROJECT_FILE,
        &ProjectFile {
            id: project.id.clone(),
            slug: project.slug.clone(),
            name: project.name.clone(),
            environments: project
                .environments
                .iter()
                .map(|e| e.slug.clone())
                .collect(),
        },
    )?;

    write_yaml(
        store,
        SYSTEMS_FILE,
        &SystemsFile {
            people: project.logical.people.clone(),
            systems: project.logical.systems.clone(),
        },
    )?;
    write_yaml(
        store,
        CONTAINERS_FILE,
        &ContainersFile {
            containers: project.logical.containers.clone(),
        },
    )?;
    write_yaml(
        store,
        RELATIONSHIPS_FILE,
        &RelationshipsFile {
            relationships: project.logical.relationships.clone(),
        },
    )?;

    for env in &project.environments {
        write_yaml(store, &environment_path(&env.slug), env)?;
    }

    Ok(())
}

/// 從任意的儲存體讀出專案。
pub fn load(store: &impl FileStore) -> Result<Project> {
    let meta: ProjectFile = read_yaml(store, PROJECT_FILE)?;
    let systems: SystemsFile = read_yaml(store, SYSTEMS_FILE)?;
    let containers: ContainersFile = read_yaml(store, CONTAINERS_FILE)?;
    let relationships: RelationshipsFile = read_yaml(store, RELATIONSHIPS_FILE)?;

    let mut seen = HashSet::new();
    let mut environments = Vec::with_capacity(meta.environments.len());
    for env_slug in &meta.environments {
        if !seen.insert(env_slug.clone()) {
            return Err(RepositoryError::DuplicateEnvironment(env_slug.clone()));
        }
        environments.push(read_yaml(store, &environment_path(env_slug))?);
    }

    Ok(Project {
        id: meta.id,
        slug: meta.slug,
        name: meta.name,
        logical: Logical {
            people: systems.people,
            systems: systems.systems,
            containers: containers.containers,
            relationships: relationships.relationships,
        },
        environments,
    })
}

/// 直接寫進磁碟上的資料夾。資料夾不存在會自動建立。
pub fn save_to_dir(project: &Project, dir: &Path) -> Result<()> {
    save(project, &mut FsStore::new(dir))
}

/// 直接從磁碟上的資料夾讀取。
pub fn load_from_dir(dir: &Path) -> Result<Project> {
    load(&FsStore::new(dir))
}

fn environment_path(env_slug: &str) -> String {
    format!("environments/{env_slug}.yaml")
}

/// 環境 slug 會直接變成檔名，所以必須是正規寫法。
///
/// 這也順便擋掉 `../` 之類會跳出資料夾的名稱——正規 slug 只含
/// 文數字與連字號。
fn check_environment_slugs(project: &Project) -> Result<()> {
    for env in &project.environments {
        if !slug::is_normalized(&env.slug) {
            return Err(RepositoryError::BadEnvironmentSlug {
                slug: env.slug.clone(),
                suggestion: slug::slugify(&env.slug),
            });
        }
    }
    Ok(())
}

fn write_yaml<T: Serialize>(store: &mut impl FileStore, path: &str, value: &T) -> Result<()> {
    let text = yaml_serde::to_string(value).map_err(|e| RepositoryError::Yaml {
        path: path.to_string(),
        message: e.to_string(),
    })?;
    store.write(path, &text)?;
    Ok(())
}

fn read_yaml<T: for<'de> Deserialize<'de>>(store: &impl FileStore, path: &str) -> Result<T> {
    let text = store.read(path)?;
    yaml_serde::from_str(&text).map_err(|e| RepositoryError::Yaml {
        path: path.to_string(),
        message: e.to_string(),
    })
}
