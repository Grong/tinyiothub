# E4c 资源仲裁 + stop 在途取消子 spec

- 日期：2026-10-22
- 分支：ai-consolidation
- 上位文档：`docs/superpowers/specs/2026-10-21-e4-access-declaration-design.md`（§3.2 stop、§2.1 resources、§4 错误矩阵）
- 范围：E4c。E4d/e（maps、全套验收）不在本期
- 状态：设计三问已与作者确认（2026-10-22）

## 0. 关键裁定

| # | 主题 | 裁定 |
|---|---|---|
| 1 | 仲裁归属 | 扩展 agent `ArbitrationRegistry`：按 `(thing, resource)` 多资源 claim；工具从声明取 `resources`；未声明 resources 的变更类工具默认 claim 合成 `__device__` exclusive 资源（保留 E1 语义）；只读工具不 claim |
| 2 | 多资源原子性 | 资源 id 排序后 all-or-nothing；任一失败释放已获 claim；busy 文案带持有者 + 资源 id |
| 3 | stop | **立即返回** `cancellation_requested`：置闸门、释放全部 claim、设置当前调用 CancelToken；异步 outcome 落事件/账本；≤1s 退出=`cancelled`，超时=`cancelled_unknown` |
| 4 | composite | 每步读取该步工具的声明 resources，排序后 all-or-nothing claim；未声明则合成 `__device__`；步骤 busy 人读得懂 |
| 5 | 兼容 | 现有 `try_acquire(thing, holder, ttl)` 保留为单设备合成资源的便捷入口；E1 每设备互斥语义不变 |

## 1. 组件与边界

```
crates/agent/src/runtime/thing_agent/arbitration.rs
  ResourceSpec / ResourceMode / ResourceLease / Busy { holder, resource }
  ArbitrationRegistry::{try_acquire_resources, try_acquire (compat), set_stop, force_release_thing}
crates/runtime/src/driver/worker.rs
  CancelToken slot + in_flight flag + WorkerResult::Cancelled { within_grace }
  WorkerHandle::{cancel_current, in_flight}
crates/runtime/src/data_server.rs
  cancel_calls(thing_id) -> usize  // sets current tokens
  WorkerResult::Cancelled -> event + metric
apps/cloud/src/domains/agent/host/tools/thing/
  invoke_action.rs / device.rs SetDeviceTool / device.rs StopTool
apps/cloud/src/domains/agent/host/composite.rs
  per-step resource claim
crates/agent/.../arbitration.rs tests
```

边界规则：

1. **声明是资源规格来源**：`ManifestTool.resources` → `ResourceDecl`（package/instance declaration）→ `ResourceSpec`。工具在调用前解析并 claim；Hub 侧校验声明仍在 E4a/E4b。
2. **驱动只协作式响应取消**：worker 共享当前 `CancelToken`；ABI v2 已把 `CancelHandle` 传给驱动；内置驱动在 IO/循环点检查。stop 不强杀 FFI。
3. **stop 永远可用**：不要求持有者；置闸门后新 call/set 立即拒绝；已持有 claim 全部释放。
4. **composite 与工具同构**：每一步的 claim 使用该步工具的 resources；步骤失败/部分成功语义沿用 E2 框架。
5. **多租户**：claim key = `(workspace_id, thing_id, resource_id)`（现有注册表按 thing_id 全局；E4c 保持 thing_id 全局键，workspace 鉴权在工具层）。

## 2. 契约

### 2.1 资源声明与 claim

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceMode { Exclusive, Shared }

#[derive(Debug, Clone)]
pub struct ResourceSpec {
    pub id: String,
    pub mode: ResourceMode,
    pub capacity: u32, // shared 时有效；exclusive 恒 1
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("busy: {holder} holds {resource}")]
pub struct Busy {
    pub holder: String,
    pub resource: String,
}

pub struct ResourceLease { /* RAII，Drop 释放全部 claim（token 校验） */ }

impl ArbitrationRegistry {
    pub fn try_acquire_resources(
        &self,
        thing_id: &str,
        holder: String,
        specs: &[ResourceSpec],
        ttl: Duration,
    ) -> Result<ResourceLease, Busy>;

    pub fn try_acquire(&self, thing_id: &str, holder: String, ttl: Duration)
        -> Result<ResourceLease, Busy>; // 兼容：单 __device__ exclusive

