//! 檔案存取的 port。
//!
//! 這是 hexagonal 意義上的次要 port：核心說「我要寫這個路徑」，
//! 至於真的寫到磁碟、寫到記憶體，還是之後寫到別的地方，核心不需要知道。
//!
//! # 為什麼切在「檔案」這一層，而不是「專案」這一層
//!
//! 直覺上會想定義 `trait ProjectStore { fn load() -> Project }`。但那樣一來，
//! 記憶體版本會**完全繞過** YAML 佈局——測試就測不到「專案該拆成哪幾個檔、
//! 每個檔裡放什麼」這些真正的領域知識。
//!
//! 切在檔案這一層，[`crate::repository`] 仍然負責決定佈局與 YAML，
//! 換掉的只有「這些字串放哪裡」。記憶體版本因此能驗證完整的佈局。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 存取失敗的種類。
///
/// **`NotFound` 一定要跟其他失敗分開。** 有些檔案不存在是正常的
/// （例如還沒對帳過就沒有 base 快照），可以安靜地當成空的；
/// 但權限不足、檔案毀損若也被當成空的，使用者會以為資料消失了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreErrorKind {
    NotFound,
    Other,
}

/// 存取失敗。路徑一律附上，否則使用者不知道是哪個檔出事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError {
    pub path: String,
    pub kind: StoreErrorKind,
    pub message: String,
}

impl StoreError {
    pub fn is_not_found(&self) -> bool {
        self.kind == StoreErrorKind::NotFound
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}：{}", self.path, self.message)
    }
}

impl std::error::Error for StoreError {}

/// 以相對路徑存取文字檔。
///
/// 路徑一律用 `/` 分隔，由實作自行轉換成平台的形式。
pub trait FileStore {
    fn read(&self, path: &str) -> Result<String, StoreError>;
    fn write(&mut self, path: &str, contents: &str) -> Result<(), StoreError>;
    /// 刪掉一個檔案。**檔案本來就不在也算成功**——呼叫端要的是
    /// 「這個路徑之後不存在」，而不是「剛剛真的刪了一次」。
    ///
    /// 目前唯一的用途是改名（見 [`crate::diagrams::migrate`]）：
    /// 寫到新路徑之後，舊的那份要消失，不然資料夾裡會留下一個
    /// 看起來還有效、實際上沒有人管的 `.drawio`。
    fn remove(&mut self, path: &str) -> Result<(), StoreError>;
}

/// 真正的磁碟。`root` 底下的相對路徑。
#[derive(Debug, Clone)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &str) -> PathBuf {
        path.split('/')
            .fold(self.root.clone(), |acc, part| acc.join(part))
    }
}

impl FileStore for FsStore {
    fn read(&self, path: &str) -> Result<String, StoreError> {
        fs::read_to_string(self.resolve(path)).map_err(|e| StoreError {
            path: path.to_string(),
            kind: match e.kind() {
                std::io::ErrorKind::NotFound => StoreErrorKind::NotFound,
                _ => StoreErrorKind::Other,
            },
            message: e.to_string(),
        })
    }

    fn write(&mut self, path: &str, contents: &str) -> Result<(), StoreError> {
        let full = self.resolve(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError {
                path: path.to_string(),
                kind: StoreErrorKind::Other,
                message: e.to_string(),
            })?;
        }
        fs::write(&full, contents).map_err(|e| StoreError {
            path: path.to_string(),
            kind: StoreErrorKind::Other,
            message: e.to_string(),
        })
    }

    fn remove(&mut self, path: &str) -> Result<(), StoreError> {
        match fs::remove_file(self.resolve(path)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError {
                path: path.to_string(),
                kind: StoreErrorKind::Other,
                message: e.to_string(),
            }),
        }
    }
}

/// 記憶體版本。測試用，也可以拿來「先算出會寫成什麼，再讓使用者確認」。
///
/// 用 [`BTreeMap`] 而非 `HashMap`：路徑順序穩定，測試才好比對。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryStore {
    files: BTreeMap<String, String>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 目前有哪些檔案，已依路徑排序。
    pub fn paths(&self) -> Vec<&str> {
        self.files.keys().map(String::as_str).collect()
    }

    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl FileStore for MemoryStore {
    fn read(&self, path: &str) -> Result<String, StoreError> {
        self.files.get(path).cloned().ok_or_else(|| StoreError {
            path: path.to_string(),
            kind: StoreErrorKind::NotFound,
            message: "找不到這個檔案".into(),
        })
    }

    fn write(&mut self, path: &str, contents: &str) -> Result<(), StoreError> {
        self.files.insert(path.to_string(), contents.to_string());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), StoreError> {
        self.files.remove(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips() {
        let mut store = MemoryStore::new();
        store
            .write("logical/containers.yaml", "containers: []")
            .unwrap();
        assert_eq!(
            store.read("logical/containers.yaml").unwrap(),
            "containers: []"
        );
    }

    #[test]
    fn reading_a_missing_file_reports_the_path_and_not_found() {
        let store = MemoryStore::new();
        let err = store.read("project.yaml").unwrap_err();
        assert_eq!(err.path, "project.yaml");
        assert!(err.is_not_found(), "必須能跟其他失敗分開");
    }

    #[test]
    fn paths_come_back_in_a_stable_order() {
        let mut store = MemoryStore::new();
        store.write("environments/prod.yaml", "").unwrap();
        store.write("project.yaml", "").unwrap();
        store.write("environments/dev.yaml", "").unwrap();

        assert_eq!(
            store.paths(),
            vec![
                "environments/dev.yaml",
                "environments/prod.yaml",
                "project.yaml"
            ]
        );
    }

    #[test]
    fn rewriting_the_same_path_overwrites() {
        let mut store = MemoryStore::new();
        store.write("a.yaml", "舊的").unwrap();
        store.write("a.yaml", "新的").unwrap();
        assert_eq!(store.get("a.yaml"), Some("新的"));
        assert_eq!(store.paths().len(), 1);
    }
}
