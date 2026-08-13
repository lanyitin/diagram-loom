//! 試算表匯入的整合測試。
//!
//! 除了各種欄位情境，最後一個測試會讀 `fixtures/excel/` 底下那份給使用者填的
//! 樣板檔——樣板若跟程式對不上，使用者第一次用就會撞牆。

use std::path::PathBuf;

use loom_core::Project;
use loom_core::environment::{ConnectionKind, Endpointing, InstanceRef, NodeKind};
use loom_core::id::Id;
use loom_core::importer::{
    ImportError, ImportWarning, Sheet, import, read_csv, row_from_pairs, template_headers,
};
use loom_core::lint::{Rule, lint};
use loom_core::logical::Logical;

fn empty_project() -> Project {
    Project {
        id: Id::generate(),
        slug: "shop".into(),
        name: "網路商店".into(),
        logical: Logical::default(),
        environments: vec![],
    }
}

fn sheet(rows: Vec<Vec<String>>) -> Sheet {
    Sheet::new(template_headers(), rows)
}

fn row(pairs: &[(&str, &str)]) -> Vec<String> {
    row_from_pairs(pairs)
}

/// 改某一欄的值。**依欄位名稱找位置**——寫死索引的話，
/// 樣板多一欄就整批測試錯位，而且錯得很難看懂。
fn set_cell(row: &mut [String], column: &str, value: &str) {
    let i = template_headers()
        .iter()
        .position(|h| h == column)
        .unwrap_or_else(|| panic!("樣板沒有 {column} 這一欄"));
    row[i] = value.into();
}

/// 最單純的一列：dev 環境，API 直連 Redis。
fn direct_row() -> Vec<String> {
    row(&[
        ("environment", "dev"),
        ("purpose", "訂單服務讀寫快取"),
        ("from_node", "app-vm-01"),
        ("from_service", "order-api"),
        ("to_node", "redis-vm-01"),
        ("to_service", "redis"),
        ("to_endpoint", "client-port"),
        ("to_address", "10.0.1.11:6379"),
        ("protocol", "TCP"),
    ])
}

#[test]
fn one_row_creates_environment_node_container_and_connection() {
    let mut project = empty_project();
    let report = import(&mut project, &sheet(vec![direct_row()])).unwrap();

    assert_eq!(report.connections, 1);
    assert_eq!(report.environments_created, 1);
    assert_eq!(report.containers_created, 2);
    assert_eq!(report.instances_created, 2);

    let dev = &project.environments[0];
    assert_eq!(dev.slug, "dev");
    assert_eq!(dev.instances().len(), 2);
}

#[test]
fn instance_slug_combines_node_and_container() {
    // 一台機器可能跑多個服務，光用機器名會撞在一起。
    let mut project = empty_project();
    import(&mut project, &sheet(vec![direct_row()])).unwrap();

    let mut slugs: Vec<String> = project.environments[0]
        .instances()
        .iter()
        .map(|i| i.slug.clone())
        .collect();
    slugs.sort();
    assert_eq!(slugs, vec!["app-vm-01-order-api", "redis-vm-01-redis"]);
}

#[test]
fn two_containers_on_one_node_do_not_collide() {
    let mut project = empty_project();
    let second_cell = row(&[
        ("environment", "dev"),
        ("purpose", "查詢設定"),
        ("from_node", "app-vm-01"),
        ("from_service", "order-api"),
        ("to_node", "app-vm-01"),
        ("to_service", "consul"),
        ("to_endpoint", "http-api"),
        ("to_address", "127.0.0.1:8500"),
        ("protocol", "TCP"),
    ]);

    import(&mut project, &sheet(vec![direct_row(), second_cell])).unwrap();

    let node = project.environments[0]
        .nodes
        .iter()
        .find(|n| n.slug == "app-vm-01")
        .unwrap();
    let mut slugs: Vec<String> = node.instances.iter().map(|i| i.slug.clone()).collect();
    slugs.sort();
    assert_eq!(slugs, vec!["app-vm-01-consul", "app-vm-01-order-api"]);
}

