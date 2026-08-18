# Agent Crate 抽取实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 `apps/cloud/src/domains/agent/` 的共性 agent 能力抽取为独立 `crates/agent`（lib `tinyiothub_agent`），buzz 式内存+事件持久化，HTTP handler 与数据实现留 cloud。

**Architecture:** 先在 cloud 原地完成事件化改造（行为变更，Phase 1），再物理搬迁（纯移动，Phase 2-5）。crate 状态全在内存，`AgentEvent` broadcast 出口，cloud subscriber 幂等投影落库；启动时 `RestoreSnapshot` 重建。工具框架进 crate，数据实现（thing 工具/chat history/config service）留 cloud 经 `ToolRegistry` 注册。

**Tech Stack:** Rust 2024, tokio (broadcast/mpsc), sqlx(sqlite), axum（仅 cloud 侧）, zeroclaw v0.8.1-patched

**Spec:** `docs/superpowers/specs/2026-08-15-agent-crate-extraction-design.md`（D2-D13 全部裁决已并入）

## Global Constraints

- crate 内**零** `axum` / `sqlx` / `tinyiothub_storage` / `crate::domains`（源码 grep + `cargo tree` 双重验收）
- **无 re-export shim**（AGENTS.md:66 硬规则）——类型迁移后消费方直改 import
- **DB schema 不变**——事件 fencing 用 `occurred_at` 时间戳（不是 version 列），见 Task 10 注
- 单实例假设：cloud 恰好一个进程持有内存真相源
- 行为变更全部集中在 Phase 1（cloud 原地）；Phase 2-5 只移动不改语义
- 测试命令：`cargo test -p tinyiothub-cloud` / `cargo test -p tinyiothub-agent`（Phase 2 起）
- 每个 Task 结束独立 commit；commit message 前缀 `refactor(agent):`
- 现有集成测试（`agent_tasks_api_tests` 等）全程必须保持绿

**测试 fixtures 约定**：计划测试代码中的 `fixtures::*` 指复用/扩展 `apps/cloud/src/domains/agent/host/test_utils.rs` 的现有夹具（它随代码迁入 crate）。其中 `fixtures::report(ws, run_id)`、`fixtures::report_with_dedup(ws, key)`、`fixtures::heartbeat_task(ws, text)` 为薄构造函数（按 Task 1 的 core 类型逐字段填合理默认值）；`fixtures::db*()` 复用 `crates/db` 的 `test_pool()` 模式；`fixtures::wait_until(cond)` 为 50ms 轮询、5s 超时的异步等待助手（Task 8 首次使用时在 `host/test_utils.rs` 定义）。`RuntimeDeps::test_stub()` 在 Task 3 实现时定义：用 `NoopPolicyEngine`、内存 `RunRegistry`、容量 16 的 `AgentEventBus` 等无 I/O 桩件组装。

## 关键实测事实（实现前必读）

- loop_ 生产代码存储耦合**不是 36 处内联 SQL**，而是两类：
  1. **类型 re-export**：`loop_/mod.rs:36`、`loop_/agent/pool.rs:8`、`loop_/heartbeat/{types,report,loop_,runner}.rs`、`loop_/thing_agent/types.rs:2` 从 `tinyiothub_storage::{heartbeat,agent_runs}` re-export 领域类型
  2. **两个 repo 依赖**：`AgentRunsRepository`（manager/pushback/orchestrator 用）+ `HeartbeatTaskRepository`（runner/loop_/orchestrator 用，经 `loop_/heartbeat/repo.rs:2` re-export）
- `HeartbeatTaskRepository` 方法签名（`crates/db/src/heartbeat.rs`）：`list_by_workspace(ws) -> Vec<HeartbeatTask>`、`load_trust_config(ws)`、`save_trust_config(ws, &TrustConfig)`、`load_heartbeat_config(ws)`、`save_heartbeat_config(...)`、`insert_result(ws, &HeartbeatResult)`、`replace_all(ws, &[NewHeartbeatTask])` 等
- `AgentRunsRepository.insert_run(&RunReport, problem_key: Option<&str>, dedup_key: Option<&str>)`（`crates/db/src/agent_runs.rs:106`）
- `AgentPool`（`host/agent/pool.rs`）持有：`db_pool: SqlitePool`（:84，用于 :222 `config_service::get_config` 和 :244）、`memory_store: Arc<MemoryStore>`（:91）、`memory_service: RwLock<Option<Arc<MemoryService>>>`、`trust_configs: DashMap<String, TrustConfig>`、`event_publisher`、`runtime: ToolRuntimeContext`
- 依赖链实测：`tinyiothub_llm` 干净；`tinyiothub_skills` → db（仅 `trust.rs` 用 `TrustConfig/TrustLevel` 类型）；`tinyiothub_policy` → db+skills（仅类型：`TrustConfig/TrustLevel/AutonomyMode/AutonomyPolicy/Proposal`）；`crates/runtime` 干净
- `HeartbeatResult.proposals` 字段类型是 db 的 `crate::policy::Proposal`（`crates/db/src/policy.rs:261`）——所以 Proposal 也必须随类型归位
- Orchestrator callbacks（`orchestrator/callbacks.rs`）还持有 `MemoryService`（memory 引擎）和 `HeartbeatBridge.runs_repo`（problem_key dedup 用）
- pushback 的 dedup 判定语义：**实现时必须先读 `pushback.rs:115-130` 的 `policy_relax_hint` 与 dedup 查询，确认 dedup 是否时间窗有界**。若有界 → RunRegistry 窗口可服务；若无界 → dedup 判定留在 cloud 侧（指令注入前检查），在 Task 5 中记录结论
- `ThingAgentManager` 生产代码只有 1 处存储写（`manager.rs:321` 的 `insert_run`）；scheduler 的 SQL 命中全是测试/原子操作误报
- `core` crate 已有 `sqlx` optional feature（`crates/core/Cargo.toml:22,26`），类型归位后 db 启用之、agent 不启用
- 既有 CI 守卫：`.github/workflows/ci.yml:129-147`（thing/tenant 边守卫 + G9 loop 纯度守卫），路径在搬迁后必须重写
- `service_manager.rs:151,235-236` 接线 `LoggingDropNotifier + SqliteDeadLetterQueue`——AiEvent 系统总线保留不动（D12），此接线点随迁

