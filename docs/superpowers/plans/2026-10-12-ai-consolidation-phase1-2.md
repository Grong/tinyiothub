# AI 子系统全链路梳理 Phase 1+2(概念文档 + 内核归一)Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 双 runner 归一为统一 Run 循环(心跳 tick = `TriggerSource::Heartbeat` 驱动的 run),HeartbeatResult 退役为读侧兼容视角,行为保持清单逐项有测试锁定。

**Architecture:** 心跳/告警信号统一走 `WakeSignal → Scheduler(merge window/dedup/throttle) → run_pipeline → RunReport → agent_runs`;提案仍留 `agent_actions`(审批生命周期在那);读侧 get_logs 改读 agent_runs 经纯函数适配器。HeartbeatRunner/heartbeat_loop 在等价对照通过后退役。

**Tech Stack:** Rust + Axum + Tokio + sqlx(SQLite,编译期内嵌迁移);前端 Phase 3 不在本计划。

**Spec:** `docs/superpowers/specs/2026-10-12-ai-consolidation-design.md`(四轮评审全 CLEAR;D8-D11、F1-F18、R1 裁定在册)

## Global Constraints

- 分支:`ai-consolidation`(已建);每 Task 一个独立 commit,可随时回滚
- TDD:行为保持清单(连败暂停/last_tick/热更新/每工作区间隔)逐项先写失败测试
- sqlx 编译期内嵌迁移:改 `migrations/` 后必须 `touch crates/db/src/lib.rs` 强制重建,否则 CI 绿本地红
- 信任门读档时机(R1):每次动作调用新鲜读档(autonomous_invoke.rs:278-280 O7 seam),不做快照
- Shadow 双跑(Q1/F2):比对 harness 内两路径共用 mock 池;统一路径只产报告不落库/不落提案;白名单=status/summary/proposals 集合/trace 工具名序列
- 迁移顺序(埋伏③):先 trace 回填(合成 agent_runs 行)再 tick_id→run_id 改写,否则提案行找不到锚
- RunRegistry(记忆注入源)与 ActiveRunRegistry(实时 trace 源)不合并,归一时注明关系(eng S1-P3)
- 概念模型 lint(E3)首版 warn-only
- 全量门禁:`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`(pre-push 钩子会跑,~10min)

---

## Phase 1: 概念文档(纯 docs 零风险,先行作评审校准尺)

### Task 1: `docs/ai-concept-model.md`

**Files:**
- Create: `docs/ai-concept-model.md`

**Interfaces:**
- Produces: 概念图 + 词汇表 + 概念→代码/表映射(Phase 2/3 的评审校准尺;E3 lint 的异名清单来源=词汇表右列)

- [ ] **Step 1: 写文档**

内容骨架(逐项落,词汇表与 spec §1 逐字一致):

```markdown
# AI 概念模型(单一事实源)

## 概念链
触发(Trigger)──► 运行(Run)──► 判断(Verdict)──► 处置(Disposition)

## 词汇表
| 概念 | 定义 | 收编的异名 |
(照 spec §1 词汇表七行:Run/Verdict/Proposal/Disposition/Trace/Trust Mode/Narrative)

## 概念→代码/表映射
| 概念 | 代码 | 表 |
| Run | crates/agent/src/runtime/thing_agent(runner.rs/manager.rs) | agent_runs |
| Verdict | host/judge(judgment_subscriber.rs parse_verdict) | judgments |
| Proposal | host/persist 提案行写入 | agent_actions(action_type='proposal') |
| Trace | ToolTraceEntry(core::agent_runs) | agent_runs.report JSON |
| Trust Mode | policy_engine.rs + autonomous_invoke.rs(每动作新鲜读,O7) | workspaces + policy_rules |
| Narrative | =summary 的展示形态(D8,无独立字段) | — |
| 告警→Heartbeat 变体 | orchestrator/callbacks.rs HeartbeatBridge | — |

## 两个注册表的关系(eng S1-P3)
RunRegistry=记忆注入源(run_pipeline 读最近 5 条摘要进 prompt);ActiveRunRegistry=实时 trace 源(trace 端点轮询)。职责不同,不合并。
```

- [ ] **Step 2: Commit**

```bash
git add docs/ai-concept-model.md
git commit -m "docs(concept): AI 概念模型单一事实源——概念图+词汇表+代码映射"
```

---

## Phase 2: 内核归一(行为保持重构)

### Task 2: 基线 + 测试分类(K3)

**Files:**
- Create: `docs/superpowers/plans/ai-consolidation-test-classification.md`

**Interfaces:**
- Produces: 三份必重写测试清单(落库改向后直接断言 agent_actions 内容的):`crates/db/src/heartbeat.rs` 锚行/提案共存测试、`apps/cloud/src/tests/agent_persist_tests.rs`、`heartbeat_bridge_tests`——标「实现细节重写」;其余标「契约保」

- [ ] **Step 1: 跑基线**

Run: `cargo test 2>&1 | tail -5`
Expected: 全绿(分类前的基线;红了先修红再动手)

- [ ] **Step 2: 写分类清单**

对上述三个文件的每个测试函数打标签(契约保=归一后必须仍绿;实现细节重写=落库改向后允许重写断言)。清单格式:`文件 | 测试函数 | 分类 | 理由`。

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/plans/ai-consolidation-test-classification.md
git commit -m "test(agent): Phase 2 测试分类——契约保 vs 实现细节重写清单"
```

### Task 3: `TriggerSource::Heartbeat` 变体(D11)

**Files:**
- Modify: `crates/agent/src/runtime/thing_agent/types.rs:12-38`
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs:279-290`(trigger_label)、301-313(collect_problem_keys_into)
- Test: `crates/agent/src/runtime/thing_agent/types.rs`(mod tests)

**Interfaces:**
- Produces: `TriggerSource::Heartbeat { alarm: Option<HeartbeatAlarmContext> }`;`HeartbeatAlarmContext { thing_id: Option<String>, alarm_type: Option<String>, rule_id: Option<String>, reason: String, context: String }`
- dedup key 语义:定时 tick=`heartbeat:{ws}`;告警=`heartbeat:{ws}:{thing_id}:{alarm_type}`(N3:不互合并)

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn heartbeat_variant_trigger_label_uses_dedup_key() {
    // 定时 tick:dedup heartbeat:{ws};告警:heartbeat:{ws}:{thing}:{alarm_type}
    let tick = WakeSignal {
        workspace_id: "ws1".into(),
        priority: Priority::Normal,
        source: TriggerSource::Heartbeat { alarm: None },
        dedup_key: Some("heartbeat:ws1".into()),
    };
    assert_eq!(trigger_label(&tick), "heartbeat:ws1");
    // trigger_type_of 取前缀 → "heartbeat"(读侧/统计按类型过滤依赖此前缀)
    assert_eq!(tick.dedup_key.as_deref().unwrap().split(':').next().unwrap(), "heartbeat");
}

