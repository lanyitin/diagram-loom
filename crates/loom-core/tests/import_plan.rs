//! 匯入預覽的整合測試。
//!
//! 預覽這一步的存在理由是：匯入是唯一會大量改動資料的操作，貼錯一格
//! 就洗掉一個環境。所以這裡最在意的是**預覽有沒有說謊**——
//! 它列出來的東西，必須就是套用之後真正發生的事。

mod common;

use common::*;
use loom_core::importer::{Sheet, row_from_pairs, template_headers};
use loom_core::plan::{ChangeKind, Element, plan};

fn 表(rows: Vec<Vec<String>>) -> Sheet {
    Sheet::new(template_headers(), rows)
}

fn 一列(env: &str, from: &str, to: &str, address: &str) -> Vec<String> {
    row_from_pairs(&[
        ("environment", env),
        ("purpose", "壓力測試"),
        ("serves", "api-連-redis"),
        ("from_node", from),
        ("from_service", "order-api"),
        ("to_node", to),
        ("to_service", "redis"),
        ("to_endpoint", "client-port"),
        ("to_address", address),
        ("protocol", "TCP"),
    ])
}

#[test]
fn 匯進全新的環境時每一項都是新增() {
    let project = healthy_project();
    let (p, _) = plan(
        &project,
        &表(vec![一列("stage", "vm-a", "vm-b", "10.9.0.1:6379")]),
    )
    .unwrap();

    assert!(p.changes.iter().all(|c| c.kind == ChangeKind::Added));
    assert_eq!(p.environments, vec!["stage"]);

    let 種類: Vec<Element> = {
        let mut k: Vec<_> = p.changes.iter().map(|c| c.element).collect();
        k.sort();
        k.dedup();
        k
    };
    assert!(種類.contains(&Element::Environment));
    assert!(種類.contains(&Element::DeploymentNode));
    assert!(種類.contains(&Element::ContainerInstance));
    assert!(種類.contains(&Element::Connection));
}

#[test]
fn 預覽不會動到原本的專案() {
    // 預覽如果有副作用，使用者按「取消」之後資料就已經被改了。
    let project = healthy_project();
    let 原本 = project.clone();

    let _ = plan(
        &project,
        &表(vec![一列("stage", "vm-a", "vm-b", "10.9.0.1:6379")]),
    )
    .unwrap();

    assert_eq!(project, 原本);
}

