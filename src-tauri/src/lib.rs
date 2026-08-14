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
//! 值得記一筆：**`script-src` 不需要 `'unsafe-inline'` 也不需要 `'unsafe-eval'`**。
//! draw.io 跑在自己的 origin（`drawio://`）上，不受主視窗這份 CSP 管轄——
//! 那是自訂協定換來的好處，不是漏掉的設定。
//!
//! `frame-src drawio:` 是內嵌 draw.io 需要的；`img-src` 的 `blob:` 是匯出圖片用的。
//!
//! # 型別同步
//!
//! TypeScript 型別由 [`tauri_specta`] 從這裡的 command 簽名自動產生，
//! 寫進 `src/lib/bindings.ts`。**不要手改那個檔**——`mise run bindings`
//! 會重新產生，`mise run check` 會檢查它是不是最新的。

pub mod drawio;
pub mod mcp;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use loom_core::Project;
use loom_core::batch::{self, BatchPlan, BatchSpec};
use loom_core::connect::{self, Choice, Proposal};
use loom_core::coverage::{self, Matrix};
use loom_core::edit::{self, Edit, Fix, FixValue, Impact};
use loom_core::highlight::{self, Focus, Highlight};
use loom_core::history::History;
use loom_core::importer;
use loom_core::inventory::{self, Table};
use loom_core::lint::{self, Finding, Rule, Severity};
use loom_core::plan::{self, Plan};
use loom_core::repository;
use loom_core::resource::{self, Kind, Resource};
use loom_core::table::{self, Row};
use loom_core::wiring;
use serde::Serialize;
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// 一個開著的專案的可變狀態。
///
/// 專案本體包在 [`History`] 裡，而且**只能透過它修改**——這樣「改了就能復原」
/// 不是靠每個 command 各自記得，而是型別上就繞不過去。
pub struct Opened {
    pub history: History,
    /// 預覽過、還沒套用的匯入結果。
    ///
    /// 存起來而不是套用時重跑一次：重跑會產生新的 UUID，使用者按下「套用」
    /// 拿到的就不是他剛剛看過的那份東西了。
    pending: Option<Project>,
}

/// 一個開著的專案。
///
/// # 身分是資料夾路徑，不是視窗
///
/// **同一個專案同時只會被一扇視窗持有。** 再開一次同一個資料夾，是把那扇
/// 視窗叫到前面，而不是開第二份 [`History`]——兩份的話兩邊各自編輯、
/// 各自存檔，後存的會安靜地蓋掉先存的，而這個工具的賣點正好是「怕漏」。
///
/// 因為是一對一，**視窗的 label 就是專案的身分**。command 因此不必從前端
/// 收專案路徑，直接跟 Tauri 要「是誰呼叫我的」就好——前端也就沒有辦法指錯。
pub struct OpenProject {
    /// 正規化過的資料夾路徑。這是它的身分。
    root: PathBuf,
    /// 哪一扇視窗在看它。
    window: String,
    state: Mutex<Opened>,
}

/// 開著的專案們，照開啟順序排。
///
/// # 為什麼是兩層鎖
///
/// 外層只用來查表與增刪，短到不會卡人；每個專案有自己的鎖，所以 Agent 對
/// A 專案送一批三百項的時候，B 專案的視窗不會跟著凍住。
///
/// # ⚠️ 不變條件：拿著內層鎖的時候絕對不碰外層
///
/// 反過來可以（外層讀鎖裡短暫拿內層）。這條沒寫下來的話，之後很容易加出
/// 一個只在多視窗又剛好同時操作時才發生的死鎖——那種東西幾乎抓不到。
///
/// # 另一個不變條件：登記簿裡的專案 ⟺ 有視窗在看它
///
/// 視窗關掉就從這裡移除（見 `run()` 裡的 `Destroyed`）。少了這條，
/// Agent 會去改一個沒有任何人看得到的專案。
#[derive(Default)]
pub struct Desk(RwLock<Vec<Arc<OpenProject>>>);