#[test]
fn heartbeat_alarm_variant_carries_context_and_distinct_dedup() {
    let alarm = WakeSignal {
        workspace_id: "ws1".into(),
        priority: Priority::Critical,
        source: TriggerSource::Heartbeat {
            alarm: Some(HeartbeatAlarmContext {
                thing_id: Some("d1".into()),
                alarm_type: Some("offline".into()),
                rule_id: Some("r1".into()),
                reason: "设备离线".into(),
                context: "{}".into(),
            }),
        },
        dedup_key: Some("heartbeat:ws1:d1:offline".into()),
    };
    assert_eq!(trigger_label(&alarm), "heartbeat:ws1:d1:offline");
}
```

注:`trigger_label`/`collect_problem_keys_into` 目前在 manager.rs 是私有 fn——测试放 manager.rs 的 `#[cfg(test)] mod tests`,types.rs 只放变体定义。

- [ ] **Step 2: 跑测试确认编译错误**

Run: `cargo test -p agent heartbeat_variant 2>&1 | tail -3`
Expected: FAIL(变体不存在)

- [ ] **Step 3: 实现变体 + 两个 match arm**

types.rs 加:

```rust
/// 心跳巡检 tick(D11 专用变体——不复用 Timer:丢每工作区可配间隔,
/// 且 dedup 与 timer:{ws} 碰撞吞 tick)。alarm=None=定时 tick;
/// Some=告警驱动(优先级映射:Critical→Critical,High/Normal→Normal)。
Heartbeat { alarm: Option<HeartbeatAlarmContext> },
```

manager.rs `trigger_label` 加 arm(放 `Timer` arm 后):

```rust
TriggerSource::Heartbeat { .. } => signal
    .dedup_key
    .clone()
    .unwrap_or_else(|| format!("heartbeat:{}", signal.workspace_id)),
```

`collect_problem_keys_into` 加 arm:`TriggerSource::Heartbeat { .. } => {}`(心跳无 problem_key;X6 桥指令走 UserDirective 不变)。`signal_thing_id`(manager.rs:367)加 arm:alarm.Some 时取 `thing_id`(T7 工单解法注入的同 thing 优先对告警 run 生效)。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p agent heartbeat 2>&1 | tail -3`
Expected: PASS(含新 2 测试 + 既有全绿——match 穷尽性会强制补齐所有 match arm,编译器领着走)

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/runtime/thing_agent/
git commit -m "feat(agent): TriggerSource::Heartbeat 变体(D11)——定时 tick 与告警分键"
```

### Task 4: 连败治理 HeartbeatGovernor(行为保持清单第 1 行,先写失败测试)

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/governor.rs`
- Modify: `crates/agent/src/runtime/thing_agent/mod.rs`(加 `pub mod governor;`)
- Test: 同文件 mod tests

**Interfaces:**
- Produces(后续 Task 5/6/11 消费):

```rust
/// 心跳连败治理:heartbeat 源 run 连败达阈(5)暂停该工作区定时触发;
/// cooldown(一个间隔)后或告警信号到达时恢复。语义对齐 heartbeat/loop_.rs:81-105。
#[derive(Clone, Default)]
pub struct HeartbeatGovernor { /* Arc<DashMap<String, GovernorState>> */ }
impl HeartbeatGovernor {
    pub fn record_success(&self, workspace_id: &str);
    pub fn record_failure(&self, workspace_id: &str); // 达阈置 paused_at=now
    /// paused 且未过 cooldown → true(抑制 tick)。cooldown 到点自动恢复(清态)。
    pub fn is_paused(&self, workspace_id: &str, cooldown: Duration) -> bool;
    pub fn resume(&self, workspace_id: &str); // 告警信号/外部唤醒恢复
    pub fn consecutive_failures(&self, workspace_id: &str) -> u32;
}
```

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn five_consecutive_failures_pause_then_cooldown_resumes() {
    let g = HeartbeatGovernor::default();
    for _ in 0..4 { g.record_failure("ws"); }
    assert!(!g.is_paused("ws", Duration::from_secs(60)), "4 连败未达阈");
    g.record_failure("ws");
    assert!(g.is_paused("ws", Duration::from_secs(60)), "5 连败暂停");
    assert!(g.is_paused("ws", Duration::from_secs(60)));
    // cooldown 用注入时钟测:paused_at 用 Instant,测试用短 cooldown 无法穿越——
    // is_paused 第三个语义(恢复)由 record_success/resume 测:
    g.resume("ws");
    assert!(!g.is_paused("ws", Duration::from_secs(60)));
    assert_eq!(g.consecutive_failures("ws"), 0);
}

#[test]
fn success_resets_failure_count() {
    let g = HeartbeatGovernor::default();
    for _ in 0..4 { g.record_failure("ws"); }
    g.record_success("ws");
    g.record_failure("ws");
    assert!(!g.is_paused("ws", Duration::from_secs(60)), "成功清零后重新计数");
}
```

- [ ] **Step 2: 跑确认失败** — Run: `cargo test -p agent governor 2>&1 | tail -3` Expected: FAIL(类型不存在)

- [ ] **Step 3: 实现 governor.rs**

```rust
//! 心跳连败治理(行为保持清单第 1 行):heartbeat 源 run 连败达阈暂停
/// 该工作区定时触发;cooldown 或告警信号恢复。原 heartbeat/loop_.rs:81-105
/// 语义的统一循环落点——暂停的是"定时触发",告警入口(桥)不受影响。
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Clone, Default)]
pub struct HeartbeatGovernor {
    inner: Arc<dashmap::DashMap<String, GovernorState>>,
}

#[derive(Default)]
struct GovernorState {
    failures: u32,
    paused_at: Option<Instant>,
}

impl HeartbeatGovernor {
    pub fn record_success(&self, workspace_id: &str) {
        self.inner.remove(workspace_id);
    }

    pub fn record_failure(&self, workspace_id: &str) {
        let mut e = self.inner.entry(workspace_id.to_string()).or_default();
        e.failures += 1;
        if e.failures >= MAX_CONSECUTIVE_FAILURES && e.paused_at.is_none() {
            e.paused_at = Some(Instant::now());
        }
    }

    pub fn is_paused(&self, workspace_id: &str, cooldown: Duration) -> bool {
        let Some(e) = self.inner.get(workspace_id) else { return false };
        match e.paused_at {
            None => false,
            Some(at) if at.elapsed() >= cooldown => {
                drop(e);
                // cooldown 到点自动恢复(对齐 loop_.rs:160-168 的 interval 冷却)
                self.inner.remove(workspace_id);
                false
            }
            Some(_) => true,
        }
    }

    pub fn resume(&self, workspace_id: &str) {
        self.inner.remove(workspace_id);
    }

    pub fn consecutive_failures(&self, workspace_id: &str) -> u32 {
        self.inner.get(workspace_id).map(|e| e.failures).unwrap_or(0)
    }
}
```

- [ ] **Step 4: 跑确认通过** — Run: `cargo test -p agent governor` Expected: 2 PASS
- [ ] **Step 5: Commit** — `git add crates/agent/src/runtime/thing_agent/ && git commit -m "feat(agent): HeartbeatGovernor 连败治理——统一循环的心跳暂停语义"`

### Task 5: HeartbeatTrigger(行为保持清单第 4 行:每工作区可配间隔)

