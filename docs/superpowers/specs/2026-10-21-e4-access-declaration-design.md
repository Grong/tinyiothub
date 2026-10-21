# E4 接入层声明制子 spec（Manifest + ABI v2 + 空间/验收）

- 日期：2026-10-21
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-20-hub-runtime-architecture-design.md`（架构原则 P1-P6、E1-E5 阶段、CEO/eng 评审裁定）
- 状态：设计五节已与作者逐节确认（架构/契约/数据流/错误与验收/分期与迁移）；10 项关键裁定见 §0
- 范围：E4a-e 全部五个子阶段。E4 从原计划的“接入层改造”扩张为四块：声明制、ABI v2 强制中断、完整 maps/place/zone、AGH 式验收
- 执行次序：**方案 B（协议先行）**——先证明 manifest + ABI v2 + 参考驱动的“零 Hub 代码接入”，再让采样/资源/空间长在已声明的契约上

## 0. 关键裁定（作者已确认，2026-10-21）

| # | 主题 | 裁定 |
|---|---|---|
| 1 | 声明载体 | 独立 `driver.manifest.json`（版本化，可选 `.so`）；静态内置驱动使用同一格式 |
| 2 | 事实源 | manifest 为唯一能力事实源；DB 行是投影（UI/历史）；漂移=验收 fail/warn |
| 3 | 通用驱动 | **双层声明**：包级 manifest = 静态契约（config schema/tools 形状/resources/limits/placement 方式）；设备绑定时 `describe(config)` 产出实例声明（stateFields + 工具实例）→ `device_declarations` → 物化 |
| 4 | 采样 | 默认全采；仅 `demandOnly` source 按需求门控；`health:true` 慢通道常开 |
| 5 | 校验 | 业务参数（schema/range/enum）越界拒绝；运行参数仅**超时**钳制；超速率/超并发拒绝（不静默丢） |
| 6 | 验收 | AGH 式全套：多 profile + schema `gen:check` + test hub + `--interactive` + 五级结果 |
| 7 | 空间 | 完整 maps/place/zone；**maps 为 zone 权威，层级降级**；改写上位裁定 #3；E1b 层级派生为回退/迁移路径 |
| 8 | stop | 强制可中断：ABI v2 协作式 async + cancel token；1s 宽限后 `cancelled_unknown` |
| 9 | 验收驱动 | `drivers/demo-declarative/`（manifest + `.so`，覆盖全特性） |
| 10 | v1 处置 | 硬切拒载 + 迁移指引；仓内 3 个内置驱动同步迁 v2；升级方式=**维护窗口原子替换**（不做双 ABI 导出） |

## 1. 架构与组件边界

```
crates/core/src/driver/
  manifest.rs      Manifest v1 类型 + JSON Schema + 版本协商（纯契约）
  dynamic.rs       ABI v2：async read/execute + cancel + set_demand + describe(config)
  mod.rs           ThingDriver v2 trait（内置驱动，协作式 cancel 语义）
crates/runtime/src/
  access/           ← 新核心：Hub 接入层
    registry.rs       ManifestRegistry（workspace 内活跃包级清单，版本化）
    device.rs         DeviceDeclarationRegistry（实例声明；启动重织，幂等 upsert）
    validator.rs      业务参数拒绝 / 超时钳制 / 未声明工具拒绝
    materialize.rs    声明 → thing_properties / device_commands / 坐标投影（走 ports）
    demand.rs         DemandTracker（watch ∪ 派生 ∪ 告警 ∪ 订阅 ∪ 近期读取 TTL）
    sampler.rs        按 source 调度，替代统一的 100ms 全量轮询
    resources.rs      多资源 claim/lease/释放 + busy 持有者文案
    cancel.rs         CancelToken、1s 宽限、cancelled/unknown 终态
  maps/
    model.rs         Map / Place / Zone 数据结构与几何校验
    zone.rs          point-in-polygon + 最具体 zone 选择 + trust 派生
  acceptance/
    runner.rs        profile 执行与聚合
    profiles/        core / materialization / lifecycle / telemetry / tools /
                     streaming / resources / cancel / placement / migration / manual
    report.rs        五级结果与报告序列化
  driver/
    registry.rs      v2-only 加载；v1 拒载
    loader.rs        包安装（manifest + 可选 .so）与卸载
    dynamic_adapter.rs ABI v2 适配（回调→async、cancel 句柄）
    worker.rs        每设备 driver worker task（独占 &mut，接收 read/execute/demand/cancel）