impl Desk {
    /// 這扇視窗正在看的專案。
    fn of_window(&self, label: &str) -> Option<Arc<OpenProject>> {
        let open = self.0.read().ok()?;
        open.iter().find(|p| p.window == label).cloned()
    }

    /// 哪一扇視窗開著這個資料夾。
    fn of_root(&self, root: &Path) -> Option<Arc<OpenProject>> {
        let open = self.0.read().ok()?;
        open.iter().find(|p| p.root == root).cloned()
    }

    /// 登記一個新開的專案，並把這扇視窗原本持有的那個放掉。
    fn bind(&self, root: PathBuf, window: String, opened: Opened) -> Result<(), Failure> {
        let mut open = self.0.write().map_err(|_| poisoned())?;
        open.retain(|p| p.window != window);
        open.push(Arc::new(OpenProject {
            root,
            window,
            state: Mutex::new(opened),
        }));
        Ok(())
    }

    /// 全部開著的專案，照開啟順序。給 MCP 那層列清單與解析用。
    ///
    /// 回的是 `Arc` 的複本並且**立刻放掉外層鎖**——呼叫端接下來要一個一個
    /// 拿內層鎖，那時候不能還握著外層。
    pub fn all(&self) -> Vec<Arc<OpenProject>> {
        self.0.read().map(|open| open.clone()).unwrap_or_default()
    }

    /// 視窗關掉了。
    fn release(&self, window: &str) {
        if let Ok(mut open) = self.0.write() {
            open.retain(|p| p.window != window);
        }
    }
}

type State<'a> = tauri::State<'a, Desk>;

fn poisoned() -> Failure {
    Failure {
        message: "內部狀態毀損".into(),
    }
}

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

/// 篩選面板需要的一切。
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FocusView {
    /// 可以勾的契約。
    pub contracts: Vec<highlight::Contract>,
    /// 照目前的勾選，誰該亮。
    pub highlight: Highlight,
    /// 這個環境總共幾個形狀。「亮了幾個 / 共幾個」的分母——
    /// 少了它，使用者不知道自己篩掉了多少，也就不知道自己在看的是一小角。
    pub shapes: u32,
}

