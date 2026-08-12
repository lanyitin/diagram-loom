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
use loom_core::repository::{load, save};

/// 每個測試用自己的暫存資料夾，避免互相干擾。
struct 暫存資料夾(PathBuf);

impl 暫存資料夾 {
    fn 建立(名稱: &str) -> Self {
        let path = std::env::temp_dir().join(format!("loom-test-{名稱}"));
        let _ = fs::remove_dir_all(&path);
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for 暫存資料夾 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn 存檔再讀回來得到一模一樣的專案() {
    let dir = 暫存資料夾::建立("roundtrip");
    let original = healthy_project();

    save(&original, dir.path()).unwrap();
    let loaded = load(dir.path()).unwrap();

    assert_eq!(loaded, original);
}

#[test]
fn 讀回來的專案跑lint結果相同() {
    // 這是真正要保護的性質：存檔不能悄悄改變任何影響 lint 的東西。
    let dir = 暫存資料夾::建立("lint-stable");
    let original = healthy_project();

    save(&original, dir.path()).unwrap();
    let loaded = load(dir.path()).unwrap();

    assert_eq!(lint(&loaded), lint(&original));
    assert!(lint(&loaded).is_empty());
}

#[test]
fn 產生預期的檔案結構() {
    let dir = 暫存資料夾::建立("layout");
    save(&healthy_project(), dir.path()).unwrap();

    for 相對路徑 in [
        "project.yaml",
        "logical/systems.yaml",
        "logical/containers.yaml",
        "logical/relationships.yaml",
        "environments/prod.yaml",
        "environments/test.yaml",
        "environments/dev.yaml",
    ] {
        assert!(dir.path().join(相對路徑).is_file(), "少了檔案 {相對路徑}");
    }
}

#[test]
fn 改一個環境不會動到其他環境的檔案() {
    // 純文字格式的重點就在這裡：改 prod 時 dev 的 diff 應該是空的。
    let dir = 暫存資料夾::建立("isolation");
    let mut project = healthy_project();
    save(&project, dir.path()).unwrap();

    let dev檔 = dir.path().join("environments/dev.yaml");
    let dev原內容 = fs::read_to_string(&dev檔).unwrap();

    project.environments[0].connections[0].purpose = "改過的用途".into();
    save(&project, dir.path()).unwrap();

    assert_eq!(fs::read_to_string(&dev檔).unwrap(), dev原內容);
}

#[test]
fn 環境檔用slug當檔名而不是uuid() {
    // 檔名是給人看的。UUID 檔名的 git diff 完全讀不出改了哪個環境。
    let dir = 暫存資料夾::建立("filename");
    save(&healthy_project(), dir.path()).unwrap();

    let mut 檔名: Vec<String> = fs::read_dir(dir.path().join("environments"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    檔名.sort();

    assert_eq!(檔名, vec!["dev.yaml", "prod.yaml", "test.yaml"]);
}

#[test]
fn yaml內容是人看得懂的() {
    let dir = 暫存資料夾::建立("readable");
    save(&healthy_project(), dir.path()).unwrap();

    let 內容 = fs::read_to_string(dir.path().join("logical/containers.yaml")).unwrap();

    // 服務名稱、endpoint 名稱、協定都應該以原樣出現，
    // 而不是被編碼成數字或 base64。
    assert!(內容.contains("order-api"), "找不到服務名稱：\n{內容}");
    assert!(內容.contains("client-port"), "找不到 endpoint 名稱");
    assert!(內容.contains("tcp"), "協定應該是可讀的 kebab-case");
    assert!(內容.contains("Redis 快取"), "中文名稱應該原樣保留");
}

#[test]
fn 連線的兩端在yaml上讀得出來() {
    let dir = 暫存資料夾::建立("connections");
    save(&healthy_project(), dir.path()).unwrap();

    let 內容 = fs::read_to_string(dir.path().join("environments/prod.yaml")).unwrap();

    // 經過 F5 的第二段：從設備連到一整群 Redis，期望 3 台。
    assert!(內容.contains("infra"), "看不出連線經過設備：\n{內容}");
    assert!(內容.contains("redis-*"), "看不出萬用字元");
    assert!(內容.contains("expect"), "看不出期望數量");
}

#[test]
fn 環境名稱不能當檔名時會被擋下() {
    let dir = 暫存資料夾::建立("bad-slug");
    let mut project = healthy_project();

    // 這種名稱若直接當檔名，會跳出專案資料夾。
    project.environments[0].slug = "../逃出去".into();

    let err = save(&project, dir.path()).unwrap_err();
    assert!(
        err.to_string().contains("不能安全地當檔名"),
        "錯誤訊息不夠清楚：{err}"
    );
}

#[test]
fn 讀不存在的資料夾會給出含路徑的錯誤() {
    let err = load(&PathBuf::from("/tmp/loom-這個資料夾不存在")).unwrap_err();
    assert!(
        err.to_string().contains("project.yaml"),
        "錯誤訊息應該指出是哪個檔案：{err}"
    );
}