#[test]
fn reimporting_creates_no_duplicates() {
    // 使用者更新試算表後重跑，既有資料應該原地更新而不是複製一份。
    let mut project = empty_project();
    import(&mut project, &sheet(vec![direct_row()])).unwrap();
    let first = project.environments[0].instances().len();

    import(&mut project, &sheet(vec![direct_row()])).unwrap();

    assert_eq!(project.environments[0].instances().len(), first);
    assert_eq!(project.logical.containers.len(), 2);
    assert_eq!(project.environments.len(), 1);
}

#[test]
fn the_imported_project_lints_straight_away() {
    let mut project = empty_project();
    import(&mut project, &sheet(vec![direct_row()])).unwrap();

    let findings = lint(&project);
    // 來源端沒填 endpoint（OS 分配）是正常的；不該有任何 Error。
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity() == loom_core::lint::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "匯入的結果有 Error：{errors:?}");
}

#[test]
fn a_hop_through_infra_splits_into_two_segments_sharing_one_contract() {
    let mut project = empty_project();
    let first_hop = row(&[
        ("environment", "prod"),
        ("purpose", "第一段：到 F5"),
        ("serves", "api-連-redis"),
        ("from_node", "app-vm-01"),
        ("from_service", "order-api"),
        ("to_node", "f5-01"),
        ("to_endpoint", "vip-redis"),
        ("to_address", "10.0.0.100:6379"),
        ("protocol", "TCP"),
    ]);
    let second_hop = row(&[
        ("environment", "prod"),
        ("purpose", "第二段：F5 到後端"),
        ("serves", "api-連-redis"),
        ("from_node", "f5-01"),
        ("to_node", "redis-vm-01"),
        ("to_service", "redis"),
        ("to_endpoint", "client-port"),
        ("to_address", "10.0.1.11:6379"),
        ("protocol", "TCP"),
    ]);

    import(&mut project, &sheet(vec![first_hop, second_hop])).unwrap();

    let prod = &project.environments[0];
    assert_eq!(prod.connections.len(), 2);
    assert_eq!(prod.infra.len(), 1, "應該建出一個設備");
    assert_eq!(
        prod.connections[0].serves, prod.connections[1].serves,
        "兩段應該服務同一條邏輯連線"
    );
    assert_eq!(project.logical.relationships.len(), 1);
}

#[test]
fn an_infra_target_without_serves_is_rejected() {
    let mut project = empty_project();
    let row = row(&[
        ("environment", "prod"),
        ("purpose", "到 F5"),
        ("from_node", "app-vm-01"),
        ("from_service", "order-api"),
        ("to_node", "f5-01"),
        ("to_endpoint", "vip-redis"),
        ("to_address", "10.0.0.100:6379"),
        ("protocol", "TCP"),
    ]);

    assert_eq!(
        import(&mut project, &sheet(vec![row])),
        Err(ImportError::InfraRowNeedsServes { row: 2 })
    );
}

#[test]
fn a_wildcard_row_creates_no_new_nodes() {
    // 萬用字元是在指涉既有的一群機器，不是在定義新機器。
    let mut project = empty_project();
    let rows = vec![
        direct_row(),
        // 萬用字元的列不必填 to_address：位址由各機器自己的定義提供。
        row(&[
            ("environment", "dev"),
            ("purpose", "叢集讀寫"),
            ("serves", "api-連-叢集"),
            ("from_node", "app-vm-01"),
            ("from_service", "order-api"),
            ("to_node", "redis-vm-*"),
            ("to_service", "redis"),
            ("to_endpoint", "client-port"),
            ("protocol", "TCP"),
            ("expect", "1"),
        ]),
    ];

    import(&mut project, &sheet(rows)).unwrap();

    // 仍然只有直連那列建出的兩台
    assert_eq!(project.environments[0].instances().len(), 2);

    let wildcard_row = &project.environments[0].connections[1];
    let Endpointing::Instance { target, .. } = &wildcard_row.to else {
        panic!("目標應該是一群 Instance");
    };
    assert_eq!(
        target,
        &InstanceRef::Pattern {
            slug_pattern: "redis-vm-*-redis".into(),
            within: None,
            expect: Some(1),
        }
    );
}

