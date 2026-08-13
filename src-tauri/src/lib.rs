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

pub mod mcp;

use std::path::PathBuf;
use std::sync::Mutex;

use loom_core::Project;
use loom_core::batch::{self, BatchPlan, BatchSpec};
use loom_core::connect::{self, Choice, Proposal};
use loom_core::coverage::{self, Matrix};
use loom_core::edit::{self, Edit, Fix, FixValue, Impact};
use loom_core::history::History;
use loom_core::importer;
use loom_core::inventory::{self, Table};
use loom_core::lint::{self, Finding, Rule, Severity};
use loom_core::plan::{self, Plan};
use loom_core::repository;
use loom_core::resource::{self, Kind, Resource};
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
pub struct Opened {
    root: Option<PathBuf>,
    pub history: Option<History>,
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

fn lock_state<'a>(state: &'a State<'_>) -> Result<std::sync::MutexGuard<'a, Opened>, Failure> {
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
fn opened_project(opened: &mut Opened) -> Result<(&PathBuf, &mut History), Failure> {
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
    let mut opened = lock_state(&state)?;
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
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
    Ok(snapshot(root, history))
}

/// 「如果做這次修改，lint 會多出什麼、少掉什麼」。**不改動任何東西。**
///
/// 刪除之前一定要先問這個。判斷不在這裡——這一層只是轉接。
#[tauri::command]
#[specta::specta]
fn preview_edit(state: State<'_>, edit: Edit) -> Result<Impact, Failure> {
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;
    edit::preview(history.project(), &edit).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
fn apply_edit(state: State<'_>, edit: Edit) -> Result<Snapshot, Failure> {
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
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
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
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
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;
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
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;
    let project = history.project();
    let env = project.environment(&environment).ok_or_else(|| Failure {
        message: format!("找不到環境 {environment}"),
    })?;
    Ok(connect::choices(project, env, end))
}

/// 算出「這批會建出什麼」，並且擋掉會撞名的。**不改動任何東西。**
///
/// L001 報在**服務**上時的修法（那個服務在這個環境一台都還沒建）。
#[tauri::command]
#[specta::specta]
fn preview_batch(
    state: State<'_>,
    environment: loom_core::id::Id,
    spec: BatchSpec,
) -> Result<BatchPlan, Failure> {
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;
    let project = history.project();
    let env = project.environment(&environment).ok_or_else(|| Failure {
        message: format!("找不到環境 {environment}"),
    })?;
    batch::plan(project, env, &spec).map_err(Into::into)
}

/// 資源清單：每種資源一張表。
///
/// 欄位與格子都是 Rust 算好的——「這個服務在各環境幾台」要走一次落地比對，
/// 那是模型知識，放前端就是把 `coverage` 再寫一遍。
#[tauri::command]
#[specta::specta]
fn resource_tables(
    state: State<'_>,
    environment: Option<loom_core::id::Id>,
) -> Result<Vec<Table>, Failure> {
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;
    Ok(inventory::tables(history.project(), environment.as_ref()))
}

/// 一個空白的新資源，給表單當起點。
///
/// id 在這裡就發好，所以 `apply` 是決定性的——復原之後重做會得到
/// 同一個元素，而不是一個新 UUID。
#[tauri::command]
#[specta::specta]
fn blank_resource(
    kind: Kind,
    environment: Option<loom_core::id::Id>,
    owner: Option<loom_core::id::Id>,
) -> Result<Resource, Failure> {
    Ok(resource::blank(kind, environment, owner))
}

/// 在一個空資料夾裡開一個新專案。
///
/// # 為什麼要求資料夾是空的
///
/// 這個操作會在裡面寫 `project.yaml` 與 `logical/`。選到一個已經有東西的
/// 資料夾（例如使用者的家目錄）就會蓋掉別人的檔案，而那沒辦法復原。
#[tauri::command]
#[specta::specta]
fn create_project(state: State<'_>, path: String, name: String) -> Result<Snapshot, Failure> {
    let root = PathBuf::from(&path);
    if root.exists()
        && std::fs::read_dir(&root)
            .map_err(|e| Failure {
                message: format!("讀不到 {path}：{e}"),
            })?
            .next()
            .is_some()
    {
        return Err(Failure {
            message: format!("{path} 不是空的。新專案要開在空資料夾裡，才不會蓋掉裡面的東西。"),
        });
    }

    let project = resource::new_project(&name);
    repository::save_to_dir(&project, &root)?;

    let history = History::opened(project);
    let out = snapshot(&root, &history);
    let mut opened = lock_state(&state)?;
    opened.root = Some(root);
    opened.history = Some(history);
    opened.pending = None;
    Ok(out)
}

/// 退回上一步。已經到底了就原樣回傳——這不是錯誤，
/// 使用者多按一次 Cmd+Z 不該看到紅字。
#[tauri::command]
#[specta::specta]
fn undo(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
    history.undo();
    Ok(snapshot(root, history))
}

#[tauri::command]
#[specta::specta]
fn redo(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
    history.redo();
    Ok(snapshot(root, history))
}

/// 讀一張表，算出「如果匯進去會發生什麼」。**不改動任何東西。**
#[tauri::command]
#[specta::specta]
fn preview_import(state: State<'_>, path: String) -> Result<Plan, Failure> {
    let sheet = read_sheet(&path)?;
    let mut opened = lock_state(&state)?;
    let (_, history) = opened_project(&mut opened)?;

    let (plan_result, after) = plan::plan(history.project(), &sheet)?;
    opened.pending = Some(after);
    Ok(plan_result)
}

/// 套用剛剛預覽過的那一份。
///
/// 走 `History::replace` 而不是直接換掉專案——匯入會動到成百上千個地方，
/// 是最需要「按錯了能退回去」的操作。
#[tauri::command]
#[specta::specta]
fn apply_import(state: State<'_>) -> Result<Snapshot, Failure> {
    let mut opened = lock_state(&state)?;
    let Some(after) = opened.pending.take() else {
        return Err(Failure {
            message: "沒有等待套用的匯入。請先預覽。".into(),
        });
    };
    let (root, history) = opened_project(&mut opened)?;
    history.replace(after, "匯入");
    Ok(snapshot(root, history))
}

#[tauri::command]
#[specta::specta]
fn cancel_import(state: State<'_>) -> Result<(), Failure> {
    lock_state(&state)?.pending = None;
    Ok(())
}

/// 副檔名決定怎麼讀。這是轉接，不是判斷。
fn read_sheet(path: &str) -> Result<importer::Sheet, Failure> {
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
    let mut opened = lock_state(&state)?;
    let (root, history) = opened_project(&mut opened)?;
    repository::save_to_dir(history.project(), root)?;
    // 只有真的寫成功才記下來，否則 dirty 會說謊，使用者會以為存過了。
    history.mark_saved();
    Ok(snapshot(root, history))
}

/// 打開給 AI Agent 用的本機端點。
///
/// 判斷與工具都在 `loom-mcp`，這裡只負責開關與把狀態交給畫面。
#[tauri::command]
#[specta::specta]
fn start_mcp(app: tauri::AppHandle) -> Result<mcp::McpStatus, Failure> {
    mcp::start(&app).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
fn stop_mcp(app: tauri::AppHandle) -> Result<mcp::McpStatus, Failure> {
    mcp::stop(&app).map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
fn mcp_status(app: tauri::AppHandle) -> Result<mcp::McpStatus, Failure> {
    mcp::status_of(&app).map_err(Into::into)
}

/// 改 AI 助手的偏好：埠、要不要 token、要不要自動啟用。
///
/// 端點正在跑的話會重開——埠與 token 都是啟動時決定的。
#[tauri::command]
#[specta::specta]
fn set_mcp_config(
    app: tauri::AppHandle,
    port: Option<u16>,
    require_token: bool,
    autostart: bool,
) -> Result<mcp::McpStatus, Failure> {
    mcp::set_config(&app, port, require_token, autostart).map_err(Into::into)
}

/// 換一組新的 token。舊的立刻失效。
#[tauri::command]
#[specta::specta]
fn regenerate_mcp_token(app: tauri::AppHandle) -> Result<mcp::McpStatus, Failure> {
    mcp::regenerate_token(&app).map_err(Into::into)
}

/// 給使用者複製到 Agent 設定檔裡的那一段 JSON。**含 token。**
///
/// token 只從這裡出去，狀態查詢不會帶——狀態會被畫面到處傳，
/// 而 token 只該在使用者主動要求時出現一次。
#[tauri::command]
#[specta::specta]
fn mcp_config(app: tauri::AppHandle) -> Result<Option<String>, Failure> {
    mcp::config_snippet(&app).map_err(Into::into)
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
        preview_batch,
        resource_tables,
        blank_resource,
        create_project,
        start_mcp,
        stop_mcp,
        mcp_status,
        mcp_config,
        set_mcp_config,
        regenerate_mcp_token,
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
            app.manage(Mutex::new(mcp::Server::default()));
            // 使用者上次打開過就自動打開。一個每次都要重設的偏好等於沒有偏好。
            if mcp::config(&app.handle().clone()).autostart {
                let _ = mcp::start(&app.handle().clone());
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Tauri 啟動失敗");
}