---

# Phase 1 — cloud 原地事件化（唯一的行为变更阶段）

## Task 1: 领域类型归位 crates/core（斩断 policy/skills→db 类型边）

**Files:**
- Create: `crates/core/src/heartbeat.rs`、`crates/core/src/agent_runs.rs`、`crates/core/src/policy.rs`
- Modify: `crates/core/src/lib.rs`（注册三个模块）、`crates/db/src/heartbeat.rs`、`crates/db/src/agent_runs.rs`、`crates/db/src/policy.rs`（删类型定义，改为 `pub use tinyiothub_core::*` —— 注意：db 内部的 re-export 是 db 对自己模块的组织，允许；禁止的是跨 crate 摆渡层）
- Modify: `crates/policy/Cargo.toml`（删 `db = { workspace = true }`，加 `core = { workspace = true }`）、`crates/policy/src/{proposal,autonomy,adapters}.rs`
- Modify: `crates/skills/Cargo.toml`（删 db 加 core）、`crates/skills/src/trust.rs`
- Modify: 所有消费方 import（用 grep 找全）

**Interfaces:**
- Consumes: 现有类型定义（见下）
- Produces: `tinyiothub_core::heartbeat::{TrustConfig, TrustLevel, HeartbeatTask, NewHeartbeatTask, HeartbeatStatus, ExecutedAction, HeartbeatResult, MIN_HEARTBEAT_INTERVAL_MINUTES}`、`tinyiothub_core::agent_runs::{ActionRecord, ActionResult, Outcome, RunReport, format_summary}`、`tinyiothub_core::policy::{Proposal, ProposalStatus, AutonomyMode, AutonomyPolicy}`

**移动的类型清单**（从 `crates/db/src/heartbeat.rs:27-110` 区域、`crates/db/src/agent_runs.rs:67-83` 区域、`crates/db/src/policy.rs:261-290` 区域逐字剪切）：
- heartbeat: `TrustConfig`(含 Default impl)、`TrustLevel`、`HeartbeatTask`、`NewHeartbeatTask`、`HeartbeatStatus`、`ExecutedAction`、`HeartbeatResult`、`MIN_HEARTBEAT_INTERVAL_MINUTES` 常量
- agent_runs: `ActionRecord`、`ActionResult`、`Outcome`（含 `from_db`/`as_str`）、`RunReport`、`format_summary`
- policy: `Proposal`、`ProposalStatus`（含 Display）、`AutonomyMode`、`AutonomyPolicy`
- **留在 db**：`PolicyRepository`、`HeartbeatTaskRepository`、`AgentRunsRepository`（查询契约+SQL）、所有 `sqlx::query*` 实现
- `HeartbeatResult.proposals` 字段类型从 `crate::policy::Proposal` 改为 `tinyiothub_core::policy::Proposal`

- [ ] **Step 1: 在 core 建三个模块，逐字移入类型定义**

`crates/core/src/heartbeat.rs` 以 `crates/db/src/heartbeat.rs` 现有定义为准逐字剪切（derive 保持原样；若类型带 `sqlx::FromRow` derive，改为 `#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]`）。`agent_runs.rs`、`policy.rs` 同理。`crates/core/src/lib.rs` 加：

```rust
pub mod agent_runs;
pub mod heartbeat;
pub mod policy;
```

- [ ] **Step 2: db 侧改为从 core 导入**

`crates/db/src/heartbeat.rs` 顶部删类型定义，改为 `pub use tinyiothub_core::heartbeat::*;`（db 内部模块组织，非跨 crate 摆渡）。确认 `crates/db/Cargo.toml` 的 core 依赖启用 `sqlx` feature。`agent_runs.rs`、`policy.rs` 同理（`PolicyRepository` 等 repo struct 留在原地不动）。

- [ ] **Step 3: 编译 db 与 core**

Run: `cargo check -p tinyiothub-core -p tinyiothub-storage`
Expected: PASS（若报 FromRow/Type derive 缺失，回 Step 1 补 cfg_attr）

- [ ] **Step 4: 斩断 policy/skills 的 db 依赖**

`crates/policy/src/proposal.rs` 改为 `pub use tinyiothub_core::policy::{Proposal, ProposalStatus};`；`autonomy.rs:9` 删 `PolicyRepository` 再导出（消费方直改 import 到 `tinyiothub_storage::policy::PolicyRepository`——D5 规则），`AutonomyMode/AutonomyPolicy` 改从 core 导入；`adapters.rs:18,238` 的 `tinyiothub_storage::heartbeat::` 改 `tinyiothub_core::heartbeat::`。`crates/skills/src/trust.rs:8` 同理。两个 Cargo.toml 删 `db` 加 `core`。

- [ ] **Step 5: 全 workspace 消费方 import 重写**

```bash
grep -rln "tinyiothub_storage::heartbeat::\|tinyiothub_storage::agent_runs::\|tinyiothub_storage::policy::{AutonomyMode\|tinyiothub_storage::policy::{Proposal" apps/ crates/ --include="*.rs"
```
逐文件把**类型**导入改到 `tinyiothub_core::*`（repo 类型保持 `tinyiothub_storage` 不变）。

- [ ] **Step 6: 全量验证 + commit**

Run: `cargo check --workspace && cargo test -p tinyiothub-storage -p tinyiothub-policy -p tinyiothub-skills`
Expected: 全绿。然后：
```bash
git add -A && git commit -m "refactor(agent): re-home agent domain value types to crates/core (Task 1)"
```

## Task 2: AgentEvent 契约 + EventBus（loop_/events.rs）

**Files:**
- Create: `apps/cloud/src/domains/agent/loop_/events.rs`（Phase 4 搬入 `crates/agent/src/events.rs`）
- Test: 同文件 `#[cfg(test)]` mod
- Modify: `apps/cloud/src/domains/agent/loop_/mod.rs`（`pub mod events;`）

**Interfaces:**
- Consumes: Task 1 的 core 类型
- Produces（后续 Task 依赖这些确切名字）:

```rust
pub struct AgentEvent {
    pub seq: u64,                    // 进程内单调序号（调试/丢事件检测）
    pub occurred_at: DateTime<Utc>,  // DB fencing 用（DB schema 不变，不用 version 列）
    pub kind: AgentEventKind,
}

pub enum AgentEventKind {
    /// thing_agent run 完成记录（含完整 RunReport，幂等 insert-or-ignore by run_id）
    RunRecorded { report: Box<RunReport>, problem_key: Option<String>, dedup_key: Option<String> },
    /// 心跳 tick 结果（orchestrator 的 insert_result 替代）
    HeartbeatResultReady { result: Box<HeartbeatResult> },
    /// trust config 变更（状态跃迁，非每 tick）
    TrustConfigChanged { workspace_id: String, config: Box<TrustConfig> },
    /// 心跳任务列表变更（CRUD 后全量替换语义）
    HeartbeatTasksChanged { workspace_id: String },
    /// DLQ 条目（cloud subscriber 写 dlq 表）
    DlqEntryAdded { entry: Box<DeadLetterEntry> },
}

pub struct AgentEventBus { /* broadcast::Sender<AgentEvent> + AtomicU64 seq */ }
impl AgentEventBus {
    pub fn new(capacity: usize) -> Self;           // 生产用 4096
    pub fn emit(&self, kind: AgentEventKind);      // seq/occurred_at 在此盖章；满则最老消息被覆盖（subscriber 靠 Lagged 检测）
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent>;
}
```

- [ ] **Step 1: 写失败测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_stamps_monotonic_seq_and_delivers_to_subscriber() {
        let bus = AgentEventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: "ws1".into() });
        bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: "ws2".into() });
        let e1 = rx.recv().await.unwrap();
        let e2 = rx.recv().await.unwrap();
        assert!(e1.seq < e2.seq);
        assert!(e1.occurred_at <= e2.occurred_at);
    }

    #[tokio::test]
    async fn slow_subscriber_observes_lagged() {
        let bus = AgentEventBus::new(2);
        let mut rx = bus.subscribe();
        for i in 0..5 {
            bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: format!("ws{i}") });
        }
        assert!(matches!(rx.recv().await, Err(broadcast::error::RecvError::Lagged(_))));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p tinyiothub-cloud loop_::events` — Expected: 编译失败（模块不存在）

- [ ] **Step 3: 实现 events.rs**

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatResult, TrustConfig};
use super::event::dlq::DeadLetterEntry;

// （上面的 struct/enum/impl 按 Interfaces 块逐字定义）
impl AgentEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, seq: AtomicU64::new(0) }
    }
    pub fn emit(&self, kind: AgentEventKind) {
        let event = AgentEvent {
            seq: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
            occurred_at: Utc::now(),
            kind,
        };
        // 无订阅者时 send 返回 Err——持久化出口允许零订阅（测试/早期启动），忽略
        let _ = self.tx.send(event);
    }
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }
}
```

- [ ] **Step 4: 跑测试确认通过** → `cargo test -p tinyiothub-cloud loop_::events` 全绿

- [ ] **Step 5: Commit** → `git commit -m "refactor(agent): AgentEvent contract + broadcast bus (Task 2)"`

## Task 3: RestoreSnapshot + AgentRuntime 门面 + 命令 API

**Files:**
- Create: `apps/cloud/src/domains/agent/loop_/runtime.rs`（AgentRuntime 门面）、`apps/cloud/src/domains/agent/loop_/snapshot.rs`
- Test: 各自 `#[cfg(test)]` mod
- Modify: `loop_/mod.rs`

**Interfaces:**
- Produces:

```rust
// snapshot.rs
pub struct WorkspaceHeartbeatState {
    pub workspace_id: String,
    pub tasks: Vec<HeartbeatTask>,
    pub trust_config: TrustConfig,
    pub interval_minutes: u32,
}
pub struct RestoreSnapshot {
    pub heartbeat: Vec<WorkspaceHeartbeatState>,
    pub recent_runs: Vec<RunReport>,   // pushback/dedup 预热窗口：每 workspace 最近 50 条
}

// runtime.rs
pub struct AgentRuntime { /* thing_agents: Arc<ThingAgentManager>, heartbeat: Arc<HeartbeatRunner>, orchestrator: Arc<Orchestrator>, events: Arc<AgentEventBus>, registry: Arc<RunRegistry> */ }
impl AgentRuntime {
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent>;   // Task 11 启动顺序：先 subscribe 再 restore
    pub fn restore(snapshot: RestoreSnapshot, deps: RuntimeDeps) -> Self;
    pub fn dump_state(&self) -> RestoreSnapshot;                  // Lagged resync + 周期对账出口
    // 命令入站（D3）。调用约定（D11-⑤）：cloud 先写 DB 成功，再调命令；命令失败告警
    pub fn update_trust_config(&self, workspace_id: &str, config: TrustConfig);
    pub fn update_heartbeat_interval(&self, workspace_id: &str, interval_minutes: u32);
    pub fn reload_heartbeat_tasks(&self, workspace_id: &str, tasks: Vec<HeartbeatTask>);
    pub fn active_runs(&self) -> Vec<RunReport>;                  // D13：实时读 API
    pub fn heartbeat_tasks(&self, workspace_id: &str) -> Vec<HeartbeatTask>;  // Task 5 测试断言用
    pub fn bus(&self) -> &AgentEventBus;                          // 测试经 bus().emit(...) 注入事件（Task 8）
}
```

`RuntimeDeps` 包含现有构造所需（`AiEventPublisher`、provider_factory 等）——**实现时从 `service_manager.rs` 现有接线代码收集**，本任务只聚已有件，不造新依赖。

- [ ] **Step 1: 写失败测试**——restore 后 dump_state 往返：

```rust
#[test]
fn restore_dump_roundtrip_preserves_heartbeat_state() {
    let snap = RestoreSnapshot {
        heartbeat: vec![WorkspaceHeartbeatState {
            workspace_id: "ws1".into(),
            tasks: vec![/* 一条 HeartbeatTask fixture */],
            trust_config: TrustConfig::default(),
            interval_minutes: 30,
        }],
        recent_runs: vec![],
    };
    let rt = AgentRuntime::restore(snap, RuntimeDeps::test_stub());
    let dumped = rt.dump_state();
    assert_eq!(dumped.heartbeat.len(), 1);
    assert_eq!(dumped.heartbeat[0].workspace_id, "ws1");
    assert_eq!(dumped.heartbeat[0].interval_minutes, 30);
}
```