crates/db/
  migrations/        driver_declarations / device_declarations / maps / places /
                     thing 坐标列 / acceptance_reports
  ports 实现
apps/cloud/          install/declare/acceptance API；AI 工具改走 access 层
drivers/demo-declarative/   参考驱动（manifest + .so）
```

### 边界规则

1. **brain 只经 Hub access 调设备**：`call_device / set_device / stop_device` 不再直连 `ThingService` / 命令队列；`access` 完成校验→钳制→资源 claim→派发→取消→落账。DataServer 退为执行引擎。
2. **每设备一个 driver worker**：worker 独占 `&mut` 驱动实例，经 mpsc 接收 read/execute/demand/cancel；避免 `parking_lot` 锁跨 await，也避免长调用卡住采样循环。
3. **双层事实源**：包级 manifest 是静态契约；实例声明由驱动 `describe(config)` 生成并持久化到 `device_declarations`（启动时重织进内存，幂等 upsert）。运行时校验/采样/仲裁读内存声明；`thing_properties/device_commands` 只是呈现与历史查询的投影。
4. **core 零 IO、runtime 零 sqlx**：新增持久化全走 `crates/runtime/src/ports.rs`（E1 F3 教训）；cloud 只做装配与 API。
5. **zone 权威切换**：`maps/zone.rs` 取代 `watch/live_state.rs` 的层级 zone 派生；后者保留为无 maps 时的回退，迁移完成后删除。
6. **执行模型统一**：内置驱动实现 v2 trait，动态驱动走 v2 ABI；不存在 v1 适配器。

## 2. 契约

### 2.1 `driver.manifest.json`（v1）

```json
{
  "manifestVersion": "1.0",
  "identity": {
    "driver": "demo-declarative", "version": "1.0.0",
    "protocol": "demo", "displayName": "Demo Declarative Driver", "abiVersion": 2,
    "dynamicCapabilities": false
  },
  "config": {
    "type": "object",
    "properties": { "intervalMs": { "type": "integer", "default": 1000 } },
    "additionalProperties": false
  },
  "capabilities": { "cancel": true, "push": false, "streaming": false },
  "tools": [{
    "name": "set_level",
    "kind": "action",
    "parameters": {
      "type": "object",
      "properties": { "level": { "type": "integer", "minimum": 0, "maximum": 100 } },
      "required": ["level"],
      "additionalProperties": false
    },
    "timeout": { "defaultMs": 3000, "maxMs": 5000 },
    "safety": "control",
    "resources": ["actuator"],
    "cancellable": true
  }],
  "stateFields": [{
    "name": "level", "type": "integer", "unit": "%",
    "range": { "min": 0, "max": 100 },
    "readOnly": false, "required": true, "source": "telemetry"
  }],
  "sources": [{
    "id": "telemetry", "kind": "poll",
    "fields": ["level"], "intervalMs": 1000, "minIntervalMs": 200,
    "demandOnly": false, "health": true
  }],
  "resources": [{ "id": "actuator", "mode": "exclusive", "capacity": 1 }],
  "placement": {
    "mode": "runtime",
    "fromSource": "position",
    "mapRef": null, "zoneRef": null, "position": null, "trust": "trusted"
  },
  "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
}
```

字段语义：

- `identity.dynamicCapabilities`：`false` = 固定能力，实例声明=包级清单；`true` = 通用驱动（modbus/snmp/opcua…），绑定时必须 `describe(config)` 返回实例声明，Hub 对实例声明做结构校验（工具/字段 schema 元校验 + `limits` 上限），不要求是包级 tools 的子集。
- `config`：驱动参数 JSON Schema；安装时校验已存在物的 `driver_options`（warn），建物/绑定驱动时校验（拒绝）。
- `tools[].kind`：`action`（自由命令）| `set`（写某个 `readOnly=false` 的 state field）。`set` 工具的 `parameters` 必须恰好含目标字段名，且类型/范围与 state field 一致，否则安装校验拒绝。
- `tools[].safety`：`safe | control | dangerous`；验收只自动调用 `safe`，`dangerous` 记 manual。
- `tools[].timeout`：`defaultMs` 用于 AI 未指定时；`maxMs` 是钳制上限。
- `tools[].resources`：调用期间 claim 的资源 id。
- `sources[].kind`：`poll | push | stream | position | x_<custom>`；未知 kind 透传不拒（MHS MSG-6），`x_` 为自定义逃生舱。
- `sources[].demandOnly`：仅该 source 受需求门控；默认 `false`（保守全采）。
- `sources[].health`：常开慢通道，忽略 `demandOnly`。
- `placement.mode`：`fixed`（位置来自声明）| `runtime`（驱动 `fromSource` 上报或人工 API 校正）| `declared`（建物时指定）。
- `placement.fromSource`：`mode=runtime` 时可指定；该 source 的字段约定为 `x/y/level/heading/trust`。
- `limits`：`maxConcurrentJobs` 与 `maxCallsPerMin`；实例声明不得超过包级上限。
- 交叉引用校验（安装/绑定/验收共用）：`sources[].fields ⊆ stateFields[].name`；`tools[].resources ⊆ resources[].id`；`placement.mapRef/zoneRef` 为 workspace 内名称，缺失时安装可过、绑定/验收 fail。
- 未知字段一律忽略；`additionalProperties: false` 仅约束工具参数，不约束未来新增的顶层字段。

### 2.2 版本协商（P5）

- `manifestVersion` = `major.minor`；Hub 接受 major=1、minor≤当前值。
- major 不匹配 → 拒绝安装并给出原因；minor 超界 → 接受，未知字段忽略。
- `identity.abiVersion` 必须 = 2；`.so` 的 `describe()` 返回 identity，与 manifest 交叉校验（防清单/二进制错配）。
- 清单与二进制哈希落 `driver_declarations`，验收检查哈希一致。
- 单一 JSON Schema `spec/driver-manifest.schema.json` 由 Rust 类型经 `schemars` 生成，CI `gen:check` 防漂移。

### 2.3 ABI v2（C 侧）

```c
typedef struct { int (*is_cancelled)(void* ctx); void* ctx; } CancelHandle;
// status: 0 ok, 1 error, 2 cancelled
typedef void (*DriverDone)(void* user_data, int status, char* out_json);