#[test]
fn conflicting_addresses_keep_the_first_and_warn() {
    let mut project = empty_project();
    let mut typo = direct_row();
    set_cell(&mut typo, "to_address", "10.0.1.99:6379");

    let report = import(&mut project, &sheet(vec![direct_row(), typo])).unwrap();

    assert_eq!(
        report.warnings,
        vec![ImportWarning::AddressConflict {
            row: 3,
            endpoint: "client-port".into(),
            kept: "10.0.1.11:6379".into(),
            ignored: "10.0.1.99:6379".into(),
        }]
    );
}

#[test]
fn missing_required_columns_are_all_listed_at_once() {
    let mut project = empty_project();
    let sheet = Sheet::new(
        vec!["environment".into(), "purpose".into()],
        vec![vec!["dev".into(), "隨便".into()]],
    );

    let err = import(&mut project, &sheet).unwrap_err();
    let ImportError::MissingColumns(missing) = err else {
        panic!("應該回報缺欄位，實際是 {err:?}");
    };
    assert_eq!(
        missing,
        vec![
            "from_node",
            "to_node",
            "to_endpoint",
            "to_address",
            "protocol"
        ]
    );
}

#[test]
fn an_unknown_protocol_lists_the_valid_options() {
    let mut project = empty_project();
    let mut row = direct_row();
    set_cell(&mut row, "protocol", "SMTP");

    let err = import(&mut project, &sheet(vec![row])).unwrap_err();
    assert!(err.to_string().contains("TCP"), "沒列出可用選項：{err}");
}

#[test]
fn an_unnormalised_name_comes_with_a_suggestion() {
    let mut project = empty_project();
    let mut row = direct_row();
    set_cell(&mut row, "from_node", "App VM 01");

    let err = import(&mut project, &sheet(vec![row])).unwrap_err();
    assert!(
        err.to_string().contains("app-vm-01"),
        "沒給出建議寫法：{err}"
    );
}

#[test]
fn an_unknown_column_warns_without_stopping() {
    let mut project = empty_project();
    let mut headers = template_headers();
    headers.push("負責人".into());
    let mut row = direct_row();
    row.push("小明".into());

    let report = import(&mut project, &Sheet::new(headers, vec![row])).unwrap();

    assert_eq!(report.connections, 1);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::UnknownColumn("負責人".into())]
    );
}

#[test]
fn the_shipped_template_parses() {
    // 樣板若跟程式對不上，使用者第一次用就會撞牆。
    let template = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");

    let sheet = read_csv(&template).expect("讀不到樣板檔");
    assert!(sheet.row_count() >= 8, "樣板應該有多列範例");

    let mut project = empty_project();
    let report = import(&mut project, &sheet).expect("樣板無法匯入");

    assert_eq!(report.connections, sheet.row_count());
    assert_eq!(project.environments.len(), 2, "樣板涵蓋 prod 與 test");
}

#[test]
fn the_templates_prod_environment_imports_clean() {
    let template = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");
    let sheet = read_csv(&template).unwrap();

    let mut project = empty_project();
    import(&mut project, &sheet).unwrap();

    let prod = project
        .environments
        .iter()
        .find(|e| e.slug == "prod")
        .expect("樣板應該建出 prod");

    let errors: Vec<String> = lint(&project)
        .iter()
        .filter(|f| f.environment.as_ref() == Some(&prod.id))
        .filter(|f| f.severity() == loom_core::lint::Severity::Error)
        .map(|f| format!("{} {}", f.rule.code(), f.detail))
        .collect();

    assert!(errors.is_empty(), "prod 匯入後有嚴重錯誤：{errors:#?}");
}

#[test]
fn the_templates_test_environment_has_one_row_so_lint_reports_gaps() {
    // 樣板的 test 環境只放一列示範萬用字元寫法，其餘服務都沒部署。
    // lint 抓到這件事正是它該做的——這個測試同時驗證匯入與 lint 接得起來。
    let template = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");
    let sheet = read_csv(&template).unwrap();

    let mut project = empty_project();
    import(&mut project, &sheet).unwrap();

    let test_env = project
        .environments
        .iter()
        .find(|e| e.slug == "test")
        .expect("樣板應該建出 test");

    let gap: Vec<&str> = lint(&project)
        .iter()
        .filter(|f| f.environment.as_ref() == Some(&test_env.id))
        .filter(|f| f.rule == Rule::L001)
        .map(|_| "缺")
        .collect();

    assert!(
        !gap.is_empty(),
        "test 環境明顯不完整，lint 卻沒指出任何缺漏"
    );
}