/// 拿到**這扇視窗**正在看的那個專案，做完事情就放掉。
///
/// 每個需要專案的 command 都以它開頭。專案是誰由視窗決定，所以前端
/// 完全不必知道自己開的是哪一份——也就不可能傳錯。
fn with_opened<R>(
    desk: &State<'_>,
    window: &tauri::WebviewWindow,
    f: impl FnOnce(&Path, &mut Opened) -> Result<R, Failure>,
) -> Result<R, Failure> {
    let project = desk.of_window(window.label()).ok_or_else(|| Failure {
        message: "還沒有開啟任何專案".into(),
    })?;
    let mut opened = project.state.lock().map_err(|_| poisoned())?;
    f(&project.root, &mut opened)
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

/// 開啟的結果。
///
/// 「已經開在別的視窗」不是錯誤——使用者只是選了一個他已經在看的東西，
/// 而正確的回應是把那扇窗叫到前面。做成錯誤的話畫面會跳紅字，
/// 像是他做錯了什麼。
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OpenOutcome {
    /// 開好了，這扇視窗現在看的是它。
    Loaded(Box<Snapshot>),
    /// 已經開在另一扇視窗，那扇已經被叫到前面了。這扇視窗維持原狀。
    Elsewhere { name: String },
}

/// 把路徑正規化成專案的身分。
///
/// `canonicalize` 會解掉 `..`、符號連結與大小寫差異——少了它，
/// `~/work/a.loom` 與 `~/work/./a.loom` 會被當成兩個專案，
/// 於是同一份東西真的被開成兩份，而那正是這裡最想避免的事。
fn identity(path: &str) -> Result<PathBuf, Failure> {
    std::fs::canonicalize(path).map_err(|e| Failure {
        message: format!("找不到資料夾 {path}：{e}"),
    })
}

#[tauri::command]
#[specta::specta]
fn open_project(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    path: String,
) -> Result<OpenOutcome, Failure> {
    let root = identity(&path)?;

    // 已經有人在看它了。除非那個人就是自己——那就是重新載入。
    if let Some(existing) = desk.of_root(&root)
        && existing.window != window.label()
    {
        let name = existing
            .state
            .lock()
            .map(|o| o.history.project().name.clone())
            .unwrap_or_else(|_| root.display().to_string());
        // 叫到前面。使用者要的東西已經在畫面上了，只是被蓋住。
        if let Some(w) = window.get_webview_window(&existing.window) {
            let _ = w.set_focus();
        }
        return Ok(OpenOutcome::Elsewhere { name });
    }

    let history = History::opened(repository::load_from_dir(&root)?);
    let out = snapshot(&root, &history);
    retitle(&window, &history);
    desk.bind(
        root,
        window.label().to_string(),
        Opened {
            history,
            pending: None,
        },
    )?;
    Ok(OpenOutcome::Loaded(Box::new(out)))
}

/// 視窗標題掛上專案名。
///
/// 多視窗之下，標題是唯一能從工作列或 Mission Control 分出誰是誰的東西。
fn retitle(window: &tauri::WebviewWindow, history: &History) {
    let _ = window.set_title(&format!("{} — diagram-loom", history.project().name));
}

/// 再開一扇空視窗。
///
/// # 為什麼由 Rust 開，不是前端自己開
///
/// 前端呼叫 `WebviewWindow.create()` 需要 `core:webview:allow-create-webview-window`
/// 這個權限，而 `capabilities/default.json` 是安全邊界不是設定樣板——
/// 從 Rust 開就完全不需要它。
///
/// label 用遞增的數字。它只是視窗的名字，**不是專案的身分**——
/// 專案的身分是資料夾路徑，兩者由登記簿配起來。
#[tauri::command]
#[specta::specta]
fn new_window(app: tauri::AppHandle) -> Result<(), Failure> {
    let label = (1..)
        .map(|n| format!("w-{n}"))
        .find(|l| app.get_webview_window(l).is_none())
        .expect("總會找得到一個沒被用掉的號碼");

    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::default())
        .title("diagram-loom")
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 600.0)
        .build()
        .map_err(|e| Failure {
            message: format!("開不了新視窗：{e}"),
        })?;
    Ok(())
}

/// 重新檢查目前開著的專案。
///
/// 前端會在使用者改動後去抖動地呼叫這個——實測數百條連線是毫秒級，
/// 所以不需要進度回報。見 `docs/scale-limits.md`。
#[tauri::command]
#[specta::specta]
fn recheck(desk: State<'_>, window: tauri::WebviewWindow) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| Ok(snapshot(root, &o.history)))
}

/// 「如果做這次修改，lint 會多出什麼、少掉什麼」。**不改動任何東西。**
///
/// 刪除之前一定要先問這個。判斷不在這裡——這一層只是轉接。
#[tauri::command]
#[specta::specta]
fn preview_edit(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    edit: Edit,
) -> Result<Impact, Failure> {
    with_opened(&desk, &window, |_, o| {
        edit::preview(o.history.project(), &edit).map_err(Into::into)
    })
}

#[tauri::command]
#[specta::specta]
fn apply_edit(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    edit: Edit,
) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        o.history.edit(&edit)?;
        Ok(snapshot(root, &o.history))
    })
}

/// 把 lint 面板上「照著修法填的那一格」變成一次修改。
///
/// 前端送回來的是**發現本身 + 使用者填了什麼**，不是 [`Edit`]——
/// 「L006 的答案要寫進哪個欄位」由 `loom_core::edit::edit_for` 決定。
/// 前端因此完全不需要知道 [`Edit`] 有哪些變體。
#[tauri::command]
#[specta::specta]
fn apply_fix(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    finding: Finding,
    value: FixValue,
) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        let edit = edit::edit_for(&finding, &value)?;
        o.history.edit(&edit)?;
        Ok(snapshot(root, &o.history))
    })
}

