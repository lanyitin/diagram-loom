//! Agent 的說明書：工具清單與每一支的描述。
//!
//! # 為什麼獨立成一個模組
//!
//! 這些字**就是** Agent 唯一的說明書。它看不到程式碼、看不到文件，
//! 只看得到這裡。寫不清楚它就會用錯，而用錯的成本是使用者的專案被弄髒。
//!
//! 所以它跟實作分開：改描述不必捲過幾百行 `match`，
//! 而讀描述的時候看到的是一份連貫的說明，不是散在實作之間的碎片。
//!
//! # 三條規矩，寫在每一支上
//!
//! Agent 每碰一支沒用過的工具，都得先判斷「這一支是哪一種寫法」。
//! 以前有五種：三種批次形狀、三種環境給法、兩種指名方式。所以現在是：
//!
//! | | 形狀 |
//! | --- | --- |
//! | **讀** | `{scope, select, detail}` |
//! | **寫** | `{scope, items, dry_run}` |
//! | **指名既有的東西** | `target` 一律吃名字（見 [`crate::target`]） |
//!
//! `scope.environment` 是**環境唯一的家**。以前 `create` 藏在每一項的
//! `fields` 裡、`add_connection` 與 `create_nodes` 放頂層、`describe` 根本
//! 不收——三種寫法沒有一種是猜得到的。
//!
//! # 舊形狀照樣收，但這裡只教新的
//!
//! 兩種寫法都寫上去只會讓模型猶豫。收還是兩種都收（見
//! [`Scope::of`](crate::query::Scope::of) 與 `tools::items_of`）：
//! 模型有時會憑印象送出舊的形狀，那時候回一個它得重試才懂的錯誤，不如照做。

use serde_json::{Value, json};

use loom_core::resource::blank;

use crate::query::KINDS;
use crate::tools::{CONNECTION_FIELDS, accepted_fields, create_only_fields};

/// 這一支收不收 `scope.environment`。
enum Scoped {
    /// `projects` 專用：它問的正是「有哪些專案可以挑」，環境對它沒有意義。
    ProjectOnly,
    Full,
}