**Files:**
- Create: `crates/agent/src/runtime/thing_agent/trigger/heartbeat.rs`
- Modify: `crates/agent/src/runtime/thing_agent/trigger/mod.rs`(导出)
- Test: 同文件 mod tests(照 timer.rs 测试模板:pause/advance/StubAutonomyPolicyReader)

**Interfaces:**
- Consumes: `TriggerSource::Heartbeat`(Task 3)、`HeartbeatGovernor`(Task 4)、`AutonomyPolicyReader`(现成 traits.rs)
- Produces:

```rust
/// 周期心跳触发器:每工作区可配间隔(intervals 表每轮重读,
/// 对齐 interval_minutes 热改);policy mode=off 抑制(O6/O19);
/// governor 暂停抑制(连败治理);reload_notify 到达立即 tick
/// (行为保持清单第 3 行:ReloadTasks/ReloadConfig 写穿+立即 tick,
/// 对齐 loop_.rs:146-150);dedup heartbeat:{ws}。
pub struct HeartbeatTrigger {
    pub workspace_id: String,
    pub default_interval: Duration,
    /// 每工作区间隔(分钟)共享表——cloud 侧 update_config 写穿(D11-⑤ 写序)。
    pub intervals: Arc<dashmap::DashMap<String, u32>>,
    /// 任务表共享内存源(空任务/全暂停不 tick,对齐 loop_.rs:63)。
    pub tasks_source: Arc<dashmap::DashMap<String, Vec<HeartbeatTask>>>,
    pub policy_repo: Arc<dyn AutonomyPolicyReader>,
    pub governor: HeartbeatGovernor,
    /// 热更新信号:handler update_tasks/update_config 写穿 DB+内存表后 notify,
    /// trigger 立即醒来一次(数据已在共享表,notify 只承担"立即 tick")。
    pub reload_notify: Arc<tokio::sync::Notify>,
}
```

- [ ] **Step 1: 写失败测试**(照 timer.rs:105-170 模式;`stub_repo`/`spawn_trigger` 助手从 timer.rs tests 复制改造,`seeded_tasks_source` 见下)

测试文件内共享夹具:

```rust
fn seeded_tasks_source(ws: &str) -> Arc<dashmap::DashMap<String, Vec<HeartbeatTask>>> {
    let m = dashmap::DashMap::new();
    m.insert(ws.to_string(), vec![HeartbeatTask {
        id: 1,
        workspace_id: ws.into(),
        priority: "high".into(),
        text: "check devices".into(),
        paused: false,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }]);
    Arc::new(m)
}
```

三个测试:

```rust
#[tokio::test]
async fn emits_heartbeat_signal_with_workspace_dedup_key() {
    pause();
    let repo = stub_repo("ws_01", AutonomyMode::Act);
    let governor = HeartbeatGovernor::default();
    let trigger = HeartbeatTrigger {
        workspace_id: "ws_01".into(),
        default_interval: Duration::from_secs(60),
        intervals: Arc::new(dashmap::DashMap::new()),
        tasks_source: seeded_tasks_source("ws_01"),
        policy_repo: repo,
        governor,
        reload_notify: Arc::new(tokio::sync::Notify::new()),
    };
    let (tx, mut rx) = mpsc::channel(16);
    let handle = tokio::spawn(async move { trigger.run(tx).await });

    // 启动即首 tick(对齐 loop_.rs:56-110 先执行后等待的顺序)
    let first = rx.recv().await.expect("startup tick fires");
    assert!(matches!(first.source, TriggerSource::Heartbeat { alarm: None }));
    assert_eq!(first.dedup_key.as_deref(), Some("heartbeat:ws_01"));
    assert_eq!(first.priority, Priority::Normal);

    drop(rx);
    handle.await.expect("run task").expect("run result");
}

#[tokio::test]
async fn per_workspace_interval_override_wins_over_default() {
    pause();
    let repo = stub_repo("ws_02", AutonomyMode::Act);
    let intervals = Arc::new(dashmap::DashMap::new());
    intervals.insert("ws_02".to_string(), 5u32); // 300s 覆盖 default 3600s
    let trigger = HeartbeatTrigger {
        workspace_id: "ws_02".into(),
        default_interval: Duration::from_secs(3600),
        intervals,
        tasks_source: seeded_tasks_source("ws_02"),
        policy_repo: repo,
        governor: HeartbeatGovernor::default(),
        reload_notify: Arc::new(tokio::sync::Notify::new()),
    };
    let (tx, mut rx) = mpsc::channel(16);
    let handle = tokio::spawn(async move { trigger.run(tx).await });

    let first = rx.recv().await.expect("startup tick");
    assert_eq!(first.dedup_key.as_deref(), Some("heartbeat:ws_02"));

    // 步进泵(timer.rs:155-163 模式,暂停时钟下确定性强):
    // 前 299s 不得有第二 tick;累计 300s 必须到点。
    for i in 1..=299 {
        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "第 {i}s 不应有 tick(覆盖间隔=300s)");
    }
    advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    let second = rx.try_recv().expect("第 300s 到点出 tick");
    assert_eq!(second.dedup_key.as_deref(), Some("heartbeat:ws_02"));

    drop(rx);
    handle.await.expect("run task").expect("run result");
}

#[tokio::test]
async fn paused_governor_suppresses_ticks_until_resume() {
    pause();
    let repo = stub_repo("ws_03", AutonomyMode::Act);
    let governor = HeartbeatGovernor::default();
    for _ in 0..5 { governor.record_failure("ws_03"); }
    let trigger = HeartbeatTrigger {
        workspace_id: "ws_03".into(),
        default_interval: Duration::from_secs(60),
        intervals: Arc::new(dashmap::DashMap::new()),
        tasks_source: seeded_tasks_source("ws_03"),
        policy_repo: repo,
        governor: governor.clone(),
        reload_notify: Arc::new(tokio::sync::Notify::new()),
    };
    let (tx, mut rx) = mpsc::channel(16);
    let handle = tokio::spawn(async move { trigger.run(tx).await });

    // paused:启动检查与 +60s 检查都被抑制(泵 70s 无信号)
    let mut got = None;
    for _ in 0..70 {
        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        if let Ok(s) = rx.try_recv() { got = Some(s); break; }
    }
    assert!(got.is_none(), "paused 工作区不得发 tick");

    // resume 后下一轮(+120s 处)出信号
    governor.resume("ws_03");
    let mut got = None;
    for _ in 0..70 {
        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        if let Ok(s) = rx.try_recv() { got = Some(s); break; }
    }
    assert_eq!(got.expect("resume 后恢复 tick").dedup_key.as_deref(), Some("heartbeat:ws_03"));

    drop(rx);
    handle.await.expect("run task").expect("run result");
}
```

另加空任务抑制测试(对齐 loop_.rs:63 空任务列表不 tick):

```rust
#[tokio::test]
async fn empty_or_all_paused_tasks_emit_nothing() {
    pause();
    let repo = stub_repo("ws_04", AutonomyMode::Act);
    let trigger = HeartbeatTrigger {
        workspace_id: "ws_04".into(),
        default_interval: Duration::from_secs(60),
        intervals: Arc::new(dashmap::DashMap::new()),
        tasks_source: Arc::new(dashmap::DashMap::new()), // 无任务
        policy_repo: repo,
        governor: HeartbeatGovernor::default(),
        reload_notify: Arc::new(tokio::sync::Notify::new()),
    };
    let (tx, mut rx) = mpsc::channel(16);
    let handle = tokio::spawn(async move { trigger.run(tx).await });
    for _ in 0..70 {
        advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "空任务工作区不发 tick(loop_.rs:63)");
    }
    drop(rx);
    handle.await.expect("run task").expect("run result");
}
```