typedef struct {
  uint32_t version;                       // 2
  int (*read_async)(void* ctx, const char* config_json, const char* sources_json,
                    CancelHandle* cancel, DriverDone done, void* user_data);
  int (*execute_async)(void* ctx, const char* config_json, const char* cmd_json,
                       CancelHandle* cancel, DriverDone done, void* user_data);
  int (*set_demand)(void* ctx, const char* demand_json);
  int (*describe)(void* ctx, const char* config_json, char** out_json);
  void (*free_string)(char* s);
} DriverVTableV2;
```

- `read_async` 的 `sources_json` 是本次要采的 source id 列表；驱动可按 source 过滤字段。
- `describe(config_json)` 返回 `{identity, stateFields, tools}`；固定能力驱动可省略实例字段（Hub 用包级清单）。调用超时 2s、输出上限 256 KiB；超限/超时→绑定失败。
- 回调可同步或异步触发；`out_json` 由驱动分配、Hub 用 `free_string` 释放。
- **stop 流程**：置 cancel → 等 ≤1s → 收到 `status=2` 记 `cancelled`；超时记 `cancelled_unknown` 并落账。迟到回调被消费丢弃；驱动 ctx 在在途调用清零前不卸载。
- `set_demand` 用于 push/stream：Hub 下发活跃 source 集合，驱动起停订阅；同步返回 0/错误。
- 内置驱动实现等价的 Rust trait（`read/execute(&CancelToken)` + `set_demand`），由设备 worker 独占调用；trait 不提供“忽略 cancel”的默认实现。
- v1 `.so` 加载直接失败，错误文案含迁移指引（async + cancel + describe）。

## 3. 数据流

### 3.1 安装与物化

```
POST /workspaces/{ws}/drivers/install  (manifest + 可选 .so)
  → 文件名/版本/ABI=2/manifest schema 校验
  → describe() 与 manifest.identity 交叉校验；manifest+binary 哈希
  → 落盘 data/drivers/workspaces/{ws}/{name}/{version}/
  → driver_declarations 写新行 (active)，旧版本置 retired
  → ManifestRegistry upsert；v1 .so 在此拒载
  → 已绑定物：重新 describe(config) → device_declarations upsert → 重物化
      stateFields→thing_properties、tools→device_commands、placement→坐标列
      只 upsert，不删除（消失项标 stale，交给验收报 drift）