/// 照契約擬一條「這個環境應該要有」的連線。**不改動任何東西。**
///
/// L001／L002 的修法。判斷全在 `loom_core::connect`——
/// 哪幾台機器算數、多台要不要收成萬用字元，都是模型知識。
#[tauri::command]
#[specta::specta]
fn propose_connection(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: loom_core::id::Id,
    relationship: loom_core::id::Id,
) -> Result<Proposal, Failure> {
    with_opened(&desk, &window, |_, o| {
        let project = o.history.project();
        let env = environment_of(project, &environment)?;
        connect::propose(project, env, &relationship).ok_or_else(|| Failure {
            message: format!("找不到邏輯連線 {relationship}"),
        })
    })
}

/// 這個環境裡，某一端接得上的所有地方。
#[tauri::command]
#[specta::specta]
fn connection_choices(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: loom_core::id::Id,
    end: loom_core::environment::ConnectionEnd,
) -> Result<Vec<Choice>, Failure> {
    with_opened(&desk, &window, |_, o| {
        let project = o.history.project();
        Ok(connect::choices(
            project,
            environment_of(project, &environment)?,
            end,
        ))
    })
}

/// 算出「這批會建出什麼」，並且擋掉會撞名的。**不改動任何東西。**
///
/// L001 報在**服務**上時的修法（那個服務在這個環境一台都還沒建）。
#[tauri::command]
#[specta::specta]
fn preview_batch(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: loom_core::id::Id,
    spec: BatchSpec,
) -> Result<BatchPlan, Failure> {
    with_opened(&desk, &window, |_, o| {
        let project = o.history.project();
        batch::plan(project, environment_of(project, &environment)?, &spec).map_err(Into::into)
    })
}

/// 資源清單：每種資源一張表。
///
/// 欄位與格子都是 Rust 算好的——「這個服務在各環境幾台」要走一次實體比對，
/// 那是模型知識，放前端就是把 `coverage` 再寫一遍。
#[tauri::command]
#[specta::specta]
fn resource_tables(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: Option<loom_core::id::Id>,
) -> Result<Vec<Table>, Failure> {
    with_opened(&desk, &window, |_, o| {
        Ok(inventory::tables(o.history.project(), environment.as_ref()))
    })
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
fn create_project(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    path: String,
    name: String,
) -> Result<Snapshot, Failure> {
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

    // 建好之後才 canonicalize——資料夾要先存在才解得開。
    let root = identity(&root.to_string_lossy())?;
    let history = History::opened(project);
    let out = snapshot(&root, &history);
    retitle(&window, &history);
    desk.bind(
        root,
        window.label().to_string(),
        Opened {
            history,
            pending: None,
        },
    )?;
    Ok(out)
}

/// 退回上一步。已經到底了就原樣回傳——這不是錯誤，
/// 使用者多按一次 Cmd+Z 不該看到紅字。
#[tauri::command]
#[specta::specta]
fn undo(desk: State<'_>, window: tauri::WebviewWindow) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        o.history.undo();
        Ok(snapshot(root, &o.history))
    })
}

#[tauri::command]
#[specta::specta]
fn redo(desk: State<'_>, window: tauri::WebviewWindow) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        o.history.redo();
        Ok(snapshot(root, &o.history))
    })
}

/// 讀一張表，算出「如果匯進去會發生什麼」。**不改動任何東西。**
#[tauri::command]
#[specta::specta]
fn preview_import(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    path: String,
) -> Result<Plan, Failure> {
    let sheet = read_sheet(&path)?;
    with_opened(&desk, &window, |_, o| {
        let (plan_result, after) = plan::plan(o.history.project(), &sheet)?;
        o.pending = Some(after);
        Ok(plan_result)
    })
}