// ── 站點與備援（限制 D）────────────────────────────────

fn row_with_site(site: &str, node: &str, service: &str, address: &str) -> Vec<String> {
    row_from_pairs(&[
        ("environment", "prod"),
        ("purpose", "測試"),
        ("serves", "app-連-redis"),
        ("from_site", site),
        ("from_node", "app-vm-01"),
        ("from_service", "app"),
        ("to_site", site),
        ("to_node", node),
        ("to_service", service),
        ("to_endpoint", "client-port"),
        ("to_address", address),
        ("protocol", "TCP"),
    ])
}

#[test]
fn a_node_with_a_site_lands_under_that_site() {
    let mut project = empty_project();
    let sheet = Sheet::new(
        template_headers(),
        vec![row_with_site(
            "dc-主中心",
            "redis-vm-01",
            "redis",
            "10.1.0.11:6379",
        )],
    );
    import(&mut project, &sheet).unwrap();

    let env = &project.environments[0];
    let site: Vec<_> = env
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Site)
        .collect();
    assert_eq!(site.len(), 1);
    assert_eq!(site[0].slug, "dc-主中心");
    assert!(site[0].instances.is_empty(), "站點本身不跑東西");

    let node: Vec<&str> = site[0].children.iter().map(|n| n.slug.as_str()).collect();
    assert_eq!(node, vec!["app-vm-01", "redis-vm-01"]);
    assert!(
        env.nodes.iter().all(|n| n.kind == NodeKind::Site),
        "最外層只該有站點，機器要在站點底下"
    );
}

#[test]
fn without_a_site_the_structure_stays_flat() {
    // 既有的試算表沒有這兩欄，匯進來的結果必須跟以前一模一樣。
    let mut project = empty_project();
    let sheet = Sheet::new(template_headers(), vec![direct_row()]);
    import(&mut project, &sheet).unwrap();

    let env = &project.environments[0];
    assert!(env.nodes.iter().all(|n| n.kind != NodeKind::Site));
    assert!(env.nodes.iter().all(|n| n.children.is_empty()));
}

#[test]
fn adding_site_columns_and_reimporting_relocates_existing_nodes() {
    // 真實流程：先匯了一份沒有站點的表，後來補上欄位再匯一次。
    // 機器應該被搬進站點，而不是變成兩台同名的。
    let mut project = empty_project();
    let without_site = Sheet::new(
        template_headers(),
        vec![row_from_pairs(&[
            ("environment", "prod"),
            ("purpose", "測試"),
            ("serves", "app-連-redis"),
            ("from_node", "app-vm-01"),
            ("from_service", "app"),
            ("to_node", "redis-vm-01"),
            ("to_service", "redis"),
            ("to_endpoint", "client-port"),
            ("to_address", "10.1.0.11:6379"),
            ("protocol", "TCP"),
        ])],
    );
    import(&mut project, &without_site).unwrap();

    let with_site = Sheet::new(
        template_headers(),
        vec![row_with_site(
            "dc-主中心",
            "redis-vm-01",
            "redis",
            "10.1.0.11:6379",
        )],
    );
    import(&mut project, &with_site).unwrap();

    let env = &project.environments[0];
    assert_eq!(env.nodes.len(), 1, "最外層只該剩下站點：{:?}", env.nodes);
    assert_eq!(env.nodes[0].children.len(), 2);
    assert_eq!(env.instances().len(), 2, "不該產生重複的機器");
}

