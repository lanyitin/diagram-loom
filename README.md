<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/diagram-loom-logo-dark.svg">
    <img src="assets/diagram-loom-logo.svg" alt="DiagramLoom" width="320">
  </picture>
</p>

<p align="center">
  管理系統的部署與連線資訊，並產出 <b>C4 Model</b> 圖的桌面應用程式。<br>
  重點不是畫圖，是<b>數百條連線一條都不要漏</b>。
</p>

這個 repo 是 diagram-loom 的**原始碼**：Rust 核心 + Tauri 殼 + Vue 前端。

## 這個專案解決什麼問題

手上的系統有數百條連線。Excel 記得零零落落，Visio 圖畫完就過期，
而真正會出事的是「某個環境漏接了一條線，但沒有人發現」。

所以 diagram-loom 把重心放在**連線的完整性管理 + Linting**：

- 你在**邏輯層**宣告「有哪些服務、誰要連誰」（這是契約）
- 每個**環境**（prod / test / dev）填「實際跑在哪台機器、IP 是什麼、幾個節點」
- **通則：邏輯層有的東西，每個環境都必須實現，沒實現就報錯**

存檔時跑 lint，條列出這個環境還缺什麼。日常用表格看，簡報時產出圖。

### 核心概念（一分鐘版）

**兩層：邏輯層是母版，環境層是分身。**

| 層 | 有什麼 | 舉例 |
| --- | --- | --- |
| 邏輯層 | `Container`、`Relationship`、`EndpointDef` | 「訂單服務要連 Redis」 |
| 環境層 | `ContainerInstance`、`Connection`、`DeploymentNode` | 「prod 的訂單服務在 3 台機器上，連到 10.0.1.5:6379」 |

命名**全面對齊 C4**。注意本專案的 `Container` 是**服務**（Redis、Consul），
機器叫 `Deployment Node`，F5 這類 VIP 設備叫 `Infrastructure Node`。

產出的圖是 **Deployment**（主場）、**Context**、**Container**。不做 Component。

### 非目標

- **不做** code → diagram 的逆向工程
- **不掃描真實環境**做自動發現
- **不做 C4 的 Component 層**

## 目前狀態

**階段 0–6 已完成**，下一步是階段 7：**對帳面板的 UI**。
三方比對的判斷已經在 Rust 做完了，缺的是讓人**逐項裁決**差異的那個畫面——
那也是這個專案最難的一塊。

能做的事：

- **表格檢視**：連線表、覆蓋率矩陣、資源清單
- **Lint 面板**：會叫的規則幾乎都有對應的**就地修法**（補連線、批次建機器、填位址……）
- **Excel / CSV 匯入**：把現有的連線資料灌進來
- **圖**：自動排版（elkjs `layered`）、篩選與調暗、螢光筆標註、折疊、存成 `.drawio`
- **編輯與復原**：快照式 history，刪除前先列出「會弄壞什麼」
- **三方對帳**：圖、模型、base 快照三邊比對，判斷在 Rust（UI 還沒做）
- **AI Agent 介面**：八支 MCP 工具 + 一個 skill

測試數：`loom-core` 384 條、`loom-mcp` 118 條、前端 458 條（另有 `src-tauri` 的 32 條）。

> ⚠️ 畫布已經從內嵌 draw.io 換成自己跑 **maxGraph**（2026-08）。
> `.drawio` 仍然是檔案格式，但編輯器是我們自己的了。

## 快速開始

