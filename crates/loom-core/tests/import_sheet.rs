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

fn 空專案() -> Project {
    Project {
        id: Id::generate(),
        slug: "shop".into(),
        name: "網路商店".into(),
        logical: Logical::default(),
        environments: vec![],
    }
}

fn 表(rows: Vec<Vec<String>>) -> Sheet {
    Sheet::new(template_headers(), rows)
}

fn 一列(pairs: &[(&str, &str)]) -> Vec<String> {
    row_from_pairs(pairs)
}

/// 改某一欄的值。**依欄位名稱找位置**——寫死索引的話，
/// 樣板多一欄就整批測試錯位，而且錯得很難看懂。
fn 改欄(row: &mut [String], 欄位: &str, 值: &str) {
    let i = template_headers()
        .iter()
        .position(|h| h == 欄位)
        .unwrap_or_else(|| panic!("樣板沒有 {欄位} 這一欄"));
    row[i] = 值.into();
}

/// 最單純的一列：dev 環境，API 直連 Redis。
fn 直連那列() -> Vec<String> {
    一列(&[
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
fn 一列就建出環境機器服務與連線() {
    let mut project = 空專案();
    let report = import(&mut project, &表(vec![直連那列()])).unwrap();

    assert_eq!(report.connections, 1);
    assert_eq!(report.environments_created, 1);
    assert_eq!(report.containers_created, 2);
    assert_eq!(report.instances_created, 2);

    let dev = &project.environments[0];
    assert_eq!(dev.slug, "dev");
    assert_eq!(dev.instances().len(), 2);
}

#[test]
fn 實例名稱是機器加服務() {
    // 一台機器可能跑多個服務，光用機器名會撞在一起。
    let mut project = 空專案();
    import(&mut project, &表(vec![直連那列()])).unwrap();

    let mut slugs: Vec<String> = project.environments[0]
        .instances()
        .iter()
        .map(|i| i.slug.clone())
        .collect();
    slugs.sort();
    assert_eq!(slugs, vec!["app-vm-01-order-api", "redis-vm-01-redis"]);
}

#[test]
fn 同一台機器跑兩個服務不會撞名() {
    let mut project = 空專案();
    let 第二列 = 一列(&[
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

    import(&mut project, &表(vec![直連那列(), 第二列])).unwrap();

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
fn 重複匯入不會產生重複的東西() {
    // 使用者更新試算表後重跑，既有資料應該原地更新而不是複製一份。
    let mut project = 空專案();
    import(&mut project, &表(vec![直連那列()])).unwrap();
    let 第一次 = project.environments[0].instances().len();

    import(&mut project, &表(vec![直連那列()])).unwrap();

    assert_eq!(project.environments[0].instances().len(), 第一次);
    assert_eq!(project.logical.containers.len(), 2);
    assert_eq!(project.environments.len(), 1);
}

#[test]
fn 匯入結果可以直接跑lint() {
    let mut project = 空專案();
    import(&mut project, &表(vec![直連那列()])).unwrap();

    let findings = lint(&project);
    // 來源端沒填 endpoint（OS 分配）是正常的；不該有任何 Error。
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity() == loom_core::lint::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "匯入的結果有 Error：{errors:?}");
}

#[test]
fn 經過設備會拆成兩段而且共用同一條邏輯連線() {
    let mut project = 空專案();
    let 第一段 = 一列(&[
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
    let 第二段 = 一列(&[
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

    import(&mut project, &表(vec![第一段, 第二段])).unwrap();

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
fn 目標是設備又沒填serves會被擋下() {
    let mut project = 空專案();
    let row = 一列(&[
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
        import(&mut project, &表(vec![row])),
        Err(ImportError::InfraRowNeedsServes { row: 2 })
    );
}

#[test]
fn 萬用字元那列不會建出新機器() {
    // 萬用字元是在指涉既有的一群機器，不是在定義新機器。
    let mut project = 空專案();
    let rows = vec![
        直連那列(),
        // 萬用字元的列不必填 to_address：位址由各機器自己的定義提供。
        一列(&[
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

    import(&mut project, &表(rows)).unwrap();

    // 仍然只有直連那列建出的兩台
    assert_eq!(project.environments[0].instances().len(), 2);

    let 萬用那條 = &project.environments[0].connections[1];
    let Endpointing::Instance { target, .. } = &萬用那條.to else {
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
fn 同一個endpoint位址不一致時保留先出現的並提醒() {
    let mut project = 空專案();
    let mut 打錯的 = 直連那列();
    改欄(&mut 打錯的, "to_address", "10.0.1.99:6379");

    let report = import(&mut project, &表(vec![直連那列(), 打錯的])).unwrap();

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
fn 缺必填欄位時一次列出全部() {
    let mut project = 空專案();
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
fn 認不得的協定會指出可用選項() {
    let mut project = 空專案();
    let mut row = 直連那列();
    改欄(&mut row, "protocol", "SMTP");

    let err = import(&mut project, &表(vec![row])).unwrap_err();
    assert!(err.to_string().contains("TCP"), "沒列出可用選項：{err}");
}

#[test]
fn 名稱不是正規寫法時給建議() {
    let mut project = 空專案();
    let mut row = 直連那列();
    改欄(&mut row, "from_node", "App VM 01");

    let err = import(&mut project, &表(vec![row])).unwrap_err();
    assert!(
        err.to_string().contains("app-vm-01"),
        "沒給出建議寫法：{err}"
    );
}

#[test]
fn 認不得的欄位只是警告不中斷() {
    let mut project = 空專案();
    let mut headers = template_headers();
    headers.push("負責人".into());
    let mut row = 直連那列();
    row.push("小明".into());

    let report = import(&mut project, &Sheet::new(headers, vec![row])).unwrap();

    assert_eq!(report.connections, 1);
    assert_eq!(
        report.warnings,
        vec![ImportWarning::UnknownColumn("負責人".into())]
    );
}

#[test]
fn 讀得懂給使用者填的那份樣板() {
    // 樣板若跟程式對不上，使用者第一次用就會撞牆。
    let 樣板 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");

    let sheet = read_csv(&樣板).expect("讀不到樣板檔");
    assert!(sheet.row_count() >= 8, "樣板應該有多列範例");

    let mut project = 空專案();
    let report = import(&mut project, &sheet).expect("樣板無法匯入");

    assert_eq!(report.connections, sheet.row_count());
    assert_eq!(project.environments.len(), 2, "樣板涵蓋 prod 與 test");
}

#[test]
fn 樣板的prod環境匯入後是乾淨的() {
    let 樣板 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");
    let sheet = read_csv(&樣板).unwrap();

    let mut project = 空專案();
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
fn 樣板的test環境只有一列所以lint會指出缺漏() {
    // 樣板的 test 環境只放一列示範萬用字元寫法，其餘服務都沒部署。
    // lint 抓到這件事正是它該做的——這個測試同時驗證匯入與 lint 接得起來。
    let 樣板 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/excel/connection-import-template.csv");
    let sheet = read_csv(&樣板).unwrap();

    let mut project = 空專案();
    import(&mut project, &sheet).unwrap();

    let test_env = project
        .environments
        .iter()
        .find(|e| e.slug == "test")
        .expect("樣板應該建出 test");

    let 缺漏: Vec<&str> = lint(&project)
        .iter()
        .filter(|f| f.environment.as_ref() == Some(&test_env.id))
        .filter(|f| f.rule == Rule::L001)
        .map(|_| "缺")
        .collect();

    assert!(
        !缺漏.is_empty(),
        "test 環境明顯不完整，lint 卻沒指出任何缺漏"
    );
}

// ── 站點與備援（限制 D）────────────────────────────────

fn 帶站點的一列(site: &str, node: &str, service: &str, address: &str) -> Vec<String> {
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
fn 填了站點的機器會被放進站點底下() {
    let mut project = 空專案();
    let sheet = Sheet::new(
        template_headers(),
        vec![帶站點的一列(
            "dc-主中心",
            "redis-vm-01",
            "redis",
            "10.1.0.11:6379",
        )],
    );
    import(&mut project, &sheet).unwrap();

    let env = &project.environments[0];
    let 站點: Vec<_> = env
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Site)
        .collect();
    assert_eq!(站點.len(), 1);
    assert_eq!(站點[0].slug, "dc-主中心");
    assert!(站點[0].instances.is_empty(), "站點本身不跑東西");

    let 機器: Vec<&str> = 站點[0].children.iter().map(|n| n.slug.as_str()).collect();
    assert_eq!(機器, vec!["app-vm-01", "redis-vm-01"]);
    assert!(
        env.nodes.iter().all(|n| n.kind == NodeKind::Site),
        "最外層只該有站點，機器要在站點底下"
    );
}

#[test]
fn 沒填站點時維持原本的扁平結構() {
    // 既有的試算表沒有這兩欄，匯進來的結果必須跟以前一模一樣。
    let mut project = 空專案();
    let sheet = Sheet::new(template_headers(), vec![直連那列()]);
    import(&mut project, &sheet).unwrap();

    let env = &project.environments[0];
    assert!(env.nodes.iter().all(|n| n.kind != NodeKind::Site));
    assert!(env.nodes.iter().all(|n| n.children.is_empty()));
}

#[test]
fn 補上站點欄位重新匯入機器會自己歸位() {
    // 真實流程：先匯了一份沒有站點的表，後來補上欄位再匯一次。
    // 機器應該被搬進站點，而不是變成兩台同名的。
    let mut project = 空專案();
    let 沒站點 = Sheet::new(
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
    import(&mut project, &沒站點).unwrap();

    let 有站點 = Sheet::new(
        template_headers(),
        vec![帶站點的一列(
            "dc-主中心",
            "redis-vm-01",
            "redis",
            "10.1.0.11:6379",
        )],
    );
    import(&mut project, &有站點).unwrap();

    let env = &project.environments[0];
    assert_eq!(env.nodes.len(), 1, "最外層只該剩下站點：{:?}", env.nodes);
    assert_eq!(env.nodes[0].children.len(), 2);
    assert_eq!(env.instances().len(), 2, "不該產生重複的機器");
}

#[test]
fn 萬用字元加站點會限定範圍() {
    // 這是站點欄位真正的價值：讓匯進來的資料也享受得到 within，
    // 否則 expect 還是在數總量，機器搬家抓不到（見 L-C）。
    let mut project = 空專案();
    let sheet = Sheet::new(
        template_headers(),
        vec![
            帶站點的一列("dc-主中心", "redis-vm-01", "redis", "10.1.0.11:6379"),
            帶站點的一列("dc-主中心", "redis-vm-02", "redis", "10.1.0.12:6379"),
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

    let 站點 = project.environments[0]
        .nodes
        .iter()
        .find(|n| n.slug == "dc-主中心")
        .unwrap()
        .id
        .clone();

    let 萬用那條 = project.environments[0]
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
    } = &萬用那條.to
    else {
        unreachable!()
    };
    assert_eq!(within.as_ref(), Some(&站點));
    assert_eq!(*expect, Some(2));

    assert!(
        loom_core::lint::lint(&project).is_empty(),
        "應該是乾淨的：{:?}",
        loom_core::lint::lint(&project)
    );
}

#[test]
fn 可以標成備援路徑() {
    let mut project = 空專案();
    let mut 那列 = 帶站點的一列("dc-異地", "redis-vm-09", "redis", "10.2.0.11:6379");
    改欄(&mut 那列, "kind", "fallback");

    let sheet = Sheet::new(template_headers(), vec![那列]);
    import(&mut project, &sheet).unwrap();

    assert_eq!(
        project.environments[0].connections[0].kind,
        ConnectionKind::Fallback
    );
}

#[test]
fn 認不得的種類會整份拒絕() {
    let mut project = 空專案();
    let mut 那列 = 直連那列();
    改欄(&mut 那列, "kind", "備用");

    let err = import(&mut project, &Sheet::new(template_headers(), vec![那列])).unwrap_err();
    assert!(matches!(
        err,
        loom_core::importer::ImportError::UnknownKind { .. }
    ));
}

#[test]
fn 用_serves_命名的契約會帶上用途() {
    // 這條原本是壞的：`ensure_relationship_by_slug` 傳空字串當用途，
    // 所以用 serves 命名的契約永遠帶著一個 L007 警告，
    // 而且從試算表修不掉——出貨的樣板本來就會產生六個。
    let mut project = 空專案();
    import(&mut project, &表(vec![直連那列()])).unwrap();

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
fn 樣板匯進來不會有沒填用途的警告() {
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

    let mut project = 空專案();
    import(&mut project, &sheet).unwrap();

    let 沒填用途: Vec<_> = lint(&project)
        .into_iter()
        .filter(|f| f.rule == Rule::L007)
        .collect();
    assert!(沒填用途.is_empty(), "{沒填用途:?}");
}