/// 套用剛剛預覽過的那一份。
///
/// 走 `History::replace` 而不是直接換掉專案——匯入會動到成百上千個地方，
/// 是最需要「按錯了能退回去」的操作。
#[tauri::command]
#[specta::specta]
fn apply_import(desk: State<'_>, window: tauri::WebviewWindow) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        let Some(after) = o.pending.take() else {
            return Err(Failure {
                message: "沒有等待套用的匯入。請先預覽。".into(),
            });
        };
        o.history.replace(after, "匯入");
        Ok(snapshot(root, &o.history))
    })
}

#[tauri::command]
#[specta::specta]
fn cancel_import(desk: State<'_>, window: tauri::WebviewWindow) -> Result<(), Failure> {
    with_opened(&desk, &window, |_, o| {
        o.pending = None;
        Ok(())
    })
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
fn save_project(desk: State<'_>, window: tauri::WebviewWindow) -> Result<Snapshot, Failure> {
    with_opened(&desk, &window, |root, o| {
        repository::save_to_dir(o.history.project(), root)?;
        // 只有真的寫成功才記下來，否則 dirty 會說謊，使用者會以為存過了。
        o.history.mark_saved();
        Ok(snapshot(root, &o.history))
    })
}

/// 一個環境裡的連線展開成圖上要畫的線。
///
/// # 為什麼是獨立的 command，不塞進 [`Snapshot`]
///
/// 萬用字元是 N×M——12 台連 12 台就是 144 條。整份專案的展開結果
/// 可能比模型本身還大，而它只有「圖」這個檢視在看。塞進 snapshot 的話
/// 每一次編輯都要付這個代價。
#[tauri::command]
#[specta::specta]
fn diagram_links(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: loom_core::id::Id,
) -> Result<Vec<wiring::Link>, Failure> {
    with_opened(&desk, &window, |_, o| {
        let project = o.history.project();
        Ok(wiring::links(
            project,
            environment_of(project, &environment)?,
        ))
    })
}

/// 篩選面板要列的契約，以及照目前的勾選「誰該亮」。
///
/// 兩件事一次給：分開問的話，畫面會有一瞬間拿舊的清單配新的亮法。
#[tauri::command]
#[specta::specta]
fn diagram_focus(
    desk: State<'_>,
    window: tauri::WebviewWindow,
    environment: loom_core::id::Id,
    focus: Focus,
) -> Result<FocusView, Failure> {
    with_opened(&desk, &window, |_, o| {
        let project = o.history.project();
        Ok(FocusView {
            contracts: highlight::contracts(project, &environment),
            highlight: highlight::highlight(project, &environment, &focus),
            shapes: highlight::shape_count(environment_of(project, &environment)?),
        })
    })
}

/// 「找不到環境 X」這句話原本抄了五遍。
fn environment_of<'a>(
    project: &'a Project,
    id: &loom_core::id::Id,
) -> Result<&'a loom_core::environment::Environment, Failure> {
    project.environment(id).ok_or_else(|| Failure {
        message: format!("找不到環境 {id}"),
    })
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

/// 供應一個 draw.io 的靜態檔。
///
/// 判斷（路徑安不安全、是什麼型別）在 [`drawio`]，可以用 `cargo test` 驗。
/// 這裡只做轉接：找到根目錄、讀檔、包成回應。
fn serve_drawio(
    app: &tauri::AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .expect("404 一定組得起來")
    };

    let Some(root) = drawio_root(app) else {
        return not_found();
    };
    let Some(path) = drawio::resolve(&root, request.uri().path()) else {
        return not_found();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return not_found();
    };

    tauri::http::Response::builder()
        .header("Content-Type", drawio::mime_of(&path))
        // 檔案在磁碟上不會變，而且每次開圖會抓上百個檔。
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(bytes)
        .unwrap_or_else(|_| not_found())
}

