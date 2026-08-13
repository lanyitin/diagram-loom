/**
 * 給畫面用的型別別名。
 *
 * # 為什麼需要這一層
 *
 * 有些欄位在 Rust 標了 `skip_serializing_if`（空陣列不寫進 YAML），
 * specta 因此把它們拆成 `X_Serialize`（Rust 送出來的樣子，欄位可能不存在）
 * 與 `X_Deserialize`（Rust 收得下的樣子，欄位可省略）兩種。
 *
 * 前端**永遠只是接收方**，所以一律用 `_Serialize` 那一支。
 * 直接用聯集型別的話，每次存取 `env.connections` 都要先收窄，很吵。
 *
 * 這個檔案只做改名，不加欄位也不改語意——真正的型別在 `bindings.ts`，
 * 那份是 `mise run bindings` 產的。
 */

export type {
  Cell,
  Change,
  ChangeKind,
  Element,
  FindingView as Finding,
  Id,
  Matrix,
  Plan,
  Relationship,
  Row,
  Rule,
  Severity,
  Side,
  SideKind,
  Status,
} from './bindings'

export type {
  Connection_Serialize as Connection,
  ContainerInstance_Serialize as ContainerInstance,
  Container_Serialize as Container,
  DeploymentNode_Serialize as DeploymentNode,
  Endpointing_Serialize as Endpointing,
  Endpoint_Serialize as Endpoint,
  Environment_Serialize as Environment,
  InfrastructureNode_Serialize as InfrastructureNode,
  InstanceRef_Serialize as InstanceRef,
  Logical_Serialize as Logical,
  Project_Serialize as Project,
  Snapshot_Serialize as Snapshot,
  SoftwareSystemInstance_Serialize as SoftwareSystemInstance,
  SoftwareSystem_Serialize as SoftwareSystem,
} from './bindings'