- [ ] **Step 2: 跑确认失败** — `cargo test -p agent heartbeat_trigger` Expected: FAIL

- [ ] **Step 3: 实现 trigger/heartbeat.rs**(发射优先——对齐 loop_.rs 先 tick 后等待;每轮重读间隔/policy/governor/任务表):

```rust
async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()> {
    loop {
        // 空任务/全暂停 = 不 tick(loop_.rs:63 语义)
        let has_active_tasks = self
            .tasks_source
            .get(&self.workspace_id)
            .map(|ts| ts.iter().any(|t| !t.paused))
            .unwrap_or(false);

        // policy kill switch(O6/O19,fail-closed)
        let policy_ok = match self.policy_repo.load_autonomy(&self.workspace_id).await {
            Ok(Some(policy)) => policy.mode != AutonomyMode::Off,
            Ok(_) => false,
            Err(e) => {
                tracing::warn!(workspace_id = %self.workspace_id, error = %e,
                    "autonomy policy read failed — skipping heartbeat tick");
                false
            }
        };

        // 每轮重读间隔(interval_minutes 热改生效)
        let interval = self
            .intervals
            .get(&self.workspace_id)
            .map(|m| Duration::from_secs((*m).max(MIN_HEARTBEAT_INTERVAL_MINUTES) as u64 * 60))
            .unwrap_or(self.default_interval);

        if has_active_tasks && policy_ok && !self.governor.is_paused(&self.workspace_id, interval) {
            let signal = WakeSignal {
                workspace_id: self.workspace_id.clone(),
                priority: Priority::Normal,
                source: TriggerSource::Heartbeat { alarm: None },
                dedup_key: Some(format!("heartbeat:{}", self.workspace_id)),
            };
            if tx.send(signal).await.is_err() {
                return Ok(()); // receiver dropped — agent loop shut down
            }
        }
        // 等待下一轮:interval 到点 OR reload_notify 立即醒(热更新立即 tick,
        // 对齐 loop_.rs ReloadConfig"forces an immediate tick"语义)。
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = self.reload_notify.notified() => {
                tracing::info!(workspace_id = %self.workspace_id, "heartbeat reload — immediate tick");
            }
        }
    }
}
```

热更新测试(行为保持清单第 3 行):

```rust
#[tokio::test]
async fn reload_notify_forces_immediate_tick() {
    pause();
    let repo = stub_repo("ws_05", AutonomyMode::Act);
    let reload = Arc::new(tokio::sync::Notify::new());
    let trigger = HeartbeatTrigger {
        workspace_id: "ws_05".into(),
        default_interval: Duration::from_secs(3600), // 长间隔:不靠到点
        intervals: Arc::new(dashmap::DashMap::new()),
        tasks_source: seeded_tasks_source("ws_05"),
        policy_repo: repo,
        governor: HeartbeatGovernor::default(),
        reload_notify: reload.clone(),
    };
    let (tx, mut rx) = mpsc::channel(16);
    let handle = tokio::spawn(async move { trigger.run(tx).await });

    let _first = rx.recv().await.expect("startup tick");
    reload.notify_one(); // update_config/update_tasks 写穿后调用
    let second = rx.recv().await.expect("notify 立即 tick,不等 3600s");
    assert_eq!(second.dedup_key.as_deref(), Some("heartbeat:ws_05"));

    drop(rx);
    handle.await.expect("run task").expect("run result");
}
```

- [ ] **Step 4: 跑确认通过** — `cargo test -p agent heartbeat_trigger` Expected: 3 PASS
- [ ] **Step 5: Commit** — `git commit -m "feat(agent): HeartbeatTrigger——每工作区间隔+policy 抑制+governor 暂停"`

### Task 6: 统一循环接线——manager 装配 Heartbeat 源 run

**Files:**
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs`(PipelineDeps + start() + run_pipeline)
- Modify: `crates/agent/src/runtime/thing_agent/traits.rs`(新增两个 reader trait)
- Modify: `crates/agent/src/prompt/mod.rs` 或 manager 内 build_prompt(Heartbeat arm)
- Test: `crates/agent/src/runtime/thing_agent/manager.rs` mod tests(ScriptedProvider 现成)

**Interfaces:**
- Consumes: Task 3/4/5
- Produces: `PipelineDeps` 新增字段 `heartbeat_tasks: Arc<dyn HeartbeatTaskReader>`、`trust_configs: Arc<dyn TrustConfigReader>`、`governor: HeartbeatGovernor`;`trait HeartbeatTaskReader { fn active_tasks(&self, ws: &str) -> Vec<HeartbeatTask>; }`、`trait TrustConfigReader { fn get(&self, ws: &str) -> TrustConfig; }`(同步读共享内存表,无 async——对齐 loop_.rs:61 parking_lot 同步读)

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn heartbeat_signal_produces_run_report_and_governor_records_outcome() {
    // ScriptedProvider 回固定 heartbeat JSON(status/summary/proposals);
    // enqueue WakeSignal{Heartbeat, dedup heartbeat:ws} → 等 RunRecorded
    // 断言:report.trigger=="heartbeat:ws_01"、governor.consecutive_failures==0
    // 再让 provider 失败一次 → failures==1
}

#[tokio::test]
async fn heartbeat_prompt_contains_tasks_and_trust() {
    // ScriptedProvider.prompts 记录装配后的 prompt;
    // 断言含任务文本(仿 loop_.rs build_heartbeat_prompt 的内容)与信任段
}
```

- [ ] **Step 2: 跑确认失败** — `cargo test -p agent heartbeat_signal` Expected: FAIL

- [ ] **Step 3: 实现**

a) traits.rs 加两个 reader trait + DashMap 的 blanket impl:

```rust
pub trait HeartbeatTaskReader: Send + Sync {
    fn active_tasks(&self, workspace_id: &str) -> Vec<tinyiothub_core::heartbeat::HeartbeatTask>;
}
pub trait TrustConfigReader: Send + Sync {
    fn get(&self, workspace_id: &str) -> tinyiothub_core::heartbeat::TrustConfig;
}

impl HeartbeatTaskReader for dashmap::DashMap<String, Vec<tinyiothub_core::heartbeat::HeartbeatTask>> {
    fn active_tasks(&self, workspace_id: &str) -> Vec<tinyiothub_core::heartbeat::HeartbeatTask> {
        self.get(workspace_id)
            .map(|ts| ts.iter().filter(|t| !t.paused).cloned().collect())
            .unwrap_or_default()
    }
}
impl TrustConfigReader for dashmap::DashMap<String, tinyiothub_core::heartbeat::TrustConfig> {
    fn get(&self, workspace_id: &str) -> tinyiothub_core::heartbeat::TrustConfig {
        self.get(workspace_id).map(|r| r.clone()).unwrap_or_default()
    }
}
```