/// draw.io 那包在哪。
///
/// 打包之後在 app bundle 的資源區；開發時在 `source/vendor/drawio`。
/// **兩個都試**，因為兩種情況都要能跑，而分不出來的時候症狀是
/// 「編輯器一片空白」——那看不出是找不到檔案。
fn drawio_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;

    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|d| d.join("vendor/drawio"));
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vendor/drawio");

    bundled
        .into_iter()
        .chain(std::iter::once(dev))
        .find(|p| p.join("index.html").is_file())
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
        diagram_links,
        diagram_focus,
        blank_resource,
        create_project,
        new_window,
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
        // draw.io 的靜態檔。152 MB，不能放 frontendDist（會嵌進執行檔）。
        .register_uri_scheme_protocol("drawio", |ctx, request| {
            serve_drawio(ctx.app_handle(), request)
        })
        .invoke_handler(builder.invoke_handler())
        // 視窗關掉就把它持有的專案放掉。
        //
        // 這條維持了登記簿最重要的不變條件：**開著的專案 ⟺ 有視窗在看它**。
        // 漏了它，Agent 會去改一個沒有任何人看得到的專案——而「看得到它在改
        // 什麼」正是把 MCP 掛在 App 裡而不是做成獨立程序的全部理由。
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window.state::<Desk>().release(window.label());
            }
        })
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Desk::default());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 登記簿不需要真的視窗也不需要磁碟，所以它的規矩全部驗得到。
    fn opened(name: &str) -> Opened {
        Opened {
            history: History::opened(resource::new_project(name)),
            pending: None,
        }
    }

    #[test]
    fn a_window_finds_what_it_is_looking_at() {
        let desk = Desk::default();
        desk.bind("/w/a.loom".into(), "main".into(), opened("A"))
            .unwrap();

        assert!(desk.of_window("main").is_some());
        assert!(desk.of_window("w-1").is_none(), "別的視窗不該看得到它");
    }

    #[test]
    fn opening_another_project_replaces_what_this_window_held() {
        // 「開啟專案…」是取代這一扇窗。舊的留在登記簿裡的話，Agent 會
        // 看到一個沒有任何視窗在顯示的專案——那正是最該避免的狀態。
        let desk = Desk::default();
        desk.bind("/w/a.loom".into(), "main".into(), opened("A"))
            .unwrap();
        desk.bind("/w/b.loom".into(), "main".into(), opened("B"))
            .unwrap();

        assert_eq!(desk.all().len(), 1);
        assert!(desk.of_root(Path::new("/w/a.loom")).is_none());
        assert!(desk.of_root(Path::new("/w/b.loom")).is_some());
    }

    #[test]
    fn two_windows_hold_two_projects() {
        let desk = Desk::default();
        desk.bind("/w/a.loom".into(), "main".into(), opened("A"))
            .unwrap();
        desk.bind("/w/b.loom".into(), "w-1".into(), opened("B"))
            .unwrap();

        assert_eq!(desk.all().len(), 2);
        // 順序＝開啟順序。Agent 看到的清單要是穩定的。
        let roots: Vec<_> = desk.all().iter().map(|p| p.root.clone()).collect();
        assert_eq!(
            roots,
            [PathBuf::from("/w/a.loom"), PathBuf::from("/w/b.loom")]
        );
    }

    #[test]
    fn closing_a_window_lets_go_of_its_project() {
        // 這條維持了最重要的不變條件：**開著的專案 ⟺ 有視窗在看它**。
        // 漏了它，Agent 會去改一個沒有任何人看得到的專案。
        let desk = Desk::default();
        desk.bind("/w/a.loom".into(), "main".into(), opened("A"))
            .unwrap();
        desk.bind("/w/b.loom".into(), "w-1".into(), opened("B"))
            .unwrap();

        desk.release("w-1");

        assert_eq!(desk.all().len(), 1);
        assert!(desk.of_root(Path::new("/w/b.loom")).is_none());
        assert!(desk.of_window("main").is_some(), "不該波及別的視窗");
    }

    #[test]
    fn releasing_a_window_that_holds_nothing_is_fine() {
        // 沒開專案的空視窗被關掉時就是這樣。
        let desk = Desk::default();
        desk.release("w-9");
        assert!(desk.all().is_empty());
    }
}
