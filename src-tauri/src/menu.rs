//! 系統選單列。
//!
//! # 為什麼這些動作屬於選單，不是標頭上的按鈕
//!
//! 「新專案／開啟／新視窗／匯入／復原／重做」都是**每次只做一下、
//! 做完就離開**的動作。它們原本擠在標頭上，跟「我在哪個模式」這種持續的
//! 狀態混在一起，而使用者每天真正在看的是表格。
//!
//! 更實際的一點：在 macOS 上，**選單列就是快捷鍵的說明書**。
//! 這個 App 原本沒有「檔案」選單，於是 `⌘O`／`⌘N` 根本不存在，
//! 而使用者沒有任何地方查得到這件事。
//!
//! # ⌘S 原本在「圖」那個模式裡是死的
//!
//! 快捷鍵原本掛在主文件的 `window` 上，而 draw.io 跑在 `drawio://` 這個
//! **不同 origin 的 iframe** 裡。跨 origin 的 iframe 不會把 keydown 冒泡
//! 給父文件——所以焦點在畫布裡的時候按 `⌘S`，專案沒有存到，
//! 而畫面上沒有任何跡象。
//!
//! 原生選單的 accelerator 由系統派送，不管焦點在哪個 iframe 裡都會到。
//! **所以這不是美觀問題。**
//!
//! # ⚠️ 自己設選單就要把系統原本給的那些一起帶上
//!
//! 沒呼叫 `set_menu` 的時候 Tauri 會給一份預設選單。一旦自己設，
//! 那份就整個不見了——包括 `⌘Q` 離開、以及**編輯選單裡的剪下／複製／貼上**。
//! 少了後者，所有文字輸入框的 `⌘C`／`⌘V` 會安靜地失效：
//! 那些項目不只是選單，它們是 macOS 把編輯指令送進文字欄位的方式。
//!
//! 這件事沒有任何錯誤訊息，所以由 `the_edit_menu_still_has_copy_and_paste`
//! 守著。
//!
//! # 選單只負責「按下去了」，不做判斷
//!
//! 按下去就送一個 `loom://menu` 事件給**目前那扇視窗**，實際動作由前端做——
//! 開檔對話框、未儲存確認、匯入精靈本來就都在那裡。選單再實作一次的話，
//! 「開啟專案之前要問未儲存」就會有兩份，而且遲早只有一份被改到。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::Opened;

/// 會跟著目前這扇視窗改變的那幾個項目。
///
/// 存起來是因為要改它們的文字與啟用狀態——macOS 的選單是**整個 App 共用
/// 一份**，不是每扇視窗一份，所以「復原什麼」必須跟著焦點走。
pub struct Menus {
    undo: MenuItem<Wry>,
    redo: MenuItem<Wry>,
    save: MenuItem<Wry>,
    import: MenuItem<Wry>,
    recheck: MenuItem<Wry>,
}

/// 選單該長什麼樣，全部由這幾個值決定。
pub struct MenuState {
    pub open: bool,
    pub dirty: bool,
    pub undo: Option<String>,
    pub redo: Option<String>,
}

impl MenuState {
    pub fn of(opened: &Opened) -> Self {
        Self::of_history(&opened.history)
    }

    /// 開專案與建專案手上只有 `History`——那時候登記簿裡還沒有這一份。
    pub fn of_history(history: &loom_core::history::History) -> Self {
        Self {
            open: true,
            dirty: history.is_dirty(),
            undo: history.undo_label().map(str::to_string),
            redo: history.redo_label().map(str::to_string),
        }
    }

    /// 這扇視窗還沒開專案。
    pub fn closed() -> Self {
        Self {
            open: false,
            dirty: false,
            undo: None,
            redo: None,
        }
    }
}

/// 選單項目的 id。**前端要照著這些字串分派**，所以它們是一份契約。
///
/// 拼錯的話按下去什麼都不會發生，而且沒有錯誤——所以有一條測試比對
/// 這份清單跟 `App.vue` 裡處理得了的那幾個。
pub const ACTIONS: &[&str] = &[
    "new-project",
    "open-project",
    "new-window",
    "import",
    "save",
    "undo",
    "redo",
    "recheck",
    "agent",
];

