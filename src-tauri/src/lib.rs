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
use loom_core::connect::{self, Choice, Proposal};
use loom_core::coverage::{self, Matrix};
use loom_core::edit::{self, Edit, Fix, FixValue, Impact};
use loom_core::history::History;
use loom_core::importer;
use loom_core::lint::{self, Finding, Rule, Severity};
use loom_core::plan::{self, Plan};
use loom_core::repository;
use loom_core::table::{self, Row};
use serde::Serialize;
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// 目前開著的專案。
///
/// 桌面應用一次只開一個專案，所以放在 app state 就夠了，不需要更複雜的東西。
///
/// 專案本體包在 [`History`] 裡，而且**只能透過它修改**——這樣「改了就能復原」
/// 不是靠每個 command 各自記得，而是型別上就繞不過去。
#[derive(Default)]
struct Opened {
    root: Option<PathBuf>,
    history: Option<History>,
    /// 預覽過、還沒套用的匯入結果。
    ///
    /// 存起來而不是套用時重跑一次：重跑會產生新的 UUID，使用者按下「套用」
    /// 拿到的就不是他剛剛看過的那份東西了。
    pending: Option<Project>,
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

/// 一項發現，外加它的嚴重度與修法。
///
/// `Rule::severity()` 在 Rust 是一個方法，序列化不會帶過去。若讓前端自己
/// 抄一份「哪些規則算錯誤」的對照表，新增規則時那份一定會忘記更新，
/// 而且是**靜靜地**算錯數字。所以在這裡攤平成欄位。
///
/// `fix` 同理：「L006 要填的是位址、L004 要填的是數字」是規則不是畫面，
/// 由 `loom_core::edit::fix_for` 決定，前端只負責照著長出控制項。
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FindingView {
    pub rule: Rule,
    pub severity: Severity,
    pub environment: Option<loom_core::id::Id>,
    pub subject: loom_core::id::Id,
    /// 問題出在連線的哪一端。前端不需要看懂它，但**必須原樣送回來**——
    /// 兩端都是萬用字元時，少了它就分不出這一項在講哪一端。
    pub end: Option<loom_core::environment::ConnectionEnd>,
    pub detail: String,
    /// 這一項能不能用單一欄位修好，以及該長成什麼樣的輸入。
    /// `None` 表示要新增／改接連線，不是填一格能解決的。
    pub fix: Option<Fix>,
}

impl FindingView {
    fn of(project: &Project, f: &Finding) -> Self {
        Self {
            rule: f.rule,
            severity: f.severity(),
            environment: f.environment.clone(),
            subject: f.subject.clone(),
            end: f.end,
            detail: f.detail.clone(),
            fix: edit::fix_for(project, f),
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
    pub findings: Vec<FindingView>,
    pub matrix: Matrix,
    /// 連線表用的列。跟矩陣一樣，是同一份資料的另一種排法。
    pub rows: Vec<Row>,
    /// 目前內容跟磁碟上不一樣。關視窗前要問的就是這個。
    pub dirty: bool,
    /// 復原／重做選單要顯示的短標籤。`None` 表示按鈕要停用。
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

fn 鎖<'a>(state: &'a State<'_>) -> Result<std::sync::MutexGuard<'a, Opened>, Failure> {
    state.lock().map_err(|_| Failure {
        message: "內部狀態毀損".into(),
    })
}

/// 每個 command 都以這個結尾，所以前端永遠拿到自洽的一份：
/// 模型、lint、矩陣、表格、未儲存狀態全部來自同一個瞬間。
fn snapshot(root: &std::path::Path, history: &History) -> Snapshot {
    let project = history.project();
    Snapshot {
        root: root.display().to_string(),
        findings: lint::lint(project)
            .iter()
            .map(|f| FindingView::of(project, f))
            .collect(),
        matrix: coverage::coverage(project),
        rows: table::rows(project),
        dirty: history.is_dirty(),
        undo_label: history.undo_label().map(str::to_string),
        redo_label: history.redo_label().map(str::to_string),
        project: project.clone(),
    }
}

/// 取出目前開著的專案，沒開就給一句話。
fn 開著的(opened: &mut Opened) -> Result<(&PathBuf, &mut History), Failure> {
    match (&opened.root, &mut opened.history) {
        (Some(root), Some(history)) => Ok((root, history)),
        _ => Err(Failure {
            message: "還沒有開啟任何專案".into(),
        }),
    }
}

#[tauri::command]
#[specta::specta]
fn open_project(state: State<'_>, path: String) -> Result<Snapshot, Failure> {
    let root = PathBuf::from(path);
    let history = History::opened(repository::load_from_dir(&root)?);

    let out = snapshot(&root, &history);
    let mut opened = 鎖(&state)?;
    opened.root = Some(root);
    opened.history = Some(history);
    opened.pending = None;
    Ok(out)
}

/// 重新檢查目前開著的專案。
///
/// 前端會在使用者改動後去抖動地呼叫這個——實測數百條連線是毫秒級，
/// 所以不需要進度回報。見 `docs/scale-limits.md`。
#[tauri::command]
#[specta::specta]
fn recheck(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    Ok(snapshot(root, history))
}

/// 「如果做這次修改，lint 會多出什麼、少掉什麼」。**不改動任何東西。**
///
/// 刪除之前一定要先問這個。判斷不在這裡——這一層只是轉接。
#[tauri::command]
#[specta::specta]
fn preview_edit(state: State<'_>, edit: Edit) -> Result<Impact, Failure> {
    let mut opened = 鎖(&state)?;
    let (_, history) = 開著的(&mut opened)?;
    edit::preview(history.project(), &edit).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
fn apply_edit(state: State<'_>, edit: Edit) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    history.edit(&edit)?;
    Ok(snapshot(root, history))
}

/// 把 lint 面板上「照著修法填的那一格」變成一次修改。
///
/// 前端送回來的是**發現本身 + 使用者填了什麼**，不是 [`Edit`]——
/// 「L006 的答案要寫進哪個欄位」由 `loom_core::edit::edit_for` 決定。
/// 前端因此完全不需要知道 [`Edit`] 有哪些變體。
#[tauri::command]
#[specta::specta]
fn apply_fix(state: State<'_>, finding: Finding, value: FixValue) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    let edit = edit::edit_for(&finding, &value)?;
    history.edit(&edit)?;
    Ok(snapshot(root, history))
}

/// 照契約擬一條「這個環境應該要有」的連線。**不改動任何東西。**
///
/// L001／L002 的修法。判斷全在 `loom_core::connect`——
/// 哪幾台機器算數、多台要不要收成萬用字元，都是模型知識。
#[tauri::command]
#[specta::specta]
fn propose_connection(
    state: State<'_>,
    environment: loom_core::id::Id,
    relationship: loom_core::id::Id,
) -> Result<Proposal, Failure> {
    let mut opened = 鎖(&state)?;
    let (_, history) = 開著的(&mut opened)?;
    let project = history.project();
    let env = project.environment(&environment).ok_or_else(|| Failure {
        message: format!("找不到環境 {environment}"),
    })?;
    connect::propose(project, env, &relationship).ok_or_else(|| Failure {
        message: format!("找不到邏輯連線 {relationship}"),
    })
}

/// 這個環境裡，某一端接得上的所有地方。
#[tauri::command]
#[specta::specta]
fn connection_choices(
    state: State<'_>,
    environment: loom_core::id::Id,
    end: loom_core::environment::ConnectionEnd,
) -> Result<Vec<Choice>, Failure> {
    let mut opened = 鎖(&state)?;
    let (_, history) = 開著的(&mut opened)?;
    let project = history.project();
    let env = project.environment(&environment).ok_or_else(|| Failure {
        message: format!("找不到環境 {environment}"),
    })?;
    Ok(connect::choices(project, env, end))
}

/// 退回上一步。已經到底了就原樣回傳——這不是錯誤，
/// 使用者多按一次 Cmd+Z 不該看到紅字。
#[tauri::command]
#[specta::specta]
fn undo(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    history.undo();
    Ok(snapshot(root, history))
}

#[tauri::command]
#[specta::specta]
fn redo(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    history.redo();
    Ok(snapshot(root, history))
}

/// 讀一張表，算出「如果匯進去會發生什麼」。**不改動任何東西。**
#[tauri::command]
#[specta::specta]
fn preview_import(state: State<'_>, path: String) -> Result<Plan, Failure> {
    let sheet = 讀表(&path)?;
    let mut opened = 鎖(&state)?;
    let (_, history) = 開著的(&mut opened)?;

    let (計畫, 之後) = plan::plan(history.project(), &sheet)?;
    opened.pending = Some(之後);
    Ok(計畫)
}

/// 套用剛剛預覽過的那一份。
///
/// 走 `History::replace` 而不是直接換掉專案——匯入會動到成百上千個地方，
/// 是最需要「按錯了能退回去」的操作。
#[tauri::command]
#[specta::specta]
fn apply_import(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let Some(之後) = opened.pending.take() else {
        return Err(Failure {
            message: "沒有等待套用的匯入。請先預覽。".into(),
        });
    };
    let (root, history) = 開著的(&mut opened)?;
    history.replace(之後, "匯入");
    Ok(snapshot(root, history))
}

#[tauri::command]
#[specta::specta]
fn cancel_import(state: State<'_>) -> Result<(), Failure> {
    鎖(&state)?.pending = None;
    Ok(())
}

/// 副檔名決定怎麼讀。這是轉接，不是判斷。
fn 讀表(path: &str) -> Result<importer::Sheet, Failure> {
    let p = std::path::Path::new(path);
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("csv") => importer::read_csv(p).map_err(Into::into),
        Some("xlsx") | Some("xls") | Some("xlsm") => importer::read_xlsx(p).map_err(Into::into),
        _ => Err(Failure {
            message: format!("認不得的副檔名：{path}"),
        }),
    }
}

#[tauri::command]
#[specta::specta]
fn save_project(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = 鎖(&state)?;
    let (root, history) = 開著的(&mut opened)?;
    repository::save_to_dir(history.project(), root)?;
    // 只有真的寫成功才記下來，否則 dirty 會說謊，使用者會以為存過了。
    history.mark_saved();
    Ok(snapshot(root, history))
}

/// 產生 TS 型別用的 builder。`main.rs` 與型別產生器共用同一份，
/// 所以不可能出現「command 加了但型別沒更新」。
pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        open_project,
        recheck,
        save_project,
        preview_import,
        apply_import,
        cancel_import,
        preview_edit,
        apply_edit,
        propose_connection,
        connection_choices,
        apply_fix,
        undo,
        redo
    ])
}

pub fn run() {
    let builder = builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Mutex::new(Opened::default()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Tauri 啟動失敗");
}