    pub fn set_stop(&self, thing_id: &str, reason: String, ttl: Duration); // 释放该设备全部 claim
    pub fn stop_reason(&self, thing_id: &str) -> Option<String>;
}
```

- `specs` 为空时：调用方决定是否使用合成 `__device__`；`try_acquire_resources` 本身不注入，工具层注入。
- 排序 + all-or-nothing：按 `specs.id` 排序逐个 claim；失败释放已获 claim 并返回首个 Busy。
- shared：持有者集合大小 < capacity 即可加入；exclusive：单持有者。
- TTL 过期：过期持有者不计入 capacity；迟到 Drop 按 token 不误删。
- `set_stop` 与 claim 互斥：stop 后新 claim 直接返回 Busy{holder:"stop:<reason>", resource:"__device__"}（或专用 Stopped 错误；本期用 Busy 文案带 stop 原因）。

### 2.2 工具层 claim

- `InvokeActionTool`（call）与 `SetDeviceTool`（set）在确认门之后、下发之前：
  1. 从 `ValidatedCall.tool.resources` 得到资源 id 集；若为空 → `__device__` exclusive。
  2. 从当前 device declaration 的 `resources[]` 解析 mode/capacity；未知 id → 拒绝（fail-closed）。
  3. `try_acquire_resources(thing_id, holder, specs, DEFAULT_LEASE_TTL)`；Busy → 失败 ToolResult（带 holder + resource）。
  4. 调用完成/失败即 drop lease（RAII）。
- 只读工具（read_device/read_property/list 等）不 claim。
- `stop` 不 claim；只置闸门 + cancel。
- composite 每步同规则；步骤失败释放该步 lease。

### 2.3 stop 与取消

```rust
// runtime worker
impl WorkerHandle {
    /// 设置当前 in-flight 调用的 CancelToken；返回是否确有在途调用。
    pub fn cancel_current(&self) -> bool;
    pub fn in_flight(&self) -> bool;
}

pub enum WorkerResult {
    // ...
    Cancelled { thing_id: String, within_grace: bool, elapsed_ms: u64 },
}
```

- worker 在每次 read/execute 前把共享 `CancelToken` 槽设为新 token，调用结束清空；`cancel_current` 设置当前 token。
- stop 流程（`StopTool`）：
  1. `registry.set_stop(thing, reason, ttl)`（释放全部 claim）
  2. `data_server.cancel_calls(thing)` → 每个 worker `cancel_current()`
  3. 立即返回 `{status:"cancellation_requested", reason, until}`；不等待。
- worker 在调用返回后，若 token 已取消：按 `elapsed <= 1s` 发 `WorkerResult::Cancelled{within_grace}`；DataServer 发布 `CommandCancelled` 事件（metadata `outcome: "cancelled"|"cancelled_unknown"`）并落账本 `call_cancelled`/`call_cancelled_unknown`。
- 1s 宽限由 worker 的 elapsed 判定；驱动不配合（阻塞超过 1s）→ `within_grace=false`，迟到返回仍发一次 unknown（不再发 cancelled）。
- 内置驱动：simulated 在 tick/生成循环检查 token；modbus/snmp 在入口与每条命令前检查；ABI v2 驱动由 adapter 传 handle。
- stop 闸门期间 call/set 返回 `stopped: <reason>`（现有 `stop_reason`）。

### 2.4 事件与错误

| 场景 | 结果 |
|---|---|
| claim busy | 失败 ToolResult `busy: <holder> holds <resource>` |
| 未知资源 id | 拒绝 `unknown resource '<id>'` |
| stop 期间调用 | 失败 ToolResult `stopped: <reason>` |
| stop 立即响应 | `cancellation_requested`（工具成功返回，不代表已中止） |
| 在途 ≤1s 退出 | 异步 `CommandCancelled outcome=cancelled` + 账本 |
| 在途 >1s | 异步 `CommandCancelled outcome=cancelled_unknown` + 账本 + 指标 |
| composite 步骤 busy | 步骤失败，带 holder/resource；已获资源释放 |
| lease 持有者崩溃 | TTL 到期后可再 claim（现有语义） |

指标：`arbitration_busy_total`、`stop_requests_total`、`stop_cancelled_total`、`stop_cancelled_unknown_total`、`stop_latency_ms`。

### 2.5 验收 profiles（E4c 子集）

- `resources`：exclusive/shared/capacity、多资源 all-or-nothing、busy 带持有者、TTL 到期释放、默认 `__device__`。
- `cancel`：长任务 stop 立即返回；≤1s 驱动收到取消并退出 → cancelled；不合作驱动 → cancelled_unknown 留痕。

## 3. 退出标准

- 两 run 并发操作同设备：后者收到带持有者的 busy；不同资源可并行（shared/capacity 正确）。
- stop 立即返回 `cancellation_requested`；协作驱动 ≤1s 退出并产生 `cancelled` 事件/账本；不合作驱动产生 `cancelled_unknown`。
- stop 后新 call/set 被 `stopped:` 拒绝；claim 已全部释放。
- composite 步骤按声明 resources claim，busy 文案可读。
- 默认无 resources 的变更工具仍保持 E1 每设备互斥。
- E4b Hub stream/E3 live stream/现有 watch/derived 不回归。

## 4. 非目标

- maps/coords（E4d）、全套 acceptance profile（E4e）、edge/X5。
- 人接管/运动安全（裁定 #15 不采纳）。
- 跨进程/分布式资源仲裁。
- 资源预占/排队（本期立即 busy 拒绝，不排队）。