- [ ] **Step 2: 跑测试确认失败**（`AgentRuntime` 不存在）
- [ ] **Step 3: 实现 runtime.rs/snapshot.rs**——门面聚合现有 `ThingAgentManager`/`HeartbeatRunner`/`Orchestrator`，命令方法当前先委托到 runner/manager 的现有方法（Task 6 才把存储换成内存态；本任务命令实现允许临时仍走 repo，下一步替换）
- [ ] **Step 4: 测试通过**
- [ ] **Step 5: Commit** → `refactor(agent): AgentRuntime facade + RestoreSnapshot + commands (Task 3)`

## Task 4: RunRegistry（thing_agent 运行记录内存化）

**Files:**
- Create: `apps/cloud/src/domains/agent/loop_/thing_agent/registry.rs`
- Modify: `apps/cloud/src/domains/agent/loop_/thing_agent/manager.rs:321` 区域、`pushback.rs:115-130` 区域
- Test: registry.rs 的 `#[cfg(test)]`

**Interfaces:**
- Produces:

```rust
pub struct RunRegistry { /* DashMap<workspace_id, VecDeque<RunReport>>，每 ws 容量：活跃 + 最近 50 条已完成 */ }
impl RunRegistry {
    pub fn record(&self, report: RunReport);                        // 满 50 驱逐最老已完成
    pub fn recent(&self, workspace_id: &str, limit: usize) -> Vec<RunReport>;
    pub fn count_by_dedup(&self, key: &str) -> usize;               // 窗口内 dedup 计数
    pub fn prewarm(&self, reports: Vec<RunReport>);                 // restore 预热
    pub fn active(&self) -> Vec<RunReport>;                         // D13 实时读
}
```

- [ ] **Step 0（前置验证，必须做）**: 读 `pushback.rs:115-130` 的 `policy_relax_hint` 与 dedup 调用链，确认 dedup 判定是否只关心近期窗口。把结论写进 registry.rs 模块文档（`//! dedup 窗口语义：...`）。若 dedup 需全量历史 → 停止本 Task，dedup 检查上移到 cloud 指令注入前（HeartbeatBridge 留 cloud 侧），并更新 spec §8
- [ ] **Step 1: 写失败测试**——容量驱逐 + dedup 计数 + prewarm：

```rust
#[test]
fn registry_evicts_oldest_completed_beyond_50() {
    let reg = RunRegistry::new();
    for i in 0..55 { reg.record(fixtures::report("ws1", &format!("run{i}"))); }
    assert_eq!(reg.recent("ws1", 100).len(), 50);
    assert_eq!(reg.recent("ws1", 1)[0].run_id, "run54");
}
#[test]
fn count_by_dedup_counts_within_window() {
    let reg = RunRegistry::new();
    reg.record(fixtures::report_with_dedup("ws1", "k1"));
    reg.record(fixtures::report_with_dedup("ws1", "k1"));
    assert_eq!(reg.count_by_dedup("k1"), 2);
}
```

- [ ] **Step 2: 确认失败 → Step 3: 实现 registry.rs → Step 4: 测试通过**
- [ ] **Step 5: manager.rs 改造**：`manager.rs:321` 的 `deps.runs_repo.insert_run(...)` 调用点改为：

```rust
// 内存记录（真相源）+ 事件出口（持久化投影）。落库失败不阻断回推语义不变（T12）。
deps.registry.record(report.clone());
deps.events.emit(AgentEventKind::RunRecorded {
    report: Box::new(report),
    problem_key: problem_key.map(str::to_owned),
    dedup_key: signal.dedup_key.clone(),
});
```
（`runs_repo` 字段从 deps 删除；`report.clone()` 的代价可接受——每 run 一次）

- [ ] **Step 6: pushback.rs 改造**：`runs_repo.<recent 查询>`（:125 区域）改 `registry.recent(workspace_id, 50)`；dedup 计数改 `registry.count_by_dedup(key)`
- [ ] **Step 7: 现有测试修复**——`manager.rs`/`pushback.rs` 测试里的 `RunsProbe`/`real_runs_with` SQLite 夹具改为内存 registry 夹具（应用 `pending-actions-expect-panics-default-ctx` 教训：测试夹具显式 wiring，不用 Default）
- [ ] **Step 8: `cargo test -p tinyiothub-cloud` 全绿 → Commit** → `refactor(agent): RunRegistry in-memory run records (Task 4)`

## Task 5: Heartbeat 存储解耦（runner.rs / loop_.rs）

**Files:**
- Modify: `loop_/heartbeat/runner.rs`（:76,90,98,148,176,188,289,303,328-329,342）、`loop_/heartbeat/loop_.rs`（:15,31,129）
- Delete: `loop_/heartbeat/repo.rs`（re-export 废除）
- Test: 现有 `real_repo()` SQLite 夹具（runner.rs:359+、loop_.rs:376+）改内存夹具

**改造映射**（生产 4 个 repo 调用点）：

| 现调用 | 改为 |
|---|---|
| `runner.rs:148` `task_repo.list_by_workspace(ws)`（start 时加载任务） | 内存：`self.tasks.get(ws)`——数据由 `restore()`/`reload_heartbeat_tasks()` 命令注入 |
| `runner.rs:289` `task_repo.save_trust_config(ws, &config)` | 删除 SQL；只更新内存 `trust_configs` + `events.emit(TrustConfigChanged{..})`。DB 写由 cloud 侧 service 先做（D11-⑤ 写序） |
| `runner.rs:303` `load_heartbeat_config(ws)` | 内存：`intervals.get(ws)`（snapshot/命令注入；缺省 `MIN_HEARTBEAT_INTERVAL_MINUTES` 逻辑保留，常量现在来自 `tinyiothub_core::heartbeat`） |
| `runner.rs:342` `load_trust_config(ws)` | 内存：`trust_configs.get(ws)`，缺省 `TrustConfig::default()` |
| `loop_.rs:129` `task_repo.list_by_workspace`（ReloadTasks 信号） | 信号语义改为"内存已被命令更新，重读内存"——直接从 runner 的内存 tasks 拿 |