/// 造出整份選單。
///
/// ⚠️ **要交給 `tauri::Builder::menu`，不能在 `setup` 裡呼叫 `app.set_menu`。**
/// 後者在 macOS 上**回 `Ok` 但什麼也沒發生**——實測過：選單列上還是 Tauri
/// 那份預設的（File 底下只有 Close Window）。沒有錯誤、沒有警告，
/// 只有真的把 App 開起來看選單列才會發現。
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    // macOS 的第一個子選單就是 App 選單。`⌘Q` 在這裡，漏了就離不開。
    let about = Submenu::with_items(
        app,
        "diagram-loom",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;

    let new_project = MenuItem::with_id(
        app,
        "new-project",
        "新專案…",
        true,
        Some("CmdOrCtrl+Shift+N"),
    )?;
    let open_project =
        MenuItem::with_id(app, "open-project", "開啟專案…", true, Some("CmdOrCtrl+O"))?;
    // 「開啟專案…」取代這扇視窗，「新視窗」不動任何現有的東西。
    // 兩個入口分開，才不會有人以為自己弄丟了工作。
    let new_window = MenuItem::with_id(app, "new-window", "新視窗", true, Some("CmdOrCtrl+N"))?;
    let import = MenuItem::with_id(app, "import", "匯入試算表…", false, Some("CmdOrCtrl+I"))?;
    let save = MenuItem::with_id(app, "save", "儲存", false, Some("CmdOrCtrl+S"))?;

    let file = Submenu::with_items(
        app,
        "檔案",
        true,
        &[
            &new_project,
            &open_project,
            &new_window,
            &PredefinedMenuItem::separator(app)?,
            &import,
            &PredefinedMenuItem::separator(app)?,
            &save,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    // 沒有可以復原的動作時，文字是「復原」而不是「復原 某件事」。
    let undo = MenuItem::with_id(app, "undo", "復原", false, Some("CmdOrCtrl+Z"))?;
    let redo = MenuItem::with_id(app, "redo", "重做", false, Some("CmdOrCtrl+Shift+Z"))?;

    // ⚠️ 底下那四個 predefined 不是裝飾。macOS 靠它們把編輯指令送進
    //    文字欄位——拿掉的話所有輸入框的 ⌘C／⌘V 會安靜地失效。
    let edit = Submenu::with_items(
        app,
        "編輯",
        true,
        &[
            &undo,
            &redo,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let recheck = MenuItem::with_id(app, "recheck", "重新檢查", false, Some("CmdOrCtrl+R"))?;
    let agent = MenuItem::with_id(app, "agent", "AI 助手…", true, None::<&str>)?;
    let view = Submenu::with_items(app, "檢視", true, &[&recheck, &agent])?;

    let window = Submenu::with_items(
        app,
        "視窗",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    app.manage(Menus {
        undo,
        redo,
        save,
        import,
        recheck,
    });
    Menu::with_items(app, &[&about, &file, &edit, &view, &window])
}

/// 讓選單反映目前這扇視窗的狀態。
///
/// 失敗就算了：一個灰不掉的選單項目按下去頂多什麼也沒發生，
/// 不值得讓使用者的那次操作整個失敗。
pub fn apply(app: &AppHandle, state: &MenuState) {
    let Some(menus) = app.try_state::<Menus>() else {
        return;
    };

    let _ = menus
        .undo
        .set_text(label("復原", state.undo.as_deref()))
        .and_then(|()| menus.undo.set_enabled(state.undo.is_some()));
    let _ = menus
        .redo
        .set_text(label("重做", state.redo.as_deref()))
        .and_then(|()| menus.redo.set_enabled(state.redo.is_some()));

    let _ = menus.save.set_enabled(state.dirty);
    let _ = menus.import.set_enabled(state.open);
    let _ = menus.recheck.set_enabled(state.open);
}

/// 「復原 新增服務」，沒東西可復原時就只是「復原」。
///
/// # 為什麼獨立成一個函式
///
/// 這是拿掉標頭那兩顆按鈕之後，「剛剛做了什麼」**唯一的**去處。而它住在
/// 原生選單上，`cargo test` 看不到、Vue 的測試也看不到——只有真的把 App
/// 開起來拉開選單才看得見。
///
/// 抽出來至少讓「該長什麼字」這件事驗得到。剩下「有沒有真的貼上去」
/// 就只能靠眼睛。
fn label(action: &str, what: Option<&str>) -> String {
    match what {
        Some(what) => format!("{action} {what}"),
        None => action.to_string(),
    }
}

/// 按下去了。**只送給目前那扇視窗**——選單是整個 App 共用的，
/// 廣播的話「開啟專案」會在每一扇視窗都跳出一個檔案對話框。
pub fn dispatch(app: &AppHandle, id: &str) {
    let Some(window) = focused(app) else {
        return;
    };
    let _ = window.emit("loom://menu", id);
}

/// 目前有焦點的那扇視窗。只有一扇時就是它——剛啟動還沒拿到焦點的
/// 那一瞬間也要能用，不然第一次按選單會什麼都不發生。
fn focused(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    let windows = app.webview_windows();
    windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| windows.values().next())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_undo_item_says_what_it_will_undo() {
        // macOS 的慣例，也是拿掉標頭那兩顆按鈕之後「剛剛做了什麼」的唯一去處。
        assert_eq!(label("復原", Some("新增服務")), "復原 新增服務");
        assert_eq!(label("重做", Some("刪除連線")), "重做 刪除連線");
    }

    #[test]
    fn with_nothing_to_undo_it_is_just_the_verb() {
        // 「復原 」後面掛一個空白看起來像壞掉了。
        assert_eq!(label("復原", None), "復原");
        assert_eq!(label("重做", None), "重做");
    }

    #[test]
    fn a_window_with_no_project_can_do_almost_nothing() {
        let s = MenuState::closed();
        assert!(!s.open, "沒開專案就不該讓人按匯入");
        assert!(!s.dirty, "沒開專案就沒有東西可以存");
        assert!(s.undo.is_none() && s.redo.is_none());
    }

    #[test]
    fn the_edit_menu_still_has_copy_and_paste() {
        // 自己設選單就會蓋掉 Tauri 的預設那份。少了這幾個 predefined，
        // **所有文字輸入框的 ⌘C／⌘V 會安靜地失效**——它們不只是選單項目，
        // 是 macOS 把編輯指令送進文字欄位的方式。沒有任何錯誤訊息。
        let source = include_str!("menu.rs");
        let edit = source
            .split("\"編輯\",")
            .nth(1)
            .expect("找不到編輯選單")
            .split("]")
            .next()
            .unwrap();

        for needed in ["cut(", "copy(", "paste(", "select_all("] {
            assert!(edit.contains(needed), "編輯選單少了 {needed}");
        }
    }

    #[test]
    fn the_menu_is_installed_by_the_builder_not_by_set_menu() {
        // `AppHandle::set_menu` 在 macOS 上回 Ok 但什麼也沒發生。實測過：
        // 選單列上還是 Tauri 的預設那份，而且沒有任何錯誤訊息。
        // 只有真的開起來看選單列才會發現——所以釘住它。
        // 只看真的程式碼——註解裡本來就會提到那個名字（就在上面幾行）。
        let code = include_str!("menu.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<String>();
        // 針拆開拼，不然這一行自己就是它要找的東西。
        let needle = format!("app.set{}", "_menu(");
        assert!(
            !code.contains(&needle),
            "set_menu 在 macOS 上不會生效，選單要交給 tauri::Builder::menu"
        );
        assert!(include_str!("lib.rs").contains(".menu(menu::build)"));
    }

    #[test]
    fn quitting_is_still_possible() {
        // 同一件事的另一半：App 選單漏了 quit 就離不開這個 App。
        let source = include_str!("menu.rs");
        assert!(source.contains("PredefinedMenuItem::quit("));
    }

    #[test]
    fn every_action_is_actually_on_the_menu() {
        // `ACTIONS` 是給前端對照的契約。列了卻沒建出來的話，前端會
        // 為一個永遠不會來的事件寫一段處理——而測試看起來是綠的。
        let source = include_str!("menu.rs");
        for id in ACTIONS {
            // 出現兩次：一次在 `ACTIONS` 裡，一次在真的建出來的地方。
            // 不比對 `with_id(app, "…"` 是因為 cargo fmt 會把長的那幾行拆開。
            let times = source.matches(&format!("\"{id}\"")).count();
            assert!(
                times >= 2,
                "ACTIONS 列了 {id}，但選單上沒有這一項（只出現 {times} 次）"
            );
        }
    }
}