建物/绑定驱动
  → 必须有 active manifest（无清单 → 拒绝，fail closed）
  → driver_options 按 manifest.config 校验（非法拒绝）
  → dynamicCapabilities=true → describe(config) → 实例声明校验 → device_declarations
  → 物化投影；模板的 properties/actions **不再拷贝**（清单是事实源），
    模板只提供 display_name/category 等展示元数据

启动时
  → device_declarations 全量重织 DeviceDeclarationRegistry（幂等 upsert）
  → 驱动升级后对绑定物重新 describe(config)；失败则保留旧声明并标 stale
```

### 3.2 调用与取消

```
AI: call_device / set_device / stop_device
  → HubAccess::invoke(ws, thing, tool, params, caller, deadline, cancel)
     1 ws/thing/active 包级+实例声明（缺失 fail closed）
     2 工具未声明 → reject "action not declared"
     3 JSON Schema 不符 → reject（带字段路径+原因）
     4 业务参数越界（range/enum）→ reject，返回声明范围
     5 超时钳制：timeout ∈ [defaultMs, maxMs]（标注 clamped）
       超速率 → E_RATE_LIMITED；超并发 → busy（带持有者）——不静默丢弃
     6 resources claim（工具的 resources，lease 复用 E1 DEFAULT_LEASE_TTL）→ 失败返回 busy "held by <holder>"
     7 派发给该设备 worker；账本 call_started
     8 结果回来 → 账本 call_result → 释放 claim
stop_device（任何人可调，永远可用）
  → 置该设备所有在途调用的 cancel（不区分持有者）
  → 释放该设备全部 claim；≤1s 收到 status=2 → cancelled
  → 超时 → cancelled_unknown（账本+指标），迟到回调丢弃，ctx 不卸载
```

### 3.3 采样

```
demand(source) = 活跃 watch 引用 ∪ 派生属性引用 ∪ 告警规则引用
              ∪ 显式 Hub 订阅 ∪ 近期读取 TTL(60s)
调度：!demandOnly → 按 interval 采样；demandOnly → demand 非空才采样
      health:true 的 source 常开慢通道（≥minInterval）
push/stream → set_demand(sources) 起停订阅；需求消失后 30s 宽限再停
读数 → apply_property_values → cache_and_publish → EventBus → LiveState → watch 求值
```

- 近期读取 = 任一经 Hub access 的 `read_device` / `read_property` / 快照装配对该设备的读取，计入 60s TTL。
- 显式 Hub 订阅 = E3 投影/未来面板通过 Hub API 声明的 source 订阅，带 TTL。
- 派生属性引用从表达式引擎的 refs 读取；告警规则引用从告警规则表读取；两者变化时增量更新 DemandTracker。
- `position` source 默认不参与 demandOnly 门控（位置是健康/空间摘要的输入）。

### 3.4 位置与 zone

```
位置来源：source kind=position（字段 x:float/y:float 米、level:int、heading:float 度、trust:enum）
          或 PUT /things/{id}/position（人工校正）
trust：fixed → trusted；runtime → age ≤30s trusted / ≤5min uncertain / >5min lost（workspace 可配）
zone：显式 placement.zoneRef 优先；否则 thing.map_id（缺省=workspace 默认图）
      → 在所有 zone 多边形中 point-in-polygon
      → 命中多个取面积最小（最具体）；无命中 → None