- [ ] **Step 1: 写失败测试**——命令注入后 runner 内存可见 + trust 变更发事件：

```rust
#[tokio::test]
async fn reload_tasks_command_updates_in_memory_tasks() {
    let rt = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
    rt.reload_heartbeat_tasks("ws1", vec![fixtures::heartbeat_task("ws1", "check temp")]);
    assert_eq!(rt.heartbeat_tasks("ws1").len(), 1);
}

#[tokio::test]
async fn update_trust_config_emits_event() {
    let rt = AgentRuntime::restore(empty_snapshot(), RuntimeDeps::test_stub());
    let mut rx = rt.subscribe();
    rt.update_trust_config("ws1", TrustConfig::default());
    let ev = rx.recv().await.unwrap();
    assert!(matches!(ev.kind, AgentEventKind::TrustConfigChanged { .. }));
}
```

- [ ] **Step 2: 确认失败 → Step 3: 按上表改造 5 个调用点，删 repo.rs → Step 4: 测试通过**
- [ ] **Step 5: cloud 侧写序接线**——`host/heartbeat.rs`（留 cloud 的 service）的 trust config 更新路径改为：**先** `task_repo.save_trust_config`（db repo 仍在，service 层照用）**成功后**调 `runtime.update_trust_config(...)`。tasks CRUD 路径同理（先 db `replace_all`/`upsert`，后 `reload_heartbeat_tasks` 命令）
- [ ] **Step 6: 全量测试 + Commit** → `refactor(agent): heartbeat runtime decoupled from task repo (Task 5)`

## Task 6: Orchestrator callbacks 解耦

**Files:**
- Modify: `loop_/orchestrator/callbacks.rs`（HeartbeatBridge 的 `runs_repo`、`insert_result` 调用点）
- Modify: `loop_/orchestrator/mod.rs`（:21,40,57 的 `task_repo` 字段）

**改造映射**：
- `insert_result(ws, &result)` → `events.emit(AgentEventKind::HeartbeatResultReady { result })`（subscriber 落库）
- HeartbeatBridge 的 dedup（`runs_repo` 查询）→ `registry.count_by_dedup(...)` / `registry.recent(...)`（Task 4 的 Step 0 结论适用）
- `MemoryService` 持有的 callback（memory profile compile/weekly digest）：**callback 注册机制留 crate，MemoryService 具体 callback 移到 cloud 侧注册**——orchestrator 定义 `#[async_trait] pub trait OrchestratorCallback`（若已有等价 trait 则复用），cloud 在 service_manager 接线处把吃 `MemoryService` 的闭包注册进来。**MemoryService 类型不得出现在 orchestrator 的字段里**

- [ ] **Step 1: 写失败测试**——HeartbeatCompleted 事件流转后产生 HeartbeatResultReady 事件且 dedup 走内存
- [ ] **Step 2-4: 确认失败 → 改造 → 通过**
- [ ] **Step 5: `cargo test -p tinyiothub-cloud` 全绿 + Commit** → `refactor(agent): orchestrator callbacks decoupled from repos (Task 6)`

## Task 7: AgentPool 剥离存储字段

**Files:**
- Modify: `host/agent/pool.rs`（:84,91,107-108,149,156,222,244 区域）
- Modify: `host/agent/chat.rs` 或 pool 内使用 `db_pool`/`memory_store`/`memory_service` 的方法（实现时 grep `self.db_pool\|self.memory_store\|self.memory_service` 找全）

**改造映射**：
- `db_pool` 字段删除。:222 `config_service::get_config(&self.db_pool, agent_id)`——`config/service.rs` 是留 cloud 的数据实现（D2）：改为 **cloud 在调用 pool 方法前先把 config 查出来作为参数传入**（pool 方法签名加 `config: AgentConfig` 参数），或 pool 持有 `config_provider: Arc<dyn Fn(&str) -> ...>` 闭包（cloud 注入）。**选参数传入**——显式优于隐式
- `memory_store` 字段删除——其使用点（prompt 构造）同样改为调用方注入数据
- `memory_service` 字段移出 pool——迁到 cloud 侧 chat service（`host/chat/service.rs` 留 cloud）
- `trust_configs: DashMap<String, TrustConfig>` 保留（TrustConfig 已是 core 类型）
- `event_publisher`、`runtime: ToolRuntimeContext`、`provider_factory`、`shared_memory`（zeroclaw 类型）保留

- [ ] **Step 1: grep 全部使用点**：`grep -n "self\.db_pool\|self\.memory_store\|self\.memory_service" apps/cloud/src/domains/agent/host/agent/*.rs`
- [ ] **Step 2: 写失败测试**——pool 构造不再需要 db_pool：`AgentPool::new(/* 无 db_pool/memory_store 参数 */)` 编译通过且 `get_or_build` 路径走注入的 config
- [ ] **Step 3: 改造 + 修 cloud 调用方**（`autonomous_factory.rs`、chat service、handlers 的 pool 调用点传 config）
- [ ] **Step 4: `cargo test -p tinyiothub-cloud` 全绿 + Commit** → `refactor(agent): AgentPool stripped of storage fields (Task 7)`

## Task 8: 持久化 subscriber（cloud 侧，新写）

**Files:**
- Create: `apps/cloud/src/domains/agent/host/persist.rs`
- Test: `apps/cloud/src/tests/agent_persist_tests.rs`（或并入现有 tests 目录约定）

**Interfaces:**
- Consumes: `AgentRuntime::subscribe()`、`AgentRuntime::dump_state()`、`tinyiothub_storage::{agent_runs::AgentRunsRepository, heartbeat::HeartbeatTaskRepository}`
- Produces: `pub async fn run_persistence_subscriber(runtime: Arc<AgentRuntime>, db: Arc<Database>)`——service_manager 启动时 spawn

**核心逻辑**（fencing 用 `occurred_at`，DB schema 不变——spec §6 的 `version: u64` 细化为进程内 `seq` + DB 侧时间戳 fencing，这是本计划对 spec 的有意细化）：