#[test]
fn 預覽說的就是套用之後的樣子() {
    // 這是整份測試最重要的一條。預覽與實際若不一致，這個步驟就只是裝飾。
    let project = healthy_project();
    let sheet = 表(vec![
        一列("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        一列("prod", "vm-api-01", "vm-new", "10.0.1.99:6379"),
    ]);

    let (p, 套用後) = plan(&project, &sheet).unwrap();

    // 拿套用後的結果再預覽一次同一張表——應該完全沒有變更可做。
    let (再一次, _) = plan(&套用後, &sheet).unwrap();
    assert!(
        再一次.changes.is_empty(),
        "預覽宣稱會做這些事：{:?}\n但套用後再跑一次還有變更：{:?}",
        p.changes.len(),
        再一次.changes
    );
    assert!(再一次.unchanged > 0);
}

#[test]
fn 重複匯入同一份檔案不會產生重複的連線() {
    // 重新匯入更新過的 Excel 是最常見的用法。連線若不 upsert，
    // 每匯一次就整份複製一遍，而且 lint 不會抱怨——使用者根本不會發現。
    let project = healthy_project();
    let sheet = 表(vec![一列("stage", "vm-a", "vm-b", "10.9.0.1:6379")]);

    let (_, 第一次) = plan(&project, &sheet).unwrap();
    let (_, 第二次) = plan(&第一次, &sheet).unwrap();

    let 連線數 = |p: &loom_core::Project| {
        p.environments
            .iter()
            .find(|e| e.slug == "stage")
            .unwrap()
            .connections
            .len()
    };
    assert_eq!(連線數(&第一次), 1);
    assert_eq!(連線數(&第二次), 1, "第二次匯入把連線複製了一份");
}

#[test]
fn 改了位址重新匯入不會覆蓋只會警告() {
    // ⚠️ 這條測的是現行規則（docs/excel-import.md）：位址第一次出現時建立，
    // 之後不一致就警告、不覆蓋。
    //
    // 那條規則當初是為了「同一份檔案裡五列重複寫同一台 Redis 的位址」。
    // 套用在「改了 IP 重新匯入」上的後果是：**試算表改不動已經存在的位址**。
    // 對一個預期用 Excel 維護數百條連線的人來說，這很可能不是他要的。
    //
    // 先照文件走，但把它釘成測試——哪天決定要改，這裡會提醒你那是刻意的取捨。
    let project = healthy_project();
    let sheet = 表(vec![一列("stage", "vm-a", "vm-b", "10.9.0.1:6379")]);
    let (_, 之前) = plan(&project, &sheet).unwrap();

    let 改過 = 表(vec![一列("stage", "vm-a", "vm-b", "10.9.0.99:6379")]);
    let (p, _) = plan(&之前, &改過).unwrap();

    let 位址變更: Vec<_> = p
        .changes
        .iter()
        .filter(|c| c.element == Element::Address && c.kind == ChangeKind::Updated)
        .collect();
    assert!(位址變更.is_empty(), "現行規則不覆蓋位址：{位址變更:?}");

    assert_eq!(p.warnings.len(), 1, "但一定要講出來：{:?}", p.warnings);
    assert!(
        p.warnings[0].contains("10.9.0.1:6379"),
        "要說清楚保留了哪個"
    );
    assert!(p.warnings[0].contains("10.9.0.99:6379"), "以及忽略了哪個");
}

#[test]
fn 位址衝突會出現在警告裡而不是靜靜吞掉() {
    // 同一個 Endpoint 在兩列填了不同位址。工具保留先出現的，
    // 但一定要講出來——不然使用者會以為兩個都寫進去了。
    let project = healthy_project();
    let sheet = 表(vec![
        一列("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        一列("stage", "vm-a", "vm-b", "10.9.0.2:6379"),
    ]);

    let (p, _) = plan(&project, &sheet).unwrap();
    assert_eq!(p.warnings.len(), 1, "警告：{:?}", p.warnings);
    assert!(p.warnings[0].contains("10.9.0.2:6379"));
}

#[test]
fn 表頭有問題時整份拒絕不做半套() {
    let project = healthy_project();
    let 壞表 = Sheet::new(vec!["environment".into()], vec![vec!["prod".into()]]);
    assert!(plan(&project, &壞表).is_err());
}

#[test]
fn 一張表動到多個環境時全部列出來() {
    let project = healthy_project();
    let sheet = 表(vec![
        一列("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
        一列("uat", "vm-c", "vm-d", "10.8.0.1:6379"),
        一列("stage", "vm-a", "vm-e", "10.9.0.2:6379"),
    ]);

    let (p, _) = plan(&project, &sheet).unwrap();
    assert_eq!(p.environments, vec!["stage", "uat"], "重複的環境只列一次");
}

#[test]
fn 變更依種類分組排好() {
    // 前端直接照順序畫，同一類的變更會排在一起。
    let project = healthy_project();
    let (p, _) = plan(
        &project,
        &表(vec![
            一列("stage", "vm-a", "vm-b", "10.9.0.1:6379"),
            一列("stage", "vm-c", "vm-d", "10.9.0.2:6379"),
        ]),
    )
    .unwrap();

    let 種類: Vec<Element> = p.changes.iter().map(|c| c.element).collect();
    let mut 排好的 = 種類.clone();
    排好的.sort();
    assert_eq!(種類, 排好的);
}