LiveState 摘要读 maps 的 zone 结果；per-workspace flag `zone.source=hierarchy|maps`
默认 hierarchy（存量）；新工作区默认 maps；切换在验收 placement 通过后进行
仅当 =hierarchy 或 maps 为空时回退 parent_id 层级（迁移期，E5 前删除）
```

### 3.5 数据模型与 API（新增，additive）

| 表 | 关键列 |
|---|---|
| `driver_declarations` | id, workspace_id, driver_name, driver_version, manifest_version, manifest_json, manifest_hash, binary_hash, status(active/retired), installed_at |
| `device_declarations` | id, workspace_id, thing_id, driver_name, driver_version, declaration_json, declaration_hash, status(active/stale), updated_at；UNIQUE(thing_id, driver_name) |
| `maps` | id, workspace_id, name, bounds_json{xMin,yMin,xMax,yMax}, anchor_map_id, is_default, created_at |
| `places` | id, map_id, name, kind(landmark/zone), geometry_json(point|polygon≥3), heading, created_at |
| `acceptance_reports` | id, workspace_id, driver_name, driver_version, profile, level, report_json, created_at |
| `things`（增加列） | map_id, position_x, position_y, position_level, position_heading, position_trust, position_updated_at；原 `position TEXT` 保留不动 |

API（workspace 隔离）：

- `POST /workspaces/{ws}/drivers/install`、`GET /workspaces/{ws}/drivers`、`POST /workspaces/{ws}/drivers/{name}/acceptance`
- `GET/POST/PATCH/DELETE /workspaces/{ws}/maps`、`/workspaces/{ws}/maps/{id}/places`
- `PUT /things/{id}/position`（人工校正；body 含 x/y/level/heading/trust）
- `GET /workspaces/{ws}/things/{id}/declaration`（实例声明与 drift）

## 4. 错误处理、验收与测试

### 4.1 错误与救援

| 失败 | 处理 | 用户可见 |
|---|---|---|
| manifest schema/版本非法 | 安装拒绝 + 原因，注册表不变 | 是（API 400） |
| `describe()` 与 identity 不一致 | 安装/绑定拒绝（清单/二进制错配） | 是 |
| `describe(config)` 超时/超 256KiB/实例声明非法 | 绑定拒绝 + 原因（已有物保留旧声明） | 是 |
| v1 `.so` | 拒载 + 迁移指引（async/cancel/describe） | 是（安装错误） |
| 无 active manifest 的物被调用 | fail closed，拒绝 | 是 |
| 参数 schema/业务范围不符 | 拒绝，带字段路径与声明范围 | 是 |
| timeout 超声明 | 钳到 `maxMs` 并标注 clamped | 是（结果字段） |
| 超速率/超并发 | `E_RATE_LIMITED` / busy（带持有者） | 是 |
| stop 后 1s 未返回 | `cancelled_unknown` + 账本/指标；ctx 保留，迟到回调丢弃 | 是（状态） |
| 物化失败 | 不回滚安装；该物保留旧投影，验收 fail + 账本 | 是（验收报告） |
| 投影/实例声明漂移 | 验收 fail/warn，不自动回写清单 | 是 |
| 位置越界/多边形非法 | API 拒绝；驱动上报越界 → zone=None + warn | 是（API）/指标 |
| 需求跟踪失效 | demandOnly 源可能少采；health 慢通道兜底；指标 `demand_miss` | 指标 |
| 验收 runner 异常 | 报告记 fail（含已跑完的项） | 是 |

### 4.2 验收 profile（AGH 式，按设备类型裁剪）

| profile | 检查 |
|---|---|
| `core` | manifest schema、identity、工具 schema 元校验、state field 类型/范围、resources、config schema |
| `materialization` | 装包+建测试物 → `thing_properties/device_commands/坐标` 与声明逐项一致；无模板能力拷贝 |
| `lifecycle` | 连接/读数/断开/重连；offline 状态与 health |
| `telemetry` | 读数覆盖 required 字段；类型/范围；interval 生效；push 模式送达 |
| `tools` | safe 工具参数往返（合法/非法）；未声明工具拒绝；越界拒绝理由正确 |
| `streaming` | demandOnly 源随需求起停；health 慢通道常开；30s 宽限 |
| `resources` | exclusive/shared/capacity；并发冲突 busy 带持有者；lease 到期释放 |
| `cancel` | 长工具 stop → ≤1s cancelled；不合作驱动 → cancelled_unknown；stop 后无后续调用 |
| `placement` | map/place 引用、点落区、多区取最小、trust 阈值、越界 |
| `migration` | v1 拒载文案；major 不匹配拒绝；未知字段忽略；版本协商 |
| `manual` | `--interactive` 人工项（继电器/指示灯等）；`--no-hardware` 记 **skip 不记 pass** |

- 五级：pass / fail / warn / skip / manual；报告落 `acceptance_reports` + 账本事件。
- 执行：集成测试内嵌 test hub（内存库 + demo-declarative）；对外 `POST /workspaces/{ws}/drivers/{name}/acceptance` 与新增 `driver-check` bin（`cargo run -p driver-check -- --package <dir> --profile all`）。
- schema 单一事实源：`schemars` 生成 + CI `gen:check` 防漂移。
- 验收报告必须能被机器消费：JSON 字段含 `profile/check/level/detail/evidence`。

### 4.3 测试金字塔

- **单测**：manifest 版本协商、校验/钳制表驱动、资源 claim、demand 并集、cancel 状态机、point-in-polygon、trust 阈值、zone 取最小。
- **集成**：物化（临时库）、采样调度（fake EventBus）、调用链路（mock 驱动）、maps 派生、验收 runner 跑 demo-declarative。
- **E2E**：装包 → 建物 → call/stop → 投影/账本；demandOnly 起停；zone 正确。
- **回归**：DataServer 轮询测试改写为 per-source 调度；`invoke_action` 测试改走 access 层；watch zone 用例切到 maps 权威 + 回退路径。
- **CI 门禁**：fmt/clippy/test + `gen:check` + demo-declarative 验收全绿。
- **LLM/eval**：本期无 prompt 变更，evals 不动。

### 4.4 观测

- 指标：安装/校验拒绝（按原因）、clamp 次数、busy 拒绝、cancel 延迟、`cancelled_unknown`、`demand_miss`、采样滞后、zone 未命中、验收结果。
- 账本（复用 E1 `hub_ledger`）：`driver_installed`、`declaration_materialized`、`call_started/result/rejected`、`call_cancelled/unknown`、`busy_rejected`、`acceptance_run`。

## 5. 分期、迁移与回写

### 5.1 分期（方案 B）与退出映射

| 子阶段 | 交付 | 对应验收 profile |
|---|---|---|
| **E4a 协议先行** | manifest v1 + ABI v2（含 cancel 通道；stop 接线在 E4c）+ 双层声明（`describe(config)`→`device_declarations`）+ 3 个内置驱动迁 v2 + demo-declarative + 安装/物化 API；v1 `.so` 拒载 | core / materialization / migration |
| **E4b 采样** | per-source 调度、DemandTracker、health 慢通道、push `set_demand` | telemetry / streaming |
| **E4c 资源与取消** | 多资源 claim/lease、busy 持有者、stop 取消在途（1s 宽限/unknown） | resources / cancel / lifecycle |
| **E4d 空间** | maps/places 数据模型 + API、坐标/trust、point-in-polygon、zone 权威切换（flag） | placement |
| **E4e 收口** | 全套 profile + test hub + `--interactive` + 五级报告 + 迁移计划文档 + 退出验证 | 全部 |

子阶段独立交付、独立回滚；E4b-e 均以 E4a 的契约与参考驱动为地基。

### 5.2 存量驱动迁移计划（E4e 交付，执行可另立阶段）

- **仓内内置（E4a 内完成）**：simulated / modbus / snmp → v2 trait + manifest；modbus/snmp 走双层声明（`driver_options` 点位 → `describe(config)` 实例声明）。
- **`drivers/*`（计划，执行另立）**：bacnet / modbus / mqtt / mqtt-bridge / onvif / opcua / snmp 纳入 workspace 编译门禁（清偿 TODOS P2），补 manifest，按使用度排序迁移：mqtt → modbus/bacnet/opcua/snmp → onvif → mqtt-bridge。
- **外部已部署 `.so`（计划）**：盘点 → 用 v2 SDK 重建 → 跑验收 → 与 Hub 在同一维护窗口原子替换。
- **数据迁移**：绑定物重新 `describe(config)` → `device_declarations` + 重物化；与旧 DB 行做 drift 报告，**不删除**现有行（消失项标 stale）。模板拷贝出的能力行在新声明下只做匹配更新，多余行保留但标 stale；场景包实例化对已声明驱动的物跳过能力拷贝（行为变更，需回写场景模板 spec）。
- **zone 切换**：maps 未填充前保持层级回退（`zone.source=hierarchy`）；填充并验收后切 `maps`，回退路径在迁移完成后删除。
- **升级方式（已裁定）**：维护窗口原子替换 hub + 驱动；遗漏的 v1 驱动在新 Hub 直接不可用。
  1. 冻结变更，盘点 `list_for_workspace` 中所有 v1 动态驱动；
  2. 用 v2 SDK 重建全部外部驱动，staging 跑验收；
  3. 停机 → 替换 hub 二进制与驱动包 → 启动；
  4. 验证设备在线、`core` 验收通过、无 v1 拒载残留；
  5. 回滚 = 停机 → 重装旧 hub 与旧驱动；`driver_declarations`/验收报告为 additive，可保留。

### 5.3 上位文档回写

1. §6 E4 行：扩写为“声明制 + ABI v2 强制中断 + 完整 maps/place/zone + AGH 式验收”，退出标准增加“v1 动态驱动拒载 + 验收全套通过”。
2. §8 裁定 #3：改写为 **maps 为 zone 权威、层级降级**（不再“不搬 maps 帧”）；E1b 层级派生标为回退/迁移路径。
3. §4 接入层：补“包级清单 + 实例声明双层事实源”“默认全采、demand_only 门控”“业务参数拒绝/超时钳制”的实际裁定。
4. §6 估算：E4 单独 human ~3-4 周 / CC ~4-6 天 [estimated]；E5 启动条件不变。
5. TODOS：关闭“stop 不中断在途”（E4c 交付）；更新 World 坐标依赖（数据模型 E4d 完成，UI 仍为 E3 结转）；新增“drivers/* 纳入 workspace”。

### 5.4 非目标（YAGNI）

- 远端/网关设备自注册（MHS over wire）、X5 云边两级
- Map/Place 编辑 UI、E3 World 新模型渲染（本期只交数据模型 + API）
- 包信任/哈希治理（E5；本期哈希仅做清单/二进制一致性）
- 运动安全/手动接管（裁定 #15 不采纳）
- v1 共存适配器、双 ABI 长期支持
- `drivers/*` 的实际迁移执行（本期交计划与模板）

### 5.5 主要风险

| 风险 | 缓解 |
|---|---|
| ABI v2 硬切 + 原子升级 | 维护窗口 + 驱动盘点清单 + 回滚演练 |
| `describe(config)` 新增攻击面 | 2s 超时/256KiB 上限、schema 校验、`migration` profile |
| 物化与旧 DB 行漂移 | 只 upsert 不删除；drift 报告；验收 fail/warn |
| 模板能力拷贝行为变更 | 已声明驱动的物跳过模板能力拷贝；场景模板 spec 回写 + 迁移 drift 报告 |
| zone 权威切换影响 E1b/E3 | per-workspace flag + 迁移报告 + 回退路径 |
| per-source 调度回归 | DataServer 轮询测试整组改写并保留行为清单 |
| 验收范围膨胀 | profile 按设备类型裁剪，不照搬 motion/manual |

### 5.6 退出标准（汇总）

`demo-declarative` 仅凭 manifest + `.so` 接入（Hub 零代码），全部自动 profile 通过；v1 `.so` 拒载且有指引；stop 在途 ≤1s cancelled（否则 unknown 留痕）；demandOnly 源随需求起停、health 常开；多资源 busy 带持有者；maps zone 生效且层级回退可切换；迁移计划覆盖内置、`drivers/*`、外部 `.so`。

## 6. 复用清单

- E1 Watch 注册表 —— 采样需求的 watch 来源
- E1 `hub_ledger` / ports —— 新账本事件与持久化
- E1 LiveState / EventBus / `cache_and_publish` —— 读数与摘要链路
- E2 固定设备工具集 —— 调用入口；`stop_device` 语义在其上扩展
- E3 SSE 投影订阅 —— 显式订阅需求的来源
- 现有动态驱动 registry/loader —— v2 包安装与拒载改造
- 派生表达式引擎 refs —— 采样需求与 zone 派生复用
- `schemars`（新依赖）—— manifest JSON Schema 生成与 `gen:check`
