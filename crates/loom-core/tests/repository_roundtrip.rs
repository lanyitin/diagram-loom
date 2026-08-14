//! 專案資料夾讀寫的整合測試。
//!
//! 真的寫進磁碟再讀回來。序列化的錯誤（欄位漏標、enum 表示法對不上）
//! 只有實際往返一次才會現形。
//!
//! 另外也檢查產出的 YAML 好不好讀——純文字格式的整個意義就是能 code review，
//! 若 diff 看不懂，選純文字就白費了。

mod common;

use std::fs;
use std::path::PathBuf;

use common::*;
use loom_core::lint::lint;
use loom_core::repository::{load_from_dir, save_to_dir};

/// 每個測試用自己的暫存資料夾，避免互相干擾。
struct TempDir(PathBuf);

impl TempDir {
    fn create(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("loom-test-{name}"));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn save_then_load_returns_an_identical_project() {
    let dir = TempDir::create("roundtrip");
    let original = healthy_project();

    save_to_dir(&original, dir.path()).unwrap();
    let loaded = load_from_dir(dir.path()).unwrap();

    assert_eq!(loaded, original);
}

#[test]
fn a_reloaded_project_lints_identically() {
    // 這是真正要保護的性質：存檔不能悄悄改變任何影響 lint 的東西。
    let dir = TempDir::create("lint-stable");
    let original = healthy_project();

    save_to_dir(&original, dir.path()).unwrap();
    let loaded = load_from_dir(dir.path()).unwrap();

    assert_eq!(lint(&loaded), lint(&original));
    assert!(lint(&loaded).is_empty());
}

#[test]
fn produces_the_expected_file_layout() {
    let dir = TempDir::create("layout");
    save_to_dir(&healthy_project(), dir.path()).unwrap();

    for relative in [
        "project.yaml",
        "logical/systems.yaml",
        "logical/containers.yaml",
        "logical/relationships.yaml",
        "environments/prod.yaml",
        "environments/test.yaml",
        "environments/dev.yaml",
    ] {
        assert!(dir.path().join(relative).is_file(), "少了檔案 {relative}");
    }
}

#[test]
fn editing_one_environment_leaves_the_others_files_alone() {
    // 純文字格式的重點就在這裡：改 prod 時 dev 的 diff 應該是空的。
    let dir = TempDir::create("isolation");
    let mut project = healthy_project();
    save_to_dir(&project, dir.path()).unwrap();

    let dev_file = dir.path().join("environments/dev.yaml");
    let dev_before = fs::read_to_string(&dev_file).unwrap();

    project.environments[0].connections[0].purpose = "改過的用途".into();
    save_to_dir(&project, dir.path()).unwrap();

    assert_eq!(fs::read_to_string(&dev_file).unwrap(), dev_before);
}

#[test]
fn environment_files_are_named_by_slug_not_uuid() {
    // 檔名是給人看的。UUID 檔名的 git diff 完全讀不出改了哪個環境。
    let dir = TempDir::create("filename");
    save_to_dir(&healthy_project(), dir.path()).unwrap();

    let mut file_name: Vec<String> = fs::read_dir(dir.path().join("environments"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    file_name.sort();

    assert_eq!(file_name, vec!["dev.yaml", "prod.yaml", "test.yaml"]);
}

#[test]
fn yaml_is_readable_by_a_human() {
    let dir = TempDir::create("readable");
    save_to_dir(&healthy_project(), dir.path()).unwrap();

    let contents = fs::read_to_string(dir.path().join("logical/containers.yaml")).unwrap();

    // 服務名稱、endpoint 名稱、協定都應該以原樣出現，
    // 而不是被編碼成數字或 base64。
    assert!(
        contents.contains("order-api"),
        "找不到服務名稱：\n{contents}"
    );
    assert!(contents.contains("client-port"), "找不到 endpoint 名稱");
    assert!(contents.contains("tcp"), "協定應該是可讀的 kebab-case");
    assert!(contents.contains("Redis 快取"), "中文名稱應該原樣保留");
}

#[test]
fn both_ends_of_a_connection_are_readable_in_the_yaml() {
    let dir = TempDir::create("connections");
    save_to_dir(&healthy_project(), dir.path()).unwrap();

    let contents = fs::read_to_string(dir.path().join("environments/prod.yaml")).unwrap();

    // 經過 F5 的第二段：從設備連到一整群 Redis，期望 3 台。
    assert!(
        contents.contains("infra"),
        "看不出連線經過設備：\n{contents}"
    );
    assert!(contents.contains("redis-*"), "看不出萬用字元");
    assert!(contents.contains("expect"), "看不出期望數量");
}

#[test]
fn an_environment_slug_that_cannot_be_a_filename_is_rejected() {
    let dir = TempDir::create("bad-slug");
    let mut project = healthy_project();

    // 這種名稱若直接當檔名，會跳出專案資料夾。
    project.environments[0].slug = "../逃出去".into();

    let err = save_to_dir(&project, dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("不能安全地當檔名"),
        "錯誤訊息不夠清楚：{err}"
    );
}

#[test]
fn loading_a_missing_directory_reports_the_path() {
    let err = load_from_dir(&PathBuf::from("/tmp/loom-這個資料夾不存在")).unwrap_err();
    assert!(
        err.to_string().contains("project.yaml"),
        "錯誤訊息應該指出是哪個檔案：{err}"
    );
}

// ── 用記憶體儲存體，完全不碰磁碟 ─────────────────────────────

#[test]
fn the_memory_store_round_trips() {
    use loom_core::repository::{load, save};
    use loom_core::store::MemoryStore;

    let original = healthy_project();
    let mut store = MemoryStore::new();

    save(&original, &mut store).unwrap();
    assert_eq!(load(&store).unwrap(), original);
}

#[test]
fn the_whole_file_list_is_verifiable_without_touching_disk() {
    use loom_core::repository::save;
    use loom_core::store::MemoryStore;

    let mut store = MemoryStore::new();
    save(&healthy_project(), &mut store).unwrap();

    assert_eq!(
        store.paths(),
        vec![
            "environments/dev.yaml",
            "environments/prod.yaml",
            "environments/test.yaml",
            "logical/containers.yaml",
            "logical/relationships.yaml",
            "logical/systems.yaml",
            "project.yaml",
        ]
    );
}

#[test]
fn the_files_to_be_written_can_be_inspected_first() {
    // 記憶體儲存體也能拿來做「先算出結果、讓使用者確認再服務實體」。
    use loom_core::repository::save;
    use loom_core::store::MemoryStore;

    let mut store = MemoryStore::new();
    save(&healthy_project(), &mut store).unwrap();

    let prod = store.get("environments/prod.yaml").unwrap();
    assert!(prod.contains("f5-vip"), "看不到 F5：\n{prod}");
    assert!(prod.contains("redis-*"), "看不到萬用字元");
}

#[test]
fn an_unsafe_environment_slug_writes_nothing_to_the_store() {
    // 驗證失敗要在寫入之前發生，不能寫到一半才發現。
    use loom_core::repository::save;
    use loom_core::store::MemoryStore;

    let mut project = healthy_project();
    project.environments[0].slug = "../逃出去".into();

    let mut store = MemoryStore::new();
    assert!(save(&project, &mut store).is_err());
    assert!(
        store.is_empty(),
        "驗證失敗卻已經寫了東西：{:?}",
        store.paths()
    );
}

#[test]
fn the_real_architecture_fixture_round_trips() {
    // 照一個真實系統的架構建的（28 個服務實體、兩層巢狀站點、一台 VM 跑兩個服務、
    // 同站優先加跨站備援）。假素材通常太乾淨，這份用來確認 YAML 佈局
    // 撐得住現實的形狀。
    //
    // **現建一份，不讀 `fixtures/通路系統.loom`。** 磁碟上那份是開來玩的範例，
    // 使用者在 App 裡改它、存檔正是它的用途；拿它當測試素材的話，
    // 每次有人玩過 `mise run check` 就紅一次，然後只能把他的操作洗掉。
    let project = common::real::project();

    // 走一趟真正的存檔與讀檔，因為這裡要驗的就是 YAML 佈局。
    // 記憶體儲存體就夠了——不碰磁碟，也就不會動到那份範例專案。
    let mut store = loom_core::store::MemoryStore::new();
    loom_core::repository::save(&project, &mut store).expect("存得下去");
    let loaded = loom_core::repository::load(&store).expect("讀得回來");
    assert_eq!(loaded, project, "存檔再讀回來變了樣");

    assert_eq!(loaded.logical.containers.len(), 8);
    assert_eq!(loaded.logical.relationships.len(), 11);
    assert_eq!(loaded.environments.len(), 2);

    let instance_count: usize = loaded.environments[0]
        .nodes
        .iter()
        .map(|n| n.instances_recursive().len())
        .sum();
    assert_eq!(
        instance_count, 28,
        "prod 應該有 28 個服務實體（含兩層巢狀站點底下的）"
    );

    // 順便釘住 lint 的結果：哪天改了模型讓這份樣本的結論變了，
    // 這裡會提醒你那是不是預期中的。
    let found: Vec<String> = loom_core::lint::lint(&loaded)
        .iter()
        .map(|f| format!("{} {}", f.rule.code(), f.subject))
        .collect();
    assert_eq!(
        found,
        vec![
            "L001 c-redis".to_string(),
            "L001 r-channel-連-redis".to_string(),
            "L004 conn-test-07".to_string(),
        ],
        "prod 必須完全乾淨，只有 test 環境刻意留的三個洞"
    );
}

/// 會被省略的欄位，**每一個都要用非預設值來回一次**。
///
/// # 為什麼不能只靠既有的來回測試
///
/// YAML 省略空集合、`None` 與 `false`（見 `docs/decisions.md`），靠的是
/// `skip_serializing_if` 與 `default` 成對而且**條件剛好等於預設值**。
/// 寫反一個的話，一個非預設值會被寫成「不存在」，讀回來變成預設值——
/// 存檔靜靜地改掉了資料。
///
/// 但來回測試只證明得了**素材裡有的東西**。而 `healthy_project()` 裡
/// `standalone: true` 出現 0 次，於是 `skip_serializing_if = "is_false"`
/// 那一條從來沒有被真正走過。這裡把它補上。
#[test]
fn every_omitted_field_survives_a_round_trip() {
    let dir = TempDir::create("omitted-fields");
    let mut project = healthy_project();

    // 兩種實體的 standalone 都翻成 true——這是唯一沒被素材涵蓋的省略條件。
    let env = &mut project.environments[0];
    fn mark(nodes: &mut [loom_core::environment::DeploymentNode]) {
        for n in nodes {
            for i in &mut n.instances {
                i.standalone = true;
            }
            mark(&mut n.children);
        }
    }
    mark(&mut env.nodes);
    for s in &mut env.systems {
        s.standalone = true;
    }

    // 其餘的省略條件素材本來就踩得到，先確認一下，免得哪天素材被簡化了
    // 這條測試會安靜地失去意義。
    let env = &project.environments[0];
    assert!(!env.nodes.is_empty() && !env.infra.is_empty() && !env.systems.is_empty());
    assert!(env.instances().iter().any(|i| !i.endpoints.is_empty()));
    assert!(
        env.instances()
            .iter()
            .any(|i| i.endpoints.iter().any(|e| e.def.is_some()))
    );
    // 備援連線素材裡也沒有（`Fallback` 只出現在真實架構那份），所以這裡
    // 自己造一條。靠素材剛好有什麼，是這條測試原本失去意義的方式。
    let env = &mut project.environments[0];
    env.connections
        .first_mut()
        .expect("素材裡總該有一條連線")
        .kind = loom_core::environment::ConnectionKind::Fallback;

    save_to_dir(&project, dir.path()).unwrap();
    let loaded = load_from_dir(dir.path()).unwrap();

    assert_eq!(loaded, project);
}
