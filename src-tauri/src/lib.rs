//! Tauri 殼。
//!
//! # 這一層只做轉接
//!
//! 每個 command 都應該短到「一眼看得出它沒有偷做決定」。判斷全在 `loom-core`，
//! 那裡可以用 `cargo test` 驗證，不必開視窗。
//!
//! 如果哪天某個 command 開始長出 `if`，那就是規則漏到殼裡了，該搬回去。
//!
//! # CSP
//!
//! `tauri.conf.json` 裡的 CSP 是階段 0 實測過的值（見 `docs/stage0-findings.md`）。
//! Tauri 的設定檔不接受任何未知欄位，所以那份設定沒辦法寫註解——備註只能放這裡。
//!
//! 等階段 6 內嵌 draw.io 時要補 `frame-src drawio:`，並註冊 `drawio://` 協定。
//!
//! # 型別同步
//!
//! TypeScript 型別由 [`tauri_specta`] 從這裡的 command 簽名自動產生，
//! 寫進 `src/lib/bindings.ts`。**不要手改那個檔**——`mise run bindings`
//! 會重新產生，`mise run check` 會檢查它是不是最新的。

use std::path::PathBuf;
use std::sync::Mutex;

use loom_core::Project;
use loom_core::coverage::{self, Matrix};
use loom_core::lint::{self, Finding};
use loom_core::repository;
use serde::Serialize;
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// 目前開著的專案。
///
/// 桌面應用一次只開一個專案，所以放在 app state 就夠了，不需要更複雜的東西。
#[derive(Default)]
struct Opened {
    root: Option<PathBuf>,
    project: Option<Project>,
}

type State<'a> = tauri::State<'a, Mutex<Opened>>;

/// 前端拿到的錯誤。
///
/// 只是字串——使用者需要的是「哪裡出錯、怎麼修」，不是錯誤型別的階層。
#[derive(Debug, Serialize, specta::Type)]
pub struct Failure {
    message: String,
}

impl<T: std::fmt::Display> From<T> for Failure {
    fn from(e: T) -> Self {
        Self {
            message: e.to_string(),
        }
    }
}

/// 開一個專案之後，前端需要的所有東西一次給齊。
///
/// 分成三個 command 來回問會有中間狀態（模型換了但 lint 還是舊的），
/// 畫面就會短暫地自相矛盾。
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub root: String,
    pub project: Project,
    pub findings: Vec<Finding>,
    pub matrix: Matrix,
}

fn snapshot(root: &std::path::Path, project: Project) -> Snapshot {
    Snapshot {
        root: root.display().to_string(),
        findings: lint::lint(&project),
        matrix: coverage::coverage(&project),
        project,
    }
}

#[tauri::command]
#[specta::specta]
fn open_project(state: State<'_>, path: String) -> Result<Snapshot, Failure> {
    let root = PathBuf::from(path);
    let project = repository::load_from_dir(&root)?;

    let out = snapshot(&root, project.clone());
    let mut opened = state.lock().map_err(|_| Failure {
        message: "內部狀態毀損".into(),
    })?;
    opened.root = Some(root);
    opened.project = Some(project);
    Ok(out)
}

/// 重新檢查目前開著的專案。
///
/// 前端會在使用者改動後去抖動地呼叫這個——實測數百條連線是毫秒級，
/// 所以不需要進度回報。見 `docs/scale-limits.md`。
#[tauri::command]
#[specta::specta]
fn recheck(state: State<'_>) -> Result<Snapshot, Failure> {
    let opened = state.lock().map_err(|_| Failure {
        message: "內部狀態毀損".into(),
    })?;
    let (Some(root), Some(project)) = (&opened.root, &opened.project) else {
        return Err(Failure {
            message: "還沒有開啟任何專案".into(),
        });
    };
    Ok(snapshot(root, project.clone()))
}

#[tauri::command]
#[specta::specta]
fn save_project(state: State<'_>) -> Result<Snapshot, Failure> {
    let opened = state.lock().map_err(|_| Failure {
        message: "內部狀態毀損".into(),
    })?;
    let (Some(root), Some(project)) = (&opened.root, &opened.project) else {
        return Err(Failure {
            message: "還沒有開啟任何專案".into(),
        });
    };
    repository::save_to_dir(project, root)?;
    Ok(snapshot(root, project.clone()))
}

/// 產生 TS 型別用的 builder。`main.rs` 與型別產生器共用同一份，
/// 所以不可能出現「command 加了但型別沒更新」。
pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![open_project, recheck, save_project])
}

pub fn run() {
    let builder = builder();

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Mutex::new(Opened::default()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Tauri 啟動失敗");
}