b) build_prompt 加 Heartbeat arm(manager.rs build_prompt 内,Timer arm 旁):

```rust
TriggerSource::Heartbeat { alarm } => {
    let tasks = deps.heartbeat_tasks.active_tasks(&signal.workspace_id);
    let trust = deps.trust_configs.get(&signal.workspace_id);
    let mut p = crate::prompt::heartbeat::build_heartbeat_prompt(
        &signal.workspace_id, &principles, &tasks.iter().collect::<Vec<_>>(), &trust,
    );
    if let Some(a) = alarm {
        // 告警驱动 run:附上触发上下文(现状 External 信号只唤醒不带上下文——
        // 此处是有意改进,审计链 trigger_label 已含告警身份)
        p.push_str(&format!(
            "\n\n## 触发原因(告警)\n- 原因:{}\n- 设备:{}\n- 告警类型:{}\n- 上下文:{}",
            a.reason,
            a.thing_id.as_deref().unwrap_or("-"),
            a.alarm_type.as_deref().unwrap_or("-"),
            a.context
        ));
    }
    p
}
```

c) run_pipeline 末尾(deliver 前)接 governor:

```rust
if matches!(signal.source, TriggerSource::Heartbeat { .. }) {
    match report.outcome {
        tinyiothub_core::agent_runs::Outcome::Failed => deps.governor.record_failure(&ws),
        _ => deps.governor.record_success(&ws),
    }
}
```

d) manager.start() 加 heartbeat trigger 任务(TimerTrigger 旁):

```rust
let heartbeat = HeartbeatTrigger {
    workspace_id: ws.clone(),
    default_interval: self.config.heartbeat_default_interval,
    intervals: Arc::clone(&self.deps.heartbeat_intervals),
    tasks_source: Arc::clone(&self.deps.heartbeat_tasks_map),
    policy_repo: Arc::clone(&self.deps.policy_repo),
    governor: self.deps.governor.clone(),
};
```

(PipelineDeps 相应持有 `heartbeat_intervals: Arc<DashMap<String,u32>>`、`heartbeat_tasks_map: Arc<DashMap<String, Vec<HeartbeatTask>>>`、governor——由 cloud bootstrap 注入,与 handler 写穿共享同一张表,D11-⑤ 写序不变。)

- [ ] **Step 4: 跑确认通过** — `cargo test -p agent` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "feat(agent): 统一循环接 Heartbeat 源——prompt 装配+governor 记账+trigger 挂载"`

### Task 7: RunRecorded 重试对齐(D10)

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/persist.rs:385-418`(project_run)
- Test: `apps/cloud/src/tests/agent_persist_tests.rs`

**Interfaces:**
- Consumes: `spawn_heartbeat_retry`(persist.rs:555+)的指数退避+shutdown→DLQ 模式
- Produces: `spawn_run_retry(pool, publisher, report, problem_key, dedup_key, shutdown)`——RunRecorded 落库失败不再直入 DLQ,先重试(对齐心跳);重试被 shutdown 中止/耗尽→DLQ(现状 DLQ 行为保持)

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn run_recorded_persist_retries_before_dlq() {
    // 故障注入 pool(首次 insert 失败、第二次成功);
    // 断言:第二次尝试后 agent_runs 有行、DLQ 空。
    // 现有 heartbeat retry 测试(同文件)是模板——仿其故障注入夹具。
}
```

- [ ] **Step 2: 跑确认失败** — `cargo test -p tinyiothub-cloud run_recorded_persist_retries` Expected: FAIL(现行为直入 DLQ)

- [ ] **Step 3: 实现**——抽公共重试骨架(heartbeat 与 run 共用,泛化 payload 类型),`project_run` 的 insert 失败分支改调 `spawn_run_retry`;重试成功路径复用 `insert_agent_run` 幂等(UNIQUE=成功)。

- [ ] **Step 4: 跑确认通过** — `cargo test -p tinyiothub-cloud persist` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "fix(cloud): RunRecorded 补心跳同款重试(D10)——归一不降级可靠性"`

### Task 8: persist 心跳支路——提案从 RunReport 落 agent_actions

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/persist.rs`(project 加 heartbeat 支路)
- Modify: `crates/agent/src/runtime/heartbeat/report.rs`(parse_healing_report 改 `pub`)
- Test: `apps/cloud/src/tests/agent_persist_tests.rs`

**Interfaces:**
- Produces: `project()` 的 RunRecorded 支路内,`trigger_type_of(&report.trigger) == "heartbeat"` 时:
  1. `parse_healing_report(&report.summary, &report.workspace_id)` 取 proposals;
  2. proposal 行写 agent_actions,`tick_id = report.run_id`(C3 联结);
  3. problem_key 就地刷新逻辑复用(heartbeat.rs:420-460 的 SELECT+UPDATE 块抽成 `pub(crate) async fn upsert_proposal_rows(tx_or_pool, ws, agent_id, proposals, tick_id)`);
  4. `outcome == Failed` 时补 G3 system_error 事件(对齐 persist.rs:303-325)
- **Shadow 门控(spec「统一 Run 路径只产报告不落库/不落提案」的实现点)**:以上 heartbeat 支路整体挂在 `ShadowFlags.heartbeat_unified_persist`(默认 false)后——双跑期统一路径的 RunRecorded 只进 shadow observer(Task 13),不落 agent_runs 也不落提案;切换期(T15)翻旗删除门控

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn heartbeat_run_recorded_writes_proposal_rows_with_run_id_anchor() {
    // RunReport{trigger:"heartbeat:ws1", summary: 含 proposals JSON, ...}
    // → project(RunRecorded) → agent_actions 有 proposal 行且 tick_id==report.run_id
    // 同 problem_key 再来一条 → 原地刷新不新增(既有折叠语义)
}

#[tokio::test]
async fn failed_heartbeat_run_emits_system_error_event() {
    // outcome=Failed 的 heartbeat run → events 表有 system_error 行(G3 闭环 parity)
}
```