```rust
loop {
    match rx.recv().await {
        Ok(event) => project(&event).await,       // 幂等 upsert，见下
        Err(RecvError::Lagged(n)) => {
            warn!(dropped = n, "persistence subscriber lagged, full resync");
            resync(runtime.dump_state()).await;   // 全量覆写
        }
        Err(RecvError::Closed) => return,
    }
}
// project 的 fencing 形态（heartbeat tasks 类有 updated_at 的行）：
//   UPDATE ... WHERE updated_at < event.occurred_at
// agent_runs 是 insert-once（run 完成才落库，现状如此）：INSERT OR IGNORE by run_id
// 落库失败：重试 3 次指数退避 → 仍失败写 dlq 表 + error!（不反向影响 crate）
```

- [ ] **Step 1: 写失败测试**（投影 + fencing + Lagged resync 三个用例）：

```rust
#[tokio::test]
async fn projects_run_recorded_to_agent_runs() {
    let (db, runtime) = fixtures::db_and_runtime().await;
    let h = tokio::spawn(run_persistence_subscriber(runtime.clone(), db.clone()));
    runtime.emit_run_recorded(fixtures::report("ws1", "run1")).await;
    fixtures::wait_until(|| async { count_runs(&db, "run1").await == 1 }).await;
    h.abort();
}

#[tokio::test]
async fn stale_event_does_not_overwrite_newer_row() {
    // 先落 occurred_at=T2 的事件，再回放 occurred_at=T1(<T2) 的事件
    // 断言行内容仍是 T2 的（fencing 生效）
}

#[tokio::test]
async fn lagged_subscriber_resyncs_from_dump_state() {
    // broadcast 容量 2 的测试 bus，发 5 事件 → subscriber 收到 Lagged →
    // 断言 dump_state 全量投影落库
}
```

- [ ] **Step 2-4: 失败 → 实现 → 通过**
- [ ] **Step 5: 周期对账**——`run_persistence_subscriber` 内嵌 `tokio::time::interval(Duration::from_secs(300))` 分支：每 5 分钟 `resync(dump_state())`
- [ ] **Step 6: Commit** → `refactor(agent): persistence subscriber with fencing + resync (Task 8)`

## Task 9: 启动接线——订阅顺序、rehydration、僵尸 reconcile

**Files:**
- Modify: `apps/cloud/src/shared/service_manager.rs`（:151,235 区域接线）、`apps/cloud/src/bootstrap.rs`

**顺序（D11-①③，错序即丢事件）**：

```rust
// 1. 从 DB 构造 RestoreSnapshot（活跃 heartbeat 配置/任务 + 每 ws 最近 50 条 run）
let snapshot = build_snapshot(&db).await;
// 2. 先建 runtime 并订阅（restore 期间的事件不丢）
let runtime = AgentRuntime::restore(snapshot, deps);
let rx = runtime.subscribe();
// 3. 僵尸 reconcile：DB 里 status='running' 但 registry 无主的 run → 'interrupted'
reconcile_zombie_runs(&db, &runtime).await;
// 4. 启动 subscriber 与周期对账
tokio::spawn(run_persistence_subscriber(runtime.clone(), db.clone()));
// 5. 既有 AiEventPublisher / DropNotifier / SqliteDeadLetterQueue 接线原样保留
```

- [ ] **Step 1: 写失败测试**——僵尸 reconcile：

```rust
#[tokio::test]
async fn startup_marks_orphan_running_runs_interrupted() {
    let db = fixtures::db().await;
    fixtures::insert_run_with_status(&db, "ghost", "running").await;
    let runtime = bootstrap_test_runtime(&db).await;   // 走真实启动顺序
    let status = fixtures::run_status(&db, "ghost").await;
    assert_eq!(status, "interrupted");
}
```

- [ ] **Step 2-4: 失败 → 实现 → 通过**
- [ ] **Step 5: `cargo test -p tinyiothub-cloud`（含 agent_tasks_api_tests）全绿 + Commit** → `refactor(agent): startup rehydration + zombie reconcile (Task 9)`

## Task 10: Phase 1 收口——全链路 E2E（D6）

**Files:**
- Test: `apps/cloud/src/tests/agent_loop_e2e_tests.rs`（新）

- [ ] **Step 1: 写 E2E 测试**（fake LLM 一轮 loop → 事件 → DB 投影 → HTTP 读 API）：

```rust
#[tokio::test]
async fn thing_agent_run_projects_to_db_and_read_api() {
    let app = fixtures::test_app_with_fake_llm().await;   // 复用现有 thing_agent_loop_tests 的 fake LLM 夹具
    app.inject_directive("ws1", "检查车间温度").await;
    // 断言事件序列
    let ev = app.next_agent_event().await;
    assert!(matches!(ev.kind, AgentEventKind::RunRecorded { .. }));
    // 断言 DB 投影
    fixtures::wait_until(|| async { app.count_runs("ws1").await == 1 }).await;
    // 断言 HTTP 读 API 一致
    let resp = app.get("/api/v1/agent/runs?workspace_id=ws1").await;
    resp.assert_status_ok();
    assert_eq!(resp.json()["runs"][0]["workspace_id"], "ws1");
}
```

- [ ] **Step 2: 跑通 + 既有全部集成测试绿 + Commit** → `test(agent): full-chain E2E run→event→DB→API (Task 10)`

**Phase 1 完成判据**：`cargo test -p tinyiothub-cloud` 全绿；`grep -rn "tinyiothub_storage\|sqlx" apps/cloud/src/domains/agent/loop_/` 仅命中 test 模块外的 0 处；`host/agent/pool.rs` 无 `SqlitePool`/`MemoryStore`。

---

# Phase 2 — crate 骨架 + CI 守卫

## Task 11: crates/agent 骨架 + workspace 注册 + CI 守卫

**Files:**
- Create: `crates/agent/Cargo.toml`、`crates/agent/src/lib.rs`（`#![forbid(unsafe_code)]` + 空模块声明）
- Modify: `Cargo.toml`（workspace members 已有 `crates/*` 通配，确认即可）、`.github/workflows/ci.yml:129-147`