/// 工具清單。
pub fn list() -> Value {
    json!([
        tool(
            "projects",
            "\
使用者現在開著哪幾個專案，各自有沒有未儲存、lint 還剩幾項。

**同時開著多個的時候先叫這個。** 其他工具都收一個選填的
`scope`: {\"project\": \"名字或路徑片段\"}，只開一個時可以省略；
開著多個又沒給的話會被擋下來——不會替你猜一個。",
            json!({"type": "object", "properties": {}}),
            Scoped::ProjectOnly,
        ),
        tool(
            "describe",
            &format!(
                "\
看專案裡已經有什麼。**動手之前先叫這個**——不然你會重複建立已經存在的
東西，而那會撞名失敗。

`scope.environment` 省略時**每個環境都看**。指定了就只看那一個。

`select` 用來只看一部分。整份專案可能有數百條連線，全部倒出來對你我都貴：
  kinds  只要這幾種。可以用：{}
  match  slug 要符合的樣式，`*` 比對零到多個字元（例如 `redis-*`）。
         連線沒有自己的名字，所以它比對的是**兩端**——「redis 那幾條」。

`detail` 決定要多細：
  count  只有每一種各幾個。想確認「建完了嗎」用這一檔。
  brief  預設。看得出是什麼、缺什麼，但**不含 id**。
  full   含 id 與每一台的位址。

**平常不需要 id。** `update` 與 `delete` 的 `target` 直接吃名字
（`redis-01`、`apache-* -> f5-01`），所以 brief 就夠用了。
真的要 id 再開 full。",
                KINDS.join("、")
            ),
            json!({
                "type": "object",
                "properties": {
                    "select": {
                        "type": "object",
                        "properties": {
                            "kinds": {
                                "type": "array",
                                "items": {"type": "string", "enum": KINDS},
                                "description": "只看這幾種。省略＝全部。"
                            },
                            "match": {
                                "type": "string",
                                "description": "slug 的樣式，`*` 比對零到多個字元。"
                            }
                        }
                    },
                    "detail": {
                        "type": "string",
                        "enum": ["count", "brief", "full"],
                        "description": "預設 brief（不含 id）。"
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "lint",
            "\
看專案現在還缺什麼。

**這是你自我檢查的方式。** 從一段文字建模型一定會漏東西——漏接點、
漏某個環境、漏一段路徑。每建完一批就叫一次，照著它說的補。
每一項都會告訴你怎麼修。

`detail: \"count\"` 只回一行「幾個錯誤、幾個警告」。**建完一批之後先用
這一檔**，數字不是零再展開成 brief 去看細節——那省下的是每一輪的整份報告。

`select` 用來只看一部分：
  rules     只要這幾條規則，例如 [\"L002\"]
  severity  至少這麼嚴重：error / warning / info

指定 `scope.environment` 時，**邏輯層的問題還是會列出來**——
它對每個環境都成立，藏起來的話你會在某個環境裡找不到病因。",
            json!({
                "type": "object",
                "properties": {
                    "select": {
                        "type": "object",
                        "properties": {
                            "rules": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "規則代號，例如 [\"L002\", \"L006\"]。"
                            },
                            "severity": {
                                "type": "string",
                                "enum": ["error", "warning", "info"],
                                "description": "至少這麼嚴重。"
                            }
                        }
                    },
                    "detail": {
                        "type": "string",
                        "enum": ["count", "brief", "full"],
                        "description": "預設 brief。count 只回一行數字。"
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "create",
            "\
建立模型元素。**一次送一批，不要一個一個送。**

`items` 是一個陣列，你這一輪想建的東西全部放進去，包括不同 kind 的。
一趟一百個跟一趟一個花的時間差不多，而一個一個送要走一百趟。

同一批裡**後面的可以指名前面的**，所以照下面的順序排就好，
不必為了先後關係拆成很多趟：
  1. system        系統。服務要掛在它底下
  2. container     服務（Redis、Consul 這種會接收請求的程序）
  3. endpoint_def  接點定義。契約要指定連到哪一個
  4. relationship  契約：誰連誰。這是母版，每個環境都必須實現
  5. environment   環境（prod / uat / dev…）
  6. node / infra / system_instance / instance   環境裡的機器、F5、實體

**有一項失敗就整批不建**，並且告訴你是第幾項、為什麼。
改掉那一項再整批送一次即可——沒有「建了一半」這種狀態。
整批也算**一步**，使用者按一次 ⌘Z 就能全部退掉。

環境層的那幾種（node / infra / infra_endpoint / instance / system_instance）
**環境寫在 `scope` 裡**，不要寫進每一項的 fields。跨環境請分開送。

各種 kind 要填的欄位：
  system            slug, name, external(bool)
  container         slug, name, system(系統的 slug)
  endpoint_def      owner(服務或系統的 slug), slug, protocol(tcp/udp/unix-socket/jdbc/file)
  relationship      slug, purpose, from, to, to_endpoint
                    from/to 寫 `container:名字`、`system:名字` 或 `person:名字`
                    to_endpoint 寫目標身上那個接點定義的 slug
  person            slug, name
  environment       slug, name
  node              slug, kind(site/physical/virtual-machine/linux-container), within(選填)
  infra             slug
  infra_endpoint    owner(設備的 slug), slug, address
  instance          node(機器的 slug), slug, container(服務的 slug),
                    address, endpoint(選填), standalone(選填 bool)
                    **一台機器上要跑第二個服務就用這個**（例如 app 跟 log-agent
                    同機）。node 填那台已經存在的機器。
  system_instance   slug, system(系統的 slug), address, endpoint(選填)
                    address 是「這個外部系統在這個環境打哪裡」，例如 sso.corp.local:443。
                    那個系統有好幾個接點定義時才需要 endpoint 指名要填哪一個。

上面每一種都另外收 **memo**：給人看的備註，例如「等年底汰換」。
沒有任何 lint 規則會讀它，所以拿它記規則管不到的事，
不要把這種話塞進 purpose——那個欄位 L007 在看。

要一次建很多台**同一個服務**的機器請改用 create_nodes，它會照樣板配 IP，
但**每個服務實體都會配一台新機器**。要共用機器就用上面的 instance。

連線不在這裡建，用 add_connection——它的兩端要用名字解析，跟這裡不一樣。",
            json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "description": "這一批要建的東西。不同 kind 可以混在同一批。",
                        "items": {
                            "type": "object",
                            "required": ["kind", "fields"],
                            "properties": {
                                "kind": {
                                    "type": "string",
                                    "enum": KINDS.iter().filter(|k| **k != "connection")
                                        .collect::<Vec<_>>()
                                },
                                "fields": {"type": "object", "description": "見上面各 kind 的欄位"}
                            }
                        }
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "create_nodes",
            "\
一次建立多台機器，每台上面跑一個指定的服務。**一次送一批**，
`items` 裡放這個環境要開的每一群機器。

六台 Redis 手打六次會打錯，而**打錯的通常是 IP，錯了不會有人發現**。
所以用樣板：`{n}` 是序號、`{ip}` 是位址序號，兩個分開數
（現實中 redis-01…redis-06 常常對到 10.0.1.11…10.0.1.16）。

一項失敗就整批不建，並且告訴你是第幾項。

要在**已經存在的機器**上加第二個服務，這一支做不到——
它一定會開新機器。那種情況用 create 的 instance，node 填那台機器。",
            json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "required": ["container", "count", "address_template"],
                            "properties": {
                                "container": {"type": "string", "description": "服務的 slug"},
                                "count": {"type": "integer", "minimum": 1},
                                "name_template": {"type": "string", "description": "服務實體名稱，預設 `<服務>-{n}`"},
                                "node_template": {"type": "string", "description": "機器名稱，預設 `vm-<服務>-{n}`"},
                                "address_template": {"type": "string", "description": "例如 `10.0.1.{ip}:6379`"},
                                "endpoint": {"type": "string", "description": "用服務身上的哪個接點定義。只有一個時可省略"},
                                "start": {"type": "integer", "description": "`{n}` 從幾號開始，預設 1"},
                                "pad": {"type": "integer", "description": "`{n}` 補零幾位，預設 2"},
                                "ip_start": {"type": "integer", "description": "`{ip}` 從幾號開始，預設 1"},
                                "node_kind": {"type": "string", "enum": ["physical", "virtual-machine", "linux-container"]},
                                "within": {"type": "string", "description": "放在哪個節點（通常是站點）底下"}
                            }
                        }
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "add_connection",
            "\
在某個環境實現契約。**一次送一批**，`items` 裡放這個環境要建的所有連線。

連線通常是整份專案裡數量最多的東西（數百條），一條一趟會慢到不能用。

`from` / `to` 用名字寫，不要用 id：
  redis-01              那一台
  redis-*               一整群（會自動幫你算 expect）
  redis-* @ dc-main     限定在某個站點底下的那一群
  f5-01 : vip-redis     設備上的某個 VIP
  person:customer       人（只能當來源）
  system:payment        外部系統的實體

一項裡兩個都省略的話就照提案建（直達的一段）。
**想先看提案長什麼樣就開 `dry_run: true`**——它會擬給你看，一個都不建。

**經過 F5 的流量要拆成兩段**，兩段都填同一個 relationship，
放在同一批裡就好：
  第一段  from: apache-*   to: f5-01 : vip-gateway
  第二段  from: f5-01      to: gateway-*
少了第二段的話 lint 會說走不通（L002）——那正是它該抓的東西。

有一項失敗就整批不建，並且告訴你是第幾項。",
            json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "required": ["relationship"],
                            "properties": {
                                "relationship": {"type": "string", "description": "契約的 slug"},
                                "from": {"type": "string"},
                                "to": {"type": "string"},
                                "purpose": {"type": "string", "description": "省略時沿用契約的用途"},
                                "fallback": {"type": "boolean", "description": "只在故障時才走的備援路徑"}
                            }
                        }
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "update",
            &update_manual(),
            json!({
                "type": "object",
                "required": ["items"],
                "properties": {
                    "items": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "required": ["target", "fields"],
                            "properties": {
                                "target": {
                                    "type": "string",
                                    "description": "要改哪一個。名字、`來源 -> 目標`（連線）或 id。"
                                },
                                "fields": {"type": "object", "description": "只填要改的欄位。"}
                            }
                        }
                    }
                }
            }),
            Scoped::Full,
        ),
        tool(
            "delete",
            "\
刪掉元素或連線。**兩步走：先看，再確認。**

`targets` 跟 update 的 `target` 是同一套寫法：名字、`來源 -> 目標`（連線）
或 id。

第一次呼叫**不填 confirm**，它只會告訴你「會刪掉哪些、之後會壞掉什麼」，
不動任何東西。看過覺得對，再帶 `confirm: true` 送同一組參數。

要大改結構就開 `cascade: true`。它會把「刪了這個之後會變成廢的」
一路掃出來（指著空氣的契約、連線、服務實體…），一次清乾淨。
沒有它的話，刪一個系統要來回幾十趟，每趟都得先跑 lint 才知道下一個刪誰。

`cascade: false`（預設）就是只刪你指名的，
懸空的東西留給 lint（L012）叫。人在畫面上一個一個處理時這樣是對的。

⚠ cascade 一次可能消失幾十個東西。**預覽不是形式**，看清楚再確認。
刪完仍然是未儲存狀態，使用者可以按 ⌘Z 一次退回整批。",
            json!({
                "type": "object",
                "required": ["targets"],
                "properties": {
                    "targets": {
                        "type": "array",
                        "minItems": 1,
                        "description": "要刪的東西。名字、`來源 -> 目標` 或 id。",
                        "items": {"type": "string"}
                    },
                    "cascade": {
                        "type": "boolean",
                        "description": "連帶刪掉會因此變成廢的東西。預設 false。"
                    },
                    "confirm": {
                        "type": "boolean",
                        "description": "true 才真的刪。省略＝只看預覽。"
                    }
                }
            }),
            Scoped::Full,
        ),
    ])
}

/// `update` 的說明書。
///
/// # 為什麼欄位清單是**長出來**的，不是手寫的
///
/// 這份清單以前根本不存在——`update` 只說「只填要改的欄位」，Agent 得去讀
/// `create` 的說明才知道有哪些。而兩邊收的欄位**不一樣**（`environment`、
/// `owner`、`within` 只有 create 讀得到），所以照著抄的那一次會安靜地失敗。
///
/// 手寫第二份的話，下次加欄位一定有一邊會漏。所以這裡直接問
/// [`accepted_fields`]——它是實作照著擋的那一份，長出來的清單不可能跟它不一致。
fn update_manual() -> String {
    let mut out = String::from(
        "\
改一個既有元素的欄位。**一次送一批**，`items` 裡放這一輪要改的全部。

`target` 指名要改哪一個，吃的是**名字**：
  redis-01              那個元素（服務實體、機器、服務、契約…都一樣）
  apache-* -> f5-01     那條連線。你在 add_connection 裡就是這樣寫的
  conn:apache-to-redis  服務那條契約的連線（這個環境只有一條時）
  一串 UUID             那個 id 也收

同一個名字在好幾個環境各有一個時（`redis-01` 在 prod 與 uat 都有是常態），
會被擋下來並列出全部候選——加 `scope`: {\"environment\": \"…\"} 指定。
**不會替你挑一個**：挑錯是安靜地改到別的環境，那比失敗糟得多。

只填要改的欄位，沒填的不動。有一項失敗就整批不改。

服務實體給 `node` 就是**搬到另一台機器上**（舊的那台不會留下一份）。

換環境、換擁有者做不到——那不是改一個欄位，是換一個東西。
帶了會被擋下來並且告訴你怎麼做，不會假裝改好了。

各種東西收哪些欄位（欄位的寫法見 create 的說明）：\n",
    );

    for key in KINDS {
        if *key == "connection" {
            continue;
        }
        let kind = match crate::query::kind_of(key) {
            Ok(k) => k,
            Err(_) => continue,
        };
        let sample = blank(kind, None, None);
        // 扣掉建立之後就搬不動的那幾個。列出來的話 Agent 會照著填，
        // 然後拿到一句「這個改不了」——而那是我們自己告訴它可以填的。
        let stuck = create_only_fields(&sample);
        let fields: Vec<&str> = accepted_fields(&sample)
            .iter()
            .copied()
            .filter(|f| !stuck.contains(f))
            .collect();
        out.push_str(&format!("  {key:<17} {}\n", fields.join(", ")));
    }

    // 連線不是 Resource，欄位另外一份，但一樣不手抄。
    out.push_str(&format!(
        "  {:<17} {}\n",
        "connection",
        CONNECTION_FIELDS.join(", ")
    ));
    out.push_str(
        "\n\
連線那一行是新的，以前改不了——只能刪掉重建，而重建會換一個新的 id。\n\
`kind` 是 primary（平常走的）或 fallback（只在故障時走的）。\n\
`from_expect` / `to_expect` 是萬用字元那一端「應該要有幾台」\n\
（L004／L005 在看的就是它），填 null 是清掉。",
    );
    out
}

/// 每個工具都自動長出一個 `scope`。
///
/// 在這裡補而不是在八個 schema 裡各抄一次：抄的話遲早有一個會漏掉，
/// 而症狀是「那個工具沒辦法指定環境」——Agent 只會覺得莫名其妙。
fn tool(name: &str, description: &str, mut schema: Value, scoped: Scoped) -> Value {
    let mut props = json!({
        "project": {
            "type": "string",
            "description": "要動哪一個專案（名字或路徑片段）。\
                只開著一個的時候可以省略；開著多個又沒給，會回一份清單要你指定。"
        }
    });
    if let Scoped::Full = scoped {
        props.as_object_mut().unwrap().insert(
            "environment".into(),
            json!({
                "type": "string",
                "description": "要動哪一個環境（slug）。**環境只寫在這裡。**\
                    讀取工具省略時＝全部環境。"
            }),
        );
    }

    // 寫入工具一律收 dry_run。「我想先看看」在每一支上都是同一個答案，
    // 就不會有人為了看一眼而真的建下去。判斷的依據是它收不收 `items`——
    // 那正好就是「這一支會改東西」的定義。
    let writes = schema
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(|v| v == "items"));

    if let Some(top) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        top.insert(
            "scope".into(),
            json!({"type": "object", "properties": props}),
        );
        if writes {
            top.insert(
                "dry_run".into(),
                json!({
                    "type": "boolean",
                    "description": "true 就只算給你看，一個都不會動。預設 false。"
                }),
            );
        }
    }
    json!({ "name": name, "description": description, "inputSchema": schema })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每一支都要有 `scope`，而寫入的那幾支還要有 `dry_run`。
    ///
    /// 這條測試守的是「形狀一致」本身。少了一個的那一支不會壞掉，
    /// 只是 Agent 在它身上做不到別支做得到的事——而那是最看不出來的缺口。
    #[test]
    fn every_tool_has_a_scope_and_every_writer_has_a_dry_run() {
        for t in list().as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            let props = &t["inputSchema"]["properties"];
            assert!(props.get("scope").is_some(), "{name} 少了 scope");

            let writes = t["inputSchema"]["required"]
                .as_array()
                .is_some_and(|r| r.iter().any(|v| v == "items"));
            assert_eq!(
                props.get("dry_run").is_some(),
                writes,
                "{name} 的 dry_run 跟它是不是寫入工具對不起來"
            );
        }
    }

    /// `projects` 不該收環境——它問的正是「有哪些專案可以挑」。
    #[test]
    fn only_projects_is_free_of_the_environment_field() {
        for t in list().as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            let has_env = t["inputSchema"]["properties"]["scope"]["properties"]
                .get("environment")
                .is_some();
            assert_eq!(has_env, name != "projects", "{name}");
        }
    }

    /// create 的說明裡要提到每一種 kind 實際收得到的每一個欄位。
    ///
    /// # 這條在守什麼
    ///
    /// `update` 的欄位清單是長出來的，`create` 的是手寫的（因為要解釋
    /// `container:名字` 那種寫法）。手寫那份漏掉一個新欄位的話，症狀是
    /// 「這個欄位存得進去，但沒有人知道它存在」——不會有任何錯誤。
    #[test]
    fn the_create_manual_mentions_every_field_it_accepts() {
        let tools = list();
        let create = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "create")
            .unwrap();
        let text = create["description"].as_str().unwrap();

        for key in KINDS {
            let Ok(kind) = crate::query::kind_of(key) else {
                continue;
            };
            for field in accepted_fields(&blank(kind, None, None)) {
                assert!(
                    text.contains(field),
                    "create 的說明沒提到 {key} 收的 {field}"
                );
            }
        }
    }
}