- [ ] **Step 2: 跑确认失败** — Expected: FAIL
- [ ] **Step 3: 实现**(按 Interfaces 四步;`insert_heartbeat_result` 的提案块抽函数后,旧路径与新支路共用——抽函数时旧路径测试必须仍绿)
- [ ] **Step 4: 跑确认通过** — `cargo test -p tinyiothub-cloud` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "feat(cloud): heartbeat run 提案落 agent_actions——tick_id 位写 run_id(C3)"`

### Task 9: 迁移三件套(trace 回填 → tick_id 改写 → 读侧切换)

**Files:**
- Create: `crates/db/migrations/YYYYMMDDHHMMSS_heartbeat_runs_backfill.sql`
- Modify: `crates/db/src/lib.rs`(touch 强制重建——sqlx 内嵌)
- Test: `crates/db/tests/heartbeat_migration_tests.rs`(新建)

**Interfaces:**
- 迁移语义(顺序即埋伏③):
  1. 每个 anchor 行(action_type IN ('summary','error') AND agent_id LIKE '__heartbeat__:%')合成 agent_runs 行:`run_id='hbm_'||tick_id`、`trigger='heartbeat:'||workspace_id`、`outcome`(error→failed,summary→acted 由 content.status 映射)、`summary`=content.result/error、`report`=含 trace 的 JSON(有 trace 才填,无则空数组——"历史无 trace 不补");
  2. `UPDATE agent_actions SET tick_id='hbm_'||tick_id WHERE agent_id LIKE '__heartbeat__:%' AND tick_id IS NOT NULL AND action_type IN ('proposal','auto_executed')` 且目标 run 行存在;
  3. 读侧兼容视图:新建 VIEW `heartbeat_tick_map`(tick_id, run_id)供过渡查询;切换完成后删(下版本)。

- [ ] **Step 1: 写失败测试**

```rust
#[tokio::test]
async fn migration_backfills_runs_then_relinks_proposals() {
    // 夹具:手工插 anchor(summary 含 trace)+ 1 proposal(tick_id=t1)
    // 跑迁移 → agent_runs 有 hbm_t1 行且 report 含 trace;
    //         proposal.tick_id == "hbm_t1";
    //         heartbeat_tick_map 视图可查 t1→hbm_t1;
    // 迁移重跑幂等(INSERT OR IGNORE / WHERE NOT EXISTS)
}

#[tokio::test]
async fn migration_skips_trace_less_anchor_gracefully() {
    // 无 trace 的 anchor:仍合成行(trace=[]),前端"无过程记录"由读侧决定
}
```

- [ ] **Step 2: 跑确认失败** — `cargo test -p db migration` Expected: FAIL(迁移不存在)
- [ ] **Step 3: 写迁移 SQL**(INSERT OR IGNORE + 分两步,顺序注释标明埋伏③)
- [ ] **Step 4: 跑确认通过 + `touch crates/db/src/lib.rs` 后全量** — `cargo test -p db` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "feat(db): 心跳历史迁移——trace 回填合成 agent_runs+提案改锚 run_id(埋伏③顺序)"`

### Task 10: 读侧兼容视角——get_logs 改读 agent_runs

**Files:**
- Modify: `apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs:267-389`(get_logs)
- Create: `apps/cloud/src/domains/agent/host/handler/execution_record.rs`(适配器纯函数)
- Test: `apps/cloud/src/tests/`(handler 级 + 纯函数单测)

**Interfaces:**
- Produces:

```rust
/// RunReport→ExecutionRecord 纯函数适配层(C6:HeartbeatResult 的读侧兼容视角)。
/// 历史无 trace 行:trace=[] → 前端显示"无过程记录"不炸。
pub fn run_row_to_execution_record(
    run_id: &str, trigger: &str, outcome: &str, summary: &str,
    report_json: &serde_json::Value, created_at: &str,
) -> ExecutionRecord;
```

- get_logs 查询改:`SELECT ... FROM agent_runs WHERE workspace_id=? AND trigger LIKE 'heartbeat:%' ORDER BY created_at DESC LIMIT 50`;proposal/auto_executed 明细仍 join agent_actions(tick_id=run_id)——明细数据源不变

- [ ] **Step 1: 写失败测试**(适配器单测:有 trace/无 trace/error 行三态;handler 测试:切换后 get_logs 返回新源数据)
- [ ] **Step 2: 跑确认失败** — Expected: FAIL
- [ ] **Step 3: 实现适配器 + 切换查询**(保留旧 agent_actions 路径一个 feature-flag-free 的 fallback:agent_runs 空且 agent_actions 有 anchor 时回旧查询——兼容期双读,切换完成后删)
- [ ] **Step 4: 跑确认通过** — `cargo test -p tinyiothub-cloud` Expected: 全绿(含任务分类清单里标「实现细节重写」的三个文件已按新落库改写完)
- [ ] **Step 5: Commit** — `git commit -m "feat(cloud): get_logs 读侧切换 agent_runs——RunReport→ExecutionRecord 纯函数适配(C6)"`

### Task 11: 告警链接入统一循环(N3)

**Files:**
- Modify: `crates/agent/src/runtime/orchestrator/callbacks.rs`(HeartbeatBridge)
- Test: `crates/agent/src/runtime/orchestrator/callbacks.rs` 或 heartbeat_bridge_tests(分类清单已标重写)

**Interfaces:**
- Consumes: Task 3 变体 + Task 4 governor
- Produces: HeartbeatBridge 内部由 `HeartbeatRunner::signal(HeartbeatSignal)` 改为向 `DirectiveSink` enqueue:

```rust
// HeartbeatSignal → WakeSignal 转换(优先级映射 N3:Critical→Critical,High/Normal→Normal)
fn to_wake_signal(s: &HeartbeatSignal) -> WakeSignal {
    let priority = match s.priority {
        SignalPriority::Critical => Priority::Critical,   // 绕过 merge window 与节流(scheduler ②)
        SignalPriority::High | SignalPriority::Normal => Priority::Normal,
    };
    let dedup_key = match s.dedup_key() {
        Some((thing, alarm)) => format!("heartbeat:{}:{}:{}", s.workspace_id, thing, alarm),
        None => format!("heartbeat:{}", s.workspace_id),
    };
    WakeSignal {
        workspace_id: s.workspace_id.clone(),
        priority,
        source: TriggerSource::Heartbeat {
            alarm: Some(HeartbeatAlarmContext {
                thing_id: s.thing_id.clone(),
                alarm_type: s.alarm_type.clone(),
                rule_id: s.rule_id.clone(),
                reason: s.reason.clone(),
                context: s.context.clone(),
            }),
        },
        dedup_key: Some(dedup_key),
    }
}
```

enqueue 前 `governor.resume(&s.workspace_id)`(外部信号恢复暂停——对齐 loop_.rs:128-133)。

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn alarm_signal_maps_priority_and_distinct_dedup_key() {
    // Critical→Critical;High→Normal;dedup=heartbeat:ws:d1:offline;
    // 与定时 tick 的 heartbeat:ws 不互合并(N3:Critical 告警不得被在途 tick 吞)
}
#[tokio::test]
async fn alarm_signal_resumes_paused_governor_and_enqueues() {
    let governor = HeartbeatGovernor::default();
    for _ in 0..5 { governor.record_failure("ws1"); }
    assert!(governor.is_paused("ws1", Duration::from_secs(3600)));

    // RecordingSink:实现 `DirectiveSink` 的测试桩,enqueue 记录 WakeSignal
    let sink = Arc::new(RecordingSink::default());
    let bridge = HeartbeatBridge::new(sink.clone(), governor.clone());
    bridge.dispatch(&sample_alarm_signal("ws1")).await;

    assert!(
        !governor.is_paused("ws1", Duration::from_secs(3600)),
        "告警信号恢复暂停(loop_.rs:128-133 语义)"
    );
    let sig = sink.last().expect("wake signal enqueued");
    assert_eq!(sig.priority, Priority::Critical);
    assert_eq!(sig.dedup_key.as_deref(), Some("heartbeat:ws1:d1:offline"));
}
```

测试夹具(放同文件 tests mod):

```rust
/// 字段照 loop_.rs:443-452 的 sample_signal;带 thing_id/alarm_type 走分键路径。
fn sample_alarm_signal(ws: &str) -> HeartbeatSignal {
    HeartbeatSignal {
        workspace_id: ws.into(),
        reason: "设备离线".into(),
        context: "{}".into(),
        priority: SignalPriority::Critical,
        thing_id: Some("d1".into()),
        alarm_type: Some("offline".into()),
        rule_id: Some("r1".into()),
    }
}