- [ ] **Step 1: 建 Cargo.toml**——`[package] name = "agent"`、`[lib] name = "tinyiothub_agent"`，依赖：`core/llm/policy/skills/runtime`（workspace）、`tokio`、`serde`、`chrono`、`dashmap`、`thiserror`、`async-trait`、`tracing`、`zeroclaw`（git tag `v0.8.1-patched`，从 `apps/cloud/Cargo.toml:153` 移入）
- [ ] **Step 2: `cargo check -p tinyiothub-agent` 通过**
- [ ] **Step 3: ci.yml 守卫重写**——G9 守卫路径从 `apps/cloud/src/domains/agent/loop_` 改为 `crates/agent/src`，检查词表扩为 `axum\|sqlx\|tinyiothub_storage\|crate::domains`；thing/tenant 边守卫（:129-137）的 `super::.*agent` 模式保留、目标路径确认仍有效（thing/tenant 不搬）。加 `cargo tree -p tinyiothub-agent | grep -E "sqlx|tinyiothub-storage" && exit 1` 依赖树检查
- [ ] **Step 4: 守卫自证**——临时在 `crates/agent/src/lib.rs` 加 `use sqlx;` 跑 CI 守卫脚本本地复现 fail，然后回滚（应用 `verify-scripted-sed-substitutions` 教训：守卫变更必须验证真的拦得住）
- [ ] **Step 5: Commit** → `refactor(agent): crates/agent skeleton + hardened CI guards (Task 11)`

---

# Phase 3 — memory 纯逻辑并入（D10'）

## Task 11.5: 斩断 runtime→db（D15 用户裁决，新增）

**背景**：`crates/runtime` 直接使用存储——`cron_executors.rs:16,47`（`tinyiothub_storage::database::Database` + `device_command::find_device_command_by_device_and_name`）、`data_server.rs:18`（`tinyiothub_storage::cache::DeviceCache`）。agent→runtime→db→sqlx 传递链使 tree 级纯度不可达。D15 裁决：本期端口化斩断，CI tree 守卫升级为全树（去掉 Task 11 的 --depth 1）。

**Files:**
- Modify: `crates/runtime/src/cron_executors.rs`、`crates/runtime/src/data_server.rs`、`crates/runtime/src/lib.rs`（端口 trait 导出）、`crates/runtime/Cargo.toml`（删 `db = { workspace = true }`）
- Modify: `apps/cloud` 组合根（service_manager/bootstrap）——注入 db-backed 实现
- Test: 端口注入后的等价行为测试

**Interfaces:**
- Produces（确切名字后续任务依赖）：
  - `runtime::ports::DeviceCommandQueries`（async trait：`find_by_device_and_name(&self, device_id: &str, name: &str)` 返回类型以实现时 `crates/db/src/device_command.rs` 现有函数为准）
  - `runtime::ports::DeviceCacheSource`（async trait：方法集以 data_server.rs 实际调用的 DeviceCache 方法为准）
- cloud 侧适配器：`apps/cloud/src/shared/` 下新建薄 adapter（直接委托 `tinyiothub_storage` 具体类型，符合 8/3 db 层模式）

- [ ] **Step 1: 盘点使用面**——`grep -rn "tinyiothub_storage" crates/runtime/src/` 列出全部调用点与用到的方法签名，写进 commit message
- [ ] **Step 2: 定义端口 trait（先写失败测试）**——cron_executors/data_server 改为 Arc<dyn 端口> 注入；cloud 侧 adapter 委托现有 db 函数
- [ ] **Step 3: 删 runtime 的 db 依赖**——`cargo tree -p runtime | grep sqlx` 应为空；`cargo check --workspace` 通过
- [ ] **Step 4: CI tree 守卫升级**——ci.yml 的 agent tree 守卫去掉 `--depth 1`，全树 grep 覆盖 sqlx/tinyiothub-storage/包名 db；自证：给 agent Cargo.toml 注入 `db = { workspace = true }` 确认拦截，回滚
- [ ] **Step 5: 全量测试绿 + Commit** → `refactor(runtime): sever runtime→db edge via ports (Task 11.5, D15)`

## Task 12: crates/memory 纯逻辑 → crates/agent/src/memory/

**Files:**
- Move: `crates/memory/src/{knowledge,reflect,types,metrics,workspace_memory,provider,reference}.rs` → `crates/agent/src/memory/`
- Modify: `crates/memory/src/lib.rs`（只剩 `service.rs`）、`crates/agent/src/lib.rs`（`pub mod memory;`）、全部消费方 import

**消费方清单**（实测，无第三方）：`apps/cloud/src/domains/agent/**`（mod.rs、host/reflect.rs、autonomous_factory.rs、chat/service.rs、agent/pool.rs、loop_ 多处）、`apps/cloud/src/shared/service_manager.rs`

- [ ] **Step 1: `git mv` 七文件到 `crates/agent/src/memory/`，改 `mod` 声明**
- [ ] **Step 2: import 重写**：`grep -rln "tinyiothub_memory::" apps/ crates/ --include="*.rs"` → 纯逻辑类型改 `tinyiothub_agent::memory::`；`MemoryService`（引擎，留 memory crate）消费方保持 `tinyiothub_memory::service::MemoryService`
- [ ] **Step 3: `crates/agent/Cargo.toml` 不新增 memory 依赖；`crates/memory` 保留（engine 继续服务 cloud）**
- [ ] **Step 4: `cargo check --workspace && cargo test --workspace` 全绿**
- [ ] **Step 5: Commit** → `refactor(agent): absorb memory pure logic into crates/agent (Task 12, D10')`

---

# Phase 4 — 搬 runtime（此时是真正的 git mv）

## Task 13: loop_/ → crates/agent/src/runtime/

**Files:**
- Move: `apps/cloud/src/domains/agent/loop_/*` → `crates/agent/src/runtime/`（含 Phase 1 新建的 events/snapshot/runtime/registry）
- Modify: `crates/agent/src/lib.rs` facade、`apps/cloud/src/domains/agent/mod.rs`（删 types 兼容 re-export）、3 个外部消费方