#[test]
fn a_wildcard_plus_a_site_narrows_the_scope() {
    // 這是站點欄位真正的價值：讓匯進來的資料也享受得到 within，
    // 否則 expect 還是在數總量，機器搬家抓不到（見 L-C）。
    let mut project = empty_project();
    let sheet = Sheet::new(
        template_headers(),
        vec![
            row_with_site("dc-主中心", "redis-vm-01", "redis", "10.1.0.11:6379"),
            row_with_site("dc-主中心", "redis-vm-02", "redis", "10.1.0.12:6379"),
            row_from_pairs(&[
                ("environment", "prod"),
                ("purpose", "測試"),
                ("serves", "app-連-redis"),
                ("from_site", "dc-主中心"),
                ("from_node", "app-vm-01"),
                ("from_service", "app"),
                ("to_site", "dc-主中心"),
                ("to_node", "redis-vm-*"),
                ("to_service", "redis"),
                ("to_endpoint", "client-port"),
                ("protocol", "TCP"),
                ("expect", "2"),
            ]),
        ],
    );
    import(&mut project, &sheet).unwrap();

    let site = project.environments[0]
        .nodes
        .iter()
        .find(|n| n.slug == "dc-主中心")
        .unwrap()
        .id
        .clone();

    let wildcard_row = project.environments[0]
        .connections
        .iter()
        .find(|c| {
            matches!(
                &c.to,
                Endpointing::Instance {
                    target: InstanceRef::Pattern { .. },
                    ..
                }
            )
        })
        .expect("應該有一條萬用字元的連線");

    let Endpointing::Instance {
        target: InstanceRef::Pattern { within, expect, .. },
        ..
    } = &wildcard_row.to
    else {
        unreachable!()
    };
    assert_eq!(within.as_ref(), Some(&site));
    assert_eq!(*expect, Some(2));

    assert!(
        loom_core::lint::lint(&project).is_empty(),
        "應該是乾淨的：{:?}",
        loom_core::lint::lint(&project)
    );
}

#[test]
fn can_be_marked_as_a_fallback_path() {
    let mut project = empty_project();
    let mut that_row = row_with_site("dc-異地", "redis-vm-09", "redis", "10.2.0.11:6379");
    set_cell(&mut that_row, "kind", "fallback");

    let sheet = Sheet::new(template_headers(), vec![that_row]);
    import(&mut project, &sheet).unwrap();

    assert_eq!(
        project.environments[0].connections[0].kind,
        ConnectionKind::Fallback
    );
}

#[test]
fn an_unknown_kind_rejects_the_whole_sheet() {
    let mut project = empty_project();
    let mut that_row = direct_row();
    set_cell(&mut that_row, "kind", "備用");

    let err = import(
        &mut project,
        &Sheet::new(template_headers(), vec![that_row]),
    )
    .unwrap_err();
    assert!(matches!(
        err,
        loom_core::importer::ImportError::UnknownKind { .. }
    ));
}

#[test]
fn a_contract_named_via_serves_carries_the_purpose() {
    // 這條原本是壞的：`ensure_relationship_by_slug` 傳空字串當用途，
    // 所以用 serves 命名的契約永遠帶著一個 L007 警告，
    // 而且從試算表修不掉——出貨的樣板本來就會產生六個。
    let mut project = empty_project();
    import(&mut project, &sheet(vec![direct_row()])).unwrap();

    assert!(
        project
            .logical
            .relationships
            .iter()
            .all(|r| !r.purpose.trim().is_empty()),
        "契約的用途沒填：{:?}",
        project.logical.relationships
    );
    assert!(
        !lint(&project).iter().any(|f| f.rule == Rule::L007),
        "不該有「沒填用途」的警告"
    );
}

#[test]
fn the_shipped_template_imports_without_purpose_warnings() {
    // 直接拿出貨的樣板驗。使用者第一次用就看到六個修不掉的警告，
    // 會直接學會忽略 lint——那這個工具就廢了。
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/excel/連線匯入樣板.csv");
    let path = if path.exists() {
        path
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/excel/connection-import-template.csv")
    };
    let sheet = read_csv(&path).expect("讀得到樣板");

    let mut project = empty_project();
    import(&mut project, &sheet).unwrap();

    let missing_purpose: Vec<_> = lint(&project)
        .into_iter()
        .filter(|f| f.rule == Rule::L007)
        .collect();
    assert!(missing_purpose.is_empty(), "{missing_purpose:?}");
}
