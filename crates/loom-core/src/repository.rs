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

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Project;
use crate::id::Id;
use crate::logical::{Container, Logical, Person, Relationship, SoftwareSystem};
use crate::slug;

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

#[derive(Debug)]
pub enum RepositoryError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Yaml {
        path: PathBuf,
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
            RepositoryError::Io { path, source } => {
                write!(f, "讀寫 {} 失敗：{source}", path.display())
            }
            RepositoryError::Yaml { path, message } => {
                write!(f, "解析 {} 失敗：{message}", path.display())
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

type Result<T> = std::result::Result<T, RepositoryError>;

/// 把專案寫進資料夾。資料夾不存在會自動建立。
pub fn save(project: &Project, dir: &Path) -> Result<()> {
    check_environment_slugs(project)?;

    create_dir(dir)?;
    create_dir(&dir.join("logical"))?;
    create_dir(&dir.join("environments"))?;

    write_yaml(
        &dir.join("project.yaml"),
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
        &dir.join("logical/systems.yaml"),
        &SystemsFile {
            people: project.logical.people.clone(),
            systems: project.logical.systems.clone(),
        },
    )?;
    write_yaml(
        &dir.join("logical/containers.yaml"),
        &ContainersFile {
            containers: project.logical.containers.clone(),
        },
    )?;
    write_yaml(
        &dir.join("logical/relationships.yaml"),
        &RelationshipsFile {
            relationships: project.logical.relationships.clone(),
        },
    )?;

    for env in &project.environments {
        write_yaml(&environment_path(dir, &env.slug), env)?;
    }

    Ok(())
}

/// 從資料夾讀出專案。
pub fn load(dir: &Path) -> Result<Project> {
    let meta: ProjectFile = read_yaml(&dir.join("project.yaml"))?;
    let systems: SystemsFile = read_yaml(&dir.join("logical/systems.yaml"))?;
    let containers: ContainersFile = read_yaml(&dir.join("logical/containers.yaml"))?;
    let relationships: RelationshipsFile = read_yaml(&dir.join("logical/relationships.yaml"))?;

    let mut seen = HashSet::new();
    let mut environments = Vec::with_capacity(meta.environments.len());
    for env_slug in &meta.environments {
        if !seen.insert(env_slug.clone()) {
            return Err(RepositoryError::DuplicateEnvironment(env_slug.clone()));
        }
        environments.push(read_yaml(&environment_path(dir, env_slug))?);
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

fn environment_path(dir: &Path, env_slug: &str) -> PathBuf {
    dir.join("environments").join(format!("{env_slug}.yaml"))
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

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_yaml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = yaml_serde::to_string(value).map_err(|e| RepositoryError::Yaml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    fs::write(path, text).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    yaml_serde::from_str(&text).map_err(|e| RepositoryError::Yaml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}