#[derive(Default)]
struct RecordingSink {
    signals: std::sync::Mutex<Vec<WakeSignal>>,
}
impl RecordingSink {
    fn last(&self) -> Option<WakeSignal> {
        self.signals.lock().unwrap().last().cloned()
    }
}
impl crate::runtime::thing_agent::traits::DirectiveSink for RecordingSink {
    fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
        self.signals.lock().unwrap().push(signal);
        Ok(())
    }
    async fn drain(&self, _workspace_id: &str) {}
}
```

- [ ] **Step 2: 跑确认失败** — Expected: FAIL
- [ ] **Step 3: 实现**(bridge 构造点换依赖:HeartbeatRunner→DirectiveSink+HeartbeatGovernor;bootstrap 接线在 Task 13)
- [ ] **Step 4: 跑确认通过** — `cargo test -p agent` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "feat(agent): 告警链转 Heartbeat 变体入统一循环——优先级映射+分键 dedup(N3)"`

### Task 12: last_tick D13 实时读透出

**Files:**
- Modify: `crates/agent/src/runtime/thing_agent/manager.rs`(run_pipeline heartbeat 支路写 last_ticks)
- Modify: `apps/cloud/src/domains/agent/host/handler/workspace_heartbeat.rs`(get_config/状态读源切换)
- Test: handler 测试不变(行为保持清单第 2 行——测试不该变,变的是内存态来源)

**Interfaces:**
- Produces: PipelineDeps 加 `last_ticks: Arc<DashMap<String, chrono::DateTime<chrono::Utc>>>`;heartbeat 源 run 完成(成败均计)写 `last_ticks.insert(ws, now)`——对齐 loop_.rs:74-75

- [ ] **Step 1: 写失败测试**(统一循环 heartbeat run 完成后 last_ticks 有值;handler 读到的 last_tick 与 run 完成时间一致)
- [ ] **Step 2: 跑确认失败**
- [ ] **Step 3: 实现**(两行插入 + handler 读源换注入表)
- [ ] **Step 4: 跑确认通过**——**handler 测试零修改通过是本任务的验收**
- [ ] **Step 5: Commit** — `git commit -m "feat(agent): 统一循环透出 last_tick 内存态(D13)——handler 测试不变"`

### Task 13: bootstrap 接线 + 双跑装配

**Files:**
- Modify: `apps/cloud/src/bootstrap.rs`(HeartbeatRunner→ThingAgentManager 依赖注入切换)
- Modify: `apps/cloud/src/shared/app_state.rs`(heartbeat_runner 字段保留至 Task 15,先并存)
- Test: `apps/cloud/src/tests/agent_startup_tests.rs`

**Interfaces:**
- Produces: cloud 启动时:heartbeat 任务表/trust 表/intervals 表/last_ticks 表/governor 注入 ThingAgentManager;HeartbeatBridge 换接 DirectiveSink(Task 11);**`ThingAgentManager` 持每工作区 `Arc<Notify>` 表并暴露 `pub fn heartbeat_reload_notify(&self, ws: &str) -> Arc<tokio::sync::Notify>`;update_tasks/update_config handler 在写穿 DB+内存表后调它**(行为保持清单第 3 行的接线点);HeartbeatRunner 仍启动(双跑期)
- **Shadow observer(spec「比对期 N=7 天或 M=50 次 tick」的生产观测面)**:新增 persist 并列订阅者 `shadow_observer`:同时订阅 HeartbeatResultReady(旧路径结果)与 RunRecorded(trigger=heartbeat:%,新路径报告),按 workspace 配对后比对白名单(status/summary/proposals 集合/trace 工具名序列),不一致打 `warn!`(带字段级 diff)并递增 `shadow_mismatches` 计数(Metrics);生产双跑期靠日志/计数观察等价性。**真实 LLM 下两路径输出天然有方差**——observer 比对的是结构白名单(状态映射/proposals 工具名集合/trace 工具名序列),summary 文本只记长度级差异提示,不做逐字断言(逐字等价由 Task 14 的确定性 harness 负责)

- [ ] **Step 1: 写失败测试**(启动后 manager 持有 heartbeat deps;workspace start 后 heartbeat trigger 任务在 WorkspaceLoop.tasks 中)
- [ ] **Step 2: 跑确认失败**
- [ ] **Step 3: 实现接线**
- [ ] **Step 4: 跑确认通过** — `cargo test -p tinyiothub-cloud startup` Expected: 全绿
- [ ] **Step 5: Commit** — `git commit -m "feat(cloud): bootstrap 双跑接线——统一循环持 heartbeat 依赖,旧 runner 并存"`

### Task 14: shadow 双跑等价对照(Q1/F2)

**Files:**
- Create: `crates/agent/tests/heartbeat_shadow_equiv.rs`
- Test: 即本体

**Interfaces:**
- 比对 harness(确定性接缝):两路径共用同一 scripted LLM 输出源;
  - 旧路径:`run_heartbeat_tick`(loop_.rs:174)配 MockPool(loop_.rs:235 模板)
  - 新路径:manager run_pipeline 配 ScriptedProvider(manager.rs:493 模板)
  - 白名单比对:status / summary / proposals 集合 / trace 工具名序列
  - 统一路径只产报告:断言 agent_runs 无落库、agent_actions 无提案(shadow 期不落)
  - `run_old_tick_with_mock`/`run_new_pipeline_with_script` 是本测试文件内定义的本地助手(分别包装上述两个模板夹具的调用+取结果),非既有函数