前置需求：**Rust stable**（edition 2024）、**Node 24**、**pnpm**，
以及 [Tauri 2 的系統相依套件](https://v2.tauri.app/start/prerequisites/)。

```sh
pnpm install
pnpm tauri dev        # 啟動開發模式
```

| 指令 | 做什麼 |
| --- | --- |
| `pnpm tauri dev` | 開發模式。快，但 macOS 上看不到 app icon（`tauri dev` 不做 `.app` bundle） |
| `pnpm tauri build --debug --bundles app` | 打包成 `.app`。**要看 icon 只能用這個** |
| `cargo test -p loom-core` | Rust 領域模型與 lint（秒級） |
| `cargo test -p loom-mcp` | MCP 工具層（秒級） |
| `pnpm run test:unit` | 前端純邏輯與 Vue 元件（秒級） |
| `cargo clippy --workspace --all-targets -- -D warnings` | 靜態檢查 |

### 改了 Rust 的 struct 之後

```sh
cargo run -q -p diagram-loom --bin bindings
```

會重新產生 `src/lib/bindings.ts`。**產出要跟著一起 commit**——
忘了跑的話，型別同步的檢查會紅燈。

### 關於 `mise`

平常的開發走 [mise](https://mise.jdx.dev)：`mise run dev`、`mise run test`、
`mise run check`（提交前跑這個：紀律 + 格式 + 靜態檢查 + 型別同步 + 快速測試）。

但 `mise.toml` 與 `docs/` 放在**外層的 workspace repo**，
不在這個 repo 裡——所以單獨 clone 這裡的話，請直接用上面那張表的原始指令。

## Repo 結構

```
.
├── crates/loom-core/    ★ 純 Rust 核心，零 Tauri 相依
├── crates/loom-mcp/     ★ 給 AI Agent 用的工具層，同樣零 Tauri 相依
├── src-tauri/           Tauri 殼（薄）
├── src/                 Vue + TypeScript
│   ├── components/      面板、表格、對話框
│   ├── lib/graph/       畫布：排版 · 建 cell · 綁定 · 螢光筆 · .drawio 讀寫 · 折疊
│   ├── lib/bindings.ts  從 Rust 產生的型別（不要手改）
│   └── assets/stencils/ 從 draw.io 挑進版控的形狀（119 KB）
├── skills/diagram-loom/ 給使用者裝進 Agent 的 skill
├── prototypes/          可丟棄的驗證原型（各自獨立 workspace，有自己的 README）
├── fixtures/            測試素材（含刻意留了破洞的範例專案）
└── assets/              app icon、logo、社群預覽圖的原始檔
```

`loom-core` 的模組大致是：`logical` / `environment`（領域模型）、`repository`＋`store`
（YAML 讀寫）、`lint`＋`coverage`（檢查）、`importer`（Excel）、`edit`＋`history`
（編輯與復原）、`reconcile`（三方對帳）、`plan`＋`batch`（批次修法）。

### 使用者的專案檔長什麼樣

一個 Project = 一個資料夾。

```
my-project.loom/
├── project.yaml              專案基本資料
├── logical/
│   ├── systems.yaml          Person、SoftwareSystem
│   ├── containers.yaml       Container + EndpointDef
│   └── relationships.yaml    邏輯連線（＝契約）
├── environments/
│   ├── prod.yaml
│   └── test.yaml
└── diagrams/
    ├── prod/
    │   ├── diagram-loom-deployment.drawio  App 產的部署圖（保留字）
    │   ├── diagram-loom-context.drawio     App 產的 context 圖（保留字）
    │   └── 我的簡報圖.drawio                使用者自己建的，名字隨他取
    ├── .catalog.yaml         有哪些圖、各自照著哪一版模型畫的
    └── .sync-state.yaml      對帳用的 base 快照（每張圖一份）
```

一張圖只屬於一個環境。可以拿 `fixtures/sample.loom` 直接開來看。

## 架構

```
┌─ Vue + TypeScript（前端）──────────────────────────────┐
│  ┌─ 面板 ──────────────┐  ┌─ 畫布 ──────────────────┐  │
│  │ 服務清單 / 表格檢視  │  │                        │  │
│  │ 格式面板 / 形狀庫    │◄─┤   maxGraph             │  │
│  │ Lint 面板           │  │   （同一個文件，        │  │
│  │ 標註 / 對帳面板      │─►│     不是 iframe）       │  │
│  └─────────────────────┘  └────────────────────────┘  │
│  lib/graph：排版(elkjs) · 建 cell · 綁定 · 螢光筆        │
│              · .drawio 讀寫 · 手感與互動 · 折疊           │
│  Pinia store（模型的唯讀鏡像，型別由 Rust 自動產生）      │
└──────────────┬─────────────────────────────────────────┘
               │ Tauri commands
┌──────────────┴─ Rust core ────────────────┐
│  domain      領域模型（serde）              │
│  repository  讀寫專案資料夾（YAML）         │
│  lint        可達性 BFS + 覆蓋率檢查        │
│  importer    Excel 匯入                    │
│  reconcile   圖與模型的三方比對              │
└───────────────────────────────────────────┘
```

### 技術選型

| 項目 | 選擇 |
| --- | --- |
| 框架 | **Tauri 2**（桌面應用） |
| 前端 | **Vue 3 + TypeScript**，狀態用 **Pinia** |
| 圖編輯器 | **maxGraph**（我們自己的畫布，不是 iframe） |
| 圖的檔案格式 | **`.drawio`**（讀寫兩邊都自己寫，見 `src/lib/graph/mxml.ts`） |
| 排版 | **elkjs**，只用 `layered`（只有它處理得了巢狀＋跨容器的邊） |
| 儲存 | 一個 Project = **一個資料夾**，內含多個 **YAML** 檔 |
| 元素 ID | **UUID 當身分證 + slug 當顯示名** |
| 型別同步 | **`tauri-specta`** 從 Rust struct 產生 TypeScript 型別 |

### 職責分界：規則在 Rust，畫面與檔案格式在 JavaScript

判斷標準是「這是**規則**還是**轉接**」。

| Rust（核心） | JavaScript（轉接頭） |
| --- | --- |
| 領域模型、Validation / Lint | 全部畫面（Vue） |
| 專案資料夾的佈局與 YAML | 畫布（maxGraph）、`.drawio` 讀寫 |
| Excel / CSV 匯入 | elkjs 排版 |
| **對帳的三方比對判斷** | 從圖上挖出 `[{loomId, label}]` 給 Rust |

對帳的流程：JS 說「**圖上有什麼**」，Rust 說「**這代表什麼**」。
對帳是規則不是畫面——若放 JS，它會和 lint 各自演化成兩套「什麼叫缺漏」的標準。

### ⭐ 紀律：`loom-core` 與 `loom-mcp` 不准相依 `tauri`

這就是 hexagonal 的本體——核心不知道自己被誰呼叫。
這條線決定了大部分邏輯能不能用 `cargo test` 自動驗證，不用開視窗。

`src-tauri` 進了同一個 workspace 之後，把 `tauri` 加進 `loom-core` 會「剛好能編」，
不會有任何錯誤提醒你。所以這條線靠機器守（`mise run guard`，`check` 會跑）：

```sh
grep -qE '^\s*tauri' crates/loom-core/Cargo.toml && echo '✗ 破功了'
```

## Lint 規則

| 代號 | 在講什麼 |
| --- | --- |
| L001 | 邏輯層元素在某環境沒有任何實現 |
| L002 | 某條 Relationship 在某環境走不通（可達性斷裂） |
| L003 | 連線指向不存在的 Instance、設備或 Endpoint |
| L004 | 萬用字元的實際數量與 `expect` 不符 |
| L005 | 萬用字元沒有註明 `expect` |
| L006 | Endpoint 缺少實際位址 |
| L007 | 連線沒有填用途 |
| L008 | 某 Instance 沒有被任何連線碰到 |
| L012 | 模型元素指向一個不存在的模型元素（刪除的安全網） |
| L013 | 契約的**目標端**是人（人沒有接點、不會被部署） |
| L014 | 同一環境裡兩個不同種類的東西撞名（警告；會讓 Agent 用名字指錯） |

L009–L011 是保留給 draw.io 那三條的代號，還沒實作。
規則的定義與理由都寫在 `crates/loom-core/src/lint.rs` 的 doc comment 裡。

> `mise run check:rules` 會擋下「程式碼提到一個還沒實作的代號」——
> 使用者照著代號去查文件，查到一條無關的規則是最糟的狀態。

## 給 AI Agent 用

App 裡的「AI 助手…」會開一個 MCP 端點（埠與 token 每次啟動都換），
搭配 `skills/diagram-loom/` 這個 skill，就能讓 Agent 幫你建模型。

這是**另一個前端，不是另一個程式**：桌面 App 給人用、MCP 給 Agent 用，
兩者共用同一份 `loom-core`。所以 Agent 建東西一樣要過 slug 唯一、參照完整那些檢查。

Agent 操作的就是**使用者當下開著的那份專案**：改的東西按 ⌘Z 退得掉、
畫面即時更新、而且**不自動存檔**——由人決定要不要留。

八支工具，形狀刻意收斂成三種：

| | 工具 | 形狀 |
| --- | --- | --- |
| **讀** | `projects`、`describe`、`lint` | `{scope, select, detail}` |
| **寫** | `create`、`create_nodes`、`add_connection`、`update` | `{scope, items, dry_run}` |
| **刪** | `delete` | `{scope, targets, cascade, confirm}` |

指名既有的東西時，`target` 一律吃**名字**（`redis-01`、`apache-* -> f5-01`），不吃 id。

`lint` 是這裡最重要的一支：Agent 從一段文字建模型一定會漏東西，
`lint` 讓它**自己發現自己漏了什麼**，然後補。

## 開發慣例

### 命名：識別字用英文，註解與畫面文字用中文

| | |
| --- | --- |
| **英文** | 變數、函式、型別、模組、欄位、**測試函式名** |
| **中文** | 註解、doc comment、錯誤訊息、UI 文字、commit message |

理由是**他國的開發人員要看得懂**。測試函式名正是他理解這個專案在乎什麼的
第一手材料。註解不同：那是說明，讀不懂可以翻譯，而且用母語寫得比較準。

> 混一半比全中文更糟。新增程式碼時直接用英文，不要留下「之後再改」。

### 測試四層，快的常跑、慢的少跑

| 層 | 測什麼 | 工具 | 速度 |
| --- | --- | --- | --- |
| 1. `loom-core` / `loom-mcp` | 領域模型、lint、YAML、Excel、Agent 工具 | `cargo test` | 秒 ⚡ |
| 2. 前端純邏輯 | `.drawio` 讀寫、對帳演算法、ELK 轉換、Vue 元件 | Vitest | 秒 ⚡ |
| 3. 瀏覽器整合 | 畫布與互動 | Playwright | 數十秒 |
| 4. 真的 App | 開窗、選單、檔案對話框 | WebdriverIO | 數分鐘 🐢 |

大部分測試放第 1、2 層——這正是那條「核心不准相依 `tauri`」的紀律換來的。
第 3、4 層（`tests/`）**還沒建立**。

**整合測試要比對「完整結果」，不是「有沒有包含某項」**——
對一個以找出缺漏為賣點的工具而言，誤報與漏報一樣糟。

## 文件

完整文件（領域模型、lint 規則的細節、`.drawio` 綁定與對帳、
為什麼換掉 draw.io、排版的已知限制、規模實測、MCP 工具介面、決策紀錄、roadmap）
放在**外層的 workspace repo** 的 `docs/`，不在這個 repo 裡。

程式碼本身的 doc comment 寫得很厚，通常比文件更貼近現況：

```sh
cargo doc -p loom-core -p loom-mcp --no-deps --document-private-items --open
```

## License

MIT（見 `Cargo.toml`）。