- [ ] **Step 1: `git mv` + 模块路径调整**（`crate::domains::agent::loop_::X` → `crate::runtime::X`；`loop_` 目录名拍平为 `runtime`）。同一步在 `crates/agent/src/` 建 `error.rs`：thiserror `AgentError { Llm, Tool, Policy, Session, Internal }` 变体（无 HTTP 语义），从被移代码现有的错误类型归拢；cloud 侧 `domains/agent/error.rs` 做 `AgentError → ApiResponse` 映射
- [ ] **Step 2: cloud 侧 import 全部指向 `tinyiothub_agent::runtime::`**；`domains/agent/mod.rs` 的 `pub mod types` 兼容 re-export **删除**（D5）
- [ ] **Step 3: 3 个外部消费方直改**：`apps/cloud/src/domains/driver/plugin/registry.rs`、`apps/cloud/src/domains/mcp/tool_registry.rs`、`apps/cloud/src/domains/mcp/agent_bridge.rs`——`grep -rn "domains::agent::types\|domains::agent::loop_" apps/cloud/src --include="*.rs" | grep -v "domains/agent/"` 找全
- [ ] **Step 4: `cargo check --workspace && cargo test -p tinyiothub-agent && cargo test -p tinyiothub-cloud` 全绿；CI 守卫本地复跑确认生效**
- [ ] **Step 5: Commit** → `refactor(agent): move loop_ runtime into crates/agent (Task 13)`

---

# Phase 5 — 搬 pool / tools 框架 / session / prompt

## Task 14: host 通用机制迁入 crates/agent

**Files:**
- Move: `host/agent/` → `crates/agent/src/pool/`；`host/session.rs` → `crates/agent/src/session.rs`；`host/shared/`（prompt 组装+paths）→ `crates/agent/src/prompt/`；`host/tools/{mod,service 框架部分}` → `crates/agent/src/tools/`
- **留 cloud 不搬**：`host/handler/`、`chat/handler/`、`host/chat/`（含 history.rs、service.rs 的 DB 部分）、`host/tools/thing/`（数据实现）、`host/tools/{canvas,autonomous_invoke,dispatch_task,get_skill}.rs`、`host/{dlq_repo,skill,policy_engine,thing_agent_host,heartbeat}.rs`、`host/config/service.rs`、`host/persist.rs`（Task 8）、`host/scaffold.rs` 的文件系统部分按 D2 判定（逐文件以"零 sqlx 零 crate::domains"为界）
- Modify: zeroclaw 依赖从 `apps/cloud/Cargo.toml` 删（已在 Task 11 移入 crates/agent）

- [ ] **Step 1: 逐文件判定表先落地**——对 `host/tools/` 每个文件 grep `sqlx\|tinyiothub_storage\|crate::domains`，零命中 → 搬；有命中 → 留 cloud 并在文件头注释 `// 数据实现，留 cloud（D2）`。判定表贴进 commit message
- [ ] **Step 2: `git mv` + import 重写 + `ToolRuntimeContext` 归位**（框架→crate；其中 device cache/data server 字段若是 cloud 类型，改为 crate 内 trait 对象或泛型——实现时按现有字段类型定）
- [ ] **Step 3: cloud 工具实现注册进 crate 的 `ToolRegistry`**——在 `service_manager.rs` 接线处显式注册（thing 工具等）
- [ ] **Step 4: 全量测试 + CI 守卫复跑 + Commit** → `refactor(agent): move pool/tools-framework/session/prompt into crates/agent (Task 14)`

---

# Phase 6 — 清理

## Task 15: 清理 + 文档

**Files:**
- Delete: cloud 侧搬空的模块文件、`domains/agent/mod.rs` 的残留 re-export
- Modify: `apps/cloud/src/domains/agent/mod.rs`（只剩 AgentState + handler 注册）、`AGENTS.md`（Repository Map 加 `agent` 行、Stability Tiers 表加 Beta 行、风险层说明）、`apps/cloud/Cargo.toml`（删 zeroclaw，加 `agent = { workspace = true }`；workspace `Cargo.toml` 的 `[workspace.dependencies]` 加 `agent = { path = "crates/agent" }`）
- Modify: `host/handler/workspace_heartbeat.rs`——实时字段（last_tick/指标）改从 `AgentRuntime` 内存 API 读（D13）

- [ ] **Step 1: 删除与 import 收尾**，`cargo check --workspace` 无 warning（孤儿 import 清理干净，含本计划自己引入的）
- [ ] **Step 2: D13 心跳实时读改造 + 测试**（读 API 返回内存态 last_tick）
- [ ] **Step 3: AGENTS.md 更新**
- [ ] **Step 4: 验收清单逐项核对**（spec §12 七条）+ 全量 `cargo test --workspace` + CI 守卫复跑
- [ ] **Step 5: Commit** → `refactor(agent): cleanup + docs after crate extraction (Task 15)`

---

## 验收清单（spec §12 修订版）

- [ ] `cargo build -p tinyiothub-agent` 独立编译通过
- [ ] `crates/agent` 源码 grep 无 `axum`/`sqlx`/`tinyiothub_storage`；`cargo tree -p tinyiothub-agent` 无 sqlx/tinyiothub-storage（Task 1 斩断 policy/skills→db 后可达）
- [ ] CI 守卫生效且经"故意违规"自证
- [ ] `cargo test -p tinyiothub-agent` 全绿（无 DB 无 HTTP）
- [ ] cloud 现有 agent 集成测试全部通过
- [ ] Task 8/9/10 的 fencing/reconcile/resync/E2E 测试落地
- [ ] AGENTS.md 更新

## 风险与备注

- **Task 4 Step 0 是本计划唯一的语义不确定点**（dedup 窗口）；结论可能把 HeartbeatBridge 推向 cloud 侧，属正常分支
- Task 13/14 是 `git mv` 级移动，review 重点看 import 重写是否彻底（grep 验证，应用 verify-scripted-sed-substitutions 教训）
- `Dockerfile:80` 的 `apps/cloud/templates` COPY 不受影响（模板不搬）
- 若 Phase 1 中途需暂停：每个 Task 独立 commit 可安全停在任意 Task 边界，系统行为与现状一致（事件是叠加出口，DB 直写在 Task 4-7 才逐点替换）