```rust
#[tokio::test]
async fn shadow_equiv_status_summary_proposals_trace() {
    let llm_output = r#"{"status":"complete","summary":"一切正常","proposals":[]}"#;
    let old = run_old_tick_with_mock(llm_output).await;   // HeartbeatResult
    let new = run_new_pipeline_with_script(llm_output).await; // RunReport
    assert_eq!(old.status.to_string(), map_outcome_to_status(&new.report));
    assert_eq!(old.summary, new.report.summary /* 或 parse 后等价 */);
    assert_eq!(
        old.proposals.iter().map(|p| &p.tool_name).collect::<Vec<_>>(),
        parse_healing_report(&new.report.summary, "ws").proposals.iter().map(|p| &p.tool_name).collect::<Vec<_>>()
    );
    assert_eq!(
        old.trace.iter().map(|t| &t.name).collect::<Vec<_>>(),
        new.report.trace.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 1: 写对照测试(先红——新路径在 shadow 期不落库断言就位)**
- [ ] **Step 2: 跑确认失败/通过调平**——两路径输出差异调齐到白名单一致(差异即归一引入的行为漂移,逐个对齐或记录为有意变更)
- [ ] **Step 3: 跑全量** — `cargo test -p agent --test heartbeat_shadow_equiv` Expected: PASS
- [ ] **Step 4: Commit** — `git commit -m "test(agent): shadow 双跑等价对照——白名单 status/summary/proposals/trace"`

注:spec 的「比对期 N=7 天或 M=50 次 tick 后切换」指**部署后**生产双跑观察期——由运维节奏执行,不是本计划的代码任务;切换判据达标后才进 Task 15。

### Task 15: HeartbeatRunner 退役(切换判据达标后)

**Files:**
- Delete: `crates/agent/src/runtime/heartbeat/loop_.rs`、`runner.rs`(heartbeat 的)
- Modify: `crates/agent/src/runtime/heartbeat/mod.rs`(保留 types/report/metrics 中被引用的部分)
- Modify: `apps/cloud/src/shared/app_state.rs`(heartbeat_runner 字段删除)、`bootstrap.rs`(含 shadow observer 与 `ShadowFlags.heartbeat_unified_persist` 翻旗为 true 后删门控)
- Modify: `apps/cloud/src/domains/agent/host/persist.rs`(HeartbeatResultReady 支路+spawn_heartbeat_retry 删除;AgentEventKind::HeartbeatResultReady 删除——事件枚举精简;heartbeat 支路的 shadow 门控删除,Task 8 代码转为唯一路径)

**Interfaces:**
- 前置:Task 14 生产双跑 N=7天/M=50 tick 达标;get_logs 读侧已切(Task 10)
- 删除前 grep 确认零引用:`grep -rn "HeartbeatResultReady\|heartbeat_runner\|HeartbeatRunner" --include="*.rs" apps/ crates/`

- [ ] **Step 1: 删除 + 编译器领着清引用**(`cargo check` 迭代到干净)
- [ ] **Step 2: 跑全量门禁** — `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` Expected: 全绿
- [ ] **Step 3: Commit** — `git commit -m "refactor(agent): HeartbeatRunner/heartbeat_loop 退役——统一 Run 循环接管"`

### Task 16: host/ 模块分组(20+ 平铺→按概念归位)

**Files:**
- Move(纯移动,零行为变更):
  - `host/judge/` ← judgment_subscriber.rs + policy_engine.rs
  - `host/notify/` ← ticket_subscriber.rs + 通知装配
  - `host/persist/` ← persist.rs + dlq_repo.rs + memory/
  - `host/trigger/` ← directive_sink.rs + thing_action_hooks.rs + agent_hooks.rs
  - `host/chat/` ← domains/agent/chat 收进(并掉两处之一)
  - `host/tools/` ← 不动
  - host 壳 ← thing_agent_host.rs + service.rs + scaffold.rs + pool_adapter.rs
- Modify: 每组 `mod.rs` 写三行契约(做什么/谁调用/依赖谁);RunRegistry vs ActiveRunRegistry 关系注记放 persist/或壳的 mod.rs(eng S1-P3)
- Test: 无新测试——移动后全量套件绿即验收

- [ ] **Step 1: git mv + mod.rs 契约 + use 路径全改**(编译器领着走)
- [ ] **Step 2: 全量门禁绿**
- [ ] **Step 3: Commit** — `git commit -m "refactor(cloud): host/ 按概念分组——judge/notify/persist/trigger/chat+三行契约"`

### Task 17: O7 立即生效回归测试(eng T11)

**Files:**
- Test: `apps/cloud/src/domains/agent/host/policy_engine.rs` mod tests 或 `apps/cloud/src/tests/`

- [ ] **Step 1: 写测试**

```rust
#[tokio::test]
async fn policy_reads_are_fresh_per_evaluate_kill_switch_immediate() {
    // O7/R1 锁定:evaluate 每次新鲜读档——Act→允许;改 Diagnose/Off 后
    // 下一次 evaluate 立即拒(无任何缓存/快照窗口)。
    // 防有人把读档「优化」成 run 触发时快照。
    let engine = /* SqlitePolicyEngine on test pool */;
    // save Act → evaluate(reboot) != Block/RequireApproval(按现语义)
    // flip to Off → evaluate(review) 立即 deny
}
```

- [ ] **Step 2: 跑确认通过**(现行为即如此——这是锁定测试,一次绿;若红说明 seam 已被改,停下来对齐 R1)
- [ ] **Step 3: Commit** — `git commit -m "test(cloud): O7 立即生效回归锁——切换档位下一动作按新档"`

### Task 18: 概念模型 lint(E3,warn-only)

**Files:**
- Create: `scripts/concept-lint.sh` + CI 接线(`.github/workflows/` 对应文件)
- 异名清单来源:docs/ai-concept-model.md 词汇表右列(Task 1)

**Interfaces:**
- 扫描范围(Q4):`web/src` 文案 + `apps/cloud/src/domains/agent` 注释;命中异名(调查 run 以外语境的 "tick"/"执行历史"/"证据" 等)打印 warning;首版 warn-only(exit 0),一版本后转 fail

- [ ] **Step 1: 写脚本 + 本地跑出当前命中清单**(作为 baseline 注释进脚本头)
- [ ] **Step 2: CI 接线(continue-on-error: true)**
- [ ] **Step 3: Commit** — `git commit -m "chore(ci): 概念模型 lint(E3)——异名扫描 warn-only"`

### Task 19: Phase 2 收尾门禁

- [ ] **Step 1: 行为保持清单四行逐项核对**(连败暂停=governor 测试+Task 5;last_tick=Task 12 handler 测试;热更新=ReloadTasks/ReloadConfig 信号通道测试迁移到统一循环——若旧信号测试随 runner 退役,统一循环侧等价测试须在 Task 6/13 已含;每工作区间隔=Task 5 测试)
- [ ] **Step 2: 全量门禁** — `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- [ ] **Step 3: spec 勾选 + commit** — `git commit -m "chore: Phase 2 内核归一收口——行为保持清单逐项核对"`

---

## 依赖与顺序

| Task | 依赖 |
|---|---|
| T1 概念文档 | — |
| T2 基线+分类 | — |
| T3 Heartbeat 变体 | — |
| T4 governor | T3 |
| T5 HeartbeatTrigger | T3, T4 |
| T6 manager 接线 | T3-T5 |
| T7 RunRecorded 重试 | —(独立) |
| T8 persist 心跳支路 | T3 |
| T9 迁移三件套 | T8(提案新锚语义先立) |
| T10 读侧切换 | T9 |
| T11 告警链接入 | T3, T4 |
| T12 last_tick | T6 |
| T13 bootstrap 双跑 | T5, T6, T11, T12 |
| T14 shadow 对照 | T13 |
| T15 旧 runner 退役 | T14 + 生产双跑达标(N=7天/M=50) |
| T16 host 分组 | T15(退役后平铺面最小) |
| T17 O7 回归锁 | —(独立,可早做) |
| T18 lint | T1 |
| T19 收尾 | 全部 |

并行机会:T1/T2/T7/T17 可随时;T3→T6 是主轴;T16 纯移动放最后防冲突。

## Phase 3 说明(不在本计划)

前端 AI 中心(T1-T10 设计轮任务)在 Phase 2 落地后另行出计划——`run_recorded` SSE 事件形、agent_runs 投影字段在 Phase 2 定型后才是准确输入。spec §3 的 F1-F18 裁定与设计轮 Implementation Tasks(T1-T10)已足够细,届时直接展开。
