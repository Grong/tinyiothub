# 心跳作为 AI 生命线 — AI 自主巡检设计

> 状态: 设计完成 | 日期: 2026-06-17

## 概述

将 HeartbeatService 从被动任务执行器改造为 AI 的生命线：AI 定时醒来，通过 MCP 工具主动查询系统状态，发现问题、决策处理、记住上下文。告警不再触发独立的 AI 处理通道，而是作为"立即唤醒"信号推送完整上下文给心跳循环。

**核心洞察：不只给 AI 工具，还要给 AI 记忆。** 这让心跳从定时任务 runner 变成一个拥有连续上下文的自治 Agent。

## 架构

```
SYSTEM EVENTS (AlarmService, etc.)
        │
        ▼
┌──────────────────────────────────────────┐
│         HeartbeatManager                  │
│  DashMap<ws_id, Sender<WakeSignal>>       │
│  start(ws_id) / stop(ws_id) / wake(ws_id) │
└──────────────┬───────────────────────────┘
               │ mpsc::channel(64)
     ┌─────────┼─────────┬──────────────┐
     ▼         ▼         ▼              ▼
  ┌──────┐ ┌──────┐ ┌──────┐      ┌──────┐
  │ws-A  │ │ws-B  │ │ws-C  │ ...  │ws-N  │  tokio::spawn
  │ loop │ │ loop │ │ loop │      │ loop │
  └──┬───┘ └──┬───┘ └──┬───┘      └──┬───┘
     │        │        │              │
     ▼        ▼        ▼              ▼
  build_prompt()
  ├── HEARTBEAT.md (per-workspace)
  ├── recent agent_actions (last 10)
  └── WakeSignal context (if any)
     │
     ▼
  AgentPool.run_single(ws_id, prompt)
  + 16 MCP tools
     │
     ▼
  record_action() → agent_actions
```

## 组件变更

| 组件 | 当前 (v0.4.2) | 新版本 (v0.5.0) |
|------|--------------|----------------|
| HeartbeatService | 1 个全局，依赖 zeroclaw | **删除** |
| HeartbeatManager | 不存在 | **新增**，管理 per-workspace 循环 |
| AutonomousAgentRunner | 直接调 LLM | **删除** |
| AlarmService | spawn AutonomousAgentRunner | 调用 `HeartbeatManager::wake()` |
| HEARTBEAT.md | 全局 1 份 | per-workspace |
| AI 调用 | chat_send() 无工具 | run_single() + 16 MCP tools |
| 审计日志 | agent_actions（分散） | agent_actions（统一，event_type="heartbeat"） |

## HeartbeatManager

**文件：** `cloud/src/modules/agent/heartbeat_manager.rs`（新增）

```rust
pub struct WakeSignal {
    pub workspace_id: String,
    pub reason: String,           // "alarm:alarm-id:高温告警"
    pub context: String,          // 完整上下文，直接入 prompt
    pub priority: WakePriority,   // Critical / High / Normal
}

pub enum WakePriority { Critical, High, Normal }

pub struct HeartbeatManager {
    // (wake_tx, shutdown_tx) pair per workspace
    channels: DashMap<String, (mpsc::Sender<WakeSignal>, tokio::sync::oneshot::Sender<()>)>,
    handles: DashMap<String, JoinHandle<()>>,
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    device_cache: Arc<DeviceCache>,  // format_context 需要设备名
    config: HeartbeatConfig,
}

impl HeartbeatManager {
    pub async fn start(&self, workspace_id: &str);
    /// 通过 oneshot 发送关闭信号，只停止单个 workspace
    pub async fn stop(&self, workspace_id: &str);
    /// try_send，非阻塞。channel 满或 workspace 不存在时静默丢弃
    pub fn wake(&self, workspace_id: &str, signal: WakeSignal);
    pub fn list_active(&self) -> Vec<String>;
    pub async fn shutdown(&self);
}
```

**生命周期：**
- `WorkspaceService::create_workspace()` → `HeartbeatManager::start(ws_id)` → 创建 HEARTBEAT.md 默认模板 + tokio::spawn
- `WorkspaceService::delete_workspace()` → `HeartbeatManager::stop(ws_id)` → 通过 oneshot 发送关闭信号 + 清理 channels 和 handles
- `ServiceManager::start_all()` → 创建 HeartbeatManager → 加载已有 workspace → 逐个 start()
- `AppState::shutdown()` → `HeartbeatManager::shutdown()` → 依次关闭

**HEARTBEAT.md 路径：** 使用现有 `paths::heartbeat_file(&workspace_id)` 而不是单独维护 workspace_dir_base。路径约定：`{agents_base}/{workspace_id}/HEARTBEAT.md`（由 paths 模块集中管理）。

**DashMap 僵尸清理：** start() 调用前先清理已有条目（幂等）。定期清理逻辑为 best-effort（zombie entries 数量极少，且下次 start() 会覆盖）。

**配置（来自 AgentSettings）：**
```rust
pub struct HeartbeatConfig {
    pub enabled: bool,                // AgentSettings.heartbeat_enabled，默认 true
    pub interval_minutes: u32,       // AgentSettings.heartbeat_interval_minutes，默认 15
    pub max_recent_actions: usize,   // 硬编码默认 10（暂不加到 AgentSettings）
    pub channel_size: usize,         // 硬编码默认 64（暂不加到 AgentSettings）
}
```

`max_recent_actions` 和 `channel_size` 使用硬编码默认值，不需要修改 AgentSettings 结构体。后续 per-workspace 配置 UI（扩展 5）再做可配置化。

## 心跳循环核心

**文件：** `cloud/src/modules/agent/heartbeat.rs`（重写）

```rust
async fn heartbeat_loop(
    workspace_id: String,
    config: HeartbeatConfig,
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    workspace_dir: PathBuf,
    mut wake_rx: mpsc::Receiver<WakeSignal>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut interval = tokio::time::interval(
        Duration::from_secs(config.interval_minutes as u64 * 60)
    );

    loop {
        let wake_signals = tokio::select! {
            _ = interval.tick() => {
                // 定时触发：排空期间积压的 wake 信号
                drain_channel(&mut wake_rx)
            }
            sig = wake_rx.recv() => {
                match sig {
                    Some(first) => {
                        let mut signals = vec![first];
                        signals.append(&mut drain_channel(&mut wake_rx));
                        signals
                    }
                    None => break, // channel 关闭 → workspace 被删除
                }
            }
            _ = &mut shutdown_rx => break, // stop() 发送关闭信号
        };

        let prompt = build_prompt(
            &workspace_dir, &action_repo, &workspace_id,
            &wake_signals, config.max_recent_actions,
        ).await;

        match agent_pool.run_single(&workspace_id, &prompt).await {
            Ok(response) => {
                record_action(&action_repo, &workspace_id, "summary", &response).await;
            }
            Err(e) => {
                record_action(&action_repo, &workspace_id, "error", &e.to_string()).await;
            }
        }
    }
}

fn drain_channel<T>(rx: &mut mpsc::Receiver<T>) -> Vec<T> {
    let mut items = Vec::new();
    while let Ok(item) = rx.try_recv() {
        items.push(item);
    }
    items
}
```

**唤醒信号收集逻辑：**
- `tokio::select!` 三路：定时 tick / wake 信号 / 关闭信号
- wake_rx.recv() 返回 None 时 break（channel 关闭 = workspace 被删除）
- drain_channel() 在一次 tick 内排空所有积压信号，不会无限等待

**record_action() 定义：**
```rust
async fn record_action(
    repo: &dyn AgentActionRepository,
    workspace_id: &str,
    action_type: &str,  // "summary" | "error"
    content: &str,
) {
    let action = AgentAction::new(
        workspace_id.to_string(),
        "default".to_string(),  // agent_id
        None, None,             // alarm_id, device_id
        "heartbeat".to_string(), // event_type
        action_type.to_string(),
        content.to_string(),
    );
    let _ = repo.insert(&action).await;
}
```

**Prompt 模板：**
```
你是 IoT 平台的自治 AI 助手。当前时间: {now}

## 你的能力
你可以通过工具查询设备状态、告警列表、属性历史、知识库等。

## 静态巡检任务
{tasks}

## 上次执行记录
{recent}

## 本次唤醒原因
{wake}

请自主执行巡检:
1. 如果 wake 有告警，优先分析告警
2. 查看上次未完成的工作，决定是否继续
3. 执行静态巡检任务
4. 发现问题时，通过工具获取更多信息，做出决策
5. 给出简明的结构化报告
```

Prompt 由三部分组成：
1. **静态任务** — per-workspace HEARTBEAT.md 中未暂停的任务
2. **执行记忆** — 最近 10 条 agent_actions（event_type 为 "heartbeat" 或 "alarm"），让 AI 知道上次做了什么、什么未完成。同时查询两种类型确保从旧 AutonomousAgentRunner 产生的 alarm action 也能作为上下文。
3. **唤醒上下文** — 如有告警触发，包含完整告警信息（设备、属性、当前值、时间）

## AlarmService 变更

**文件：** `cloud/src/modules/alarm/service.rs`（修改）

```rust
// 移除
- autonomous_runner: OnceLock<Arc<AutonomousAgentRunner>>
- pub fn set_autonomous_runner()

// 新增
+ heartbeat_manager: Arc<HeartbeatManager>

// create_alarm_with_event() 末尾
if alarm.alarm_level == AlarmLevel::Error || alarm.alarm_level == AlarmLevel::Critical {
    heartbeat_manager.wake(workspace_id, WakeSignal {
        workspace_id: workspace_id.to_string(),
        reason: format!("alarm:{}:{}", alarm.id, alarm.message),
        context: format_context(&alarm, &event),
        priority: WakePriority::High,
    });
}
```

**format_context()** 将告警和事件数据格式化为 AI 可理解的文本。

**移除的文件：**
- `cloud/src/modules/agent/autonomous_runner.rs` — 所有逻辑由 HeartbeatManager + 心跳循环接管

## 执行连续性（AI 记忆）

上一次 tick 的 AI 输出作为 `action_type="summary"` 写入 agent_actions。下一次 tick 的 prompt 包含最近 10 条记录，形成跨 tick 的连续工作流：

```
## 上次执行记录
- [2026-06-17 10:00] summary:
  ✓ 离线设备检查: 全部在线
  ✓ 告警扫描: 发现 alarm-456 高温告警，已分析建议降温
  ⚠ 待跟进: dev-05 上次离线重连失败，建议检查硬件连接

- [2026-06-17 09:45] error:
  LLM 调用超时，未能完成巡检
```

agent_actions Schema 不变，`event_type` 增加 `"heartbeat"` 值。

## 错误处理

| 场景 | 处理方式 |
|------|---------|
| LLM 超时（>60s） | 记录 error action，下次 tick 继续，不重试 |
| LLM 调用失败（网络/配额） | 记录 error action，打印 error 日志 |
| wake channel 满（>64） | try_send 丢弃，打印 warning |
| 心跳循环 panic | JoinHandle 自动清理，restart = stop() + start()（start 幂等） |
| HEARTBEAT.md 不存在 | 自动创建默认模板 |
| agent_actions 写入失败 | 打印 error 日志，不影响循环 |
| build_prompt 失败（DB 错误/文件不可读） | 跳过本次 tick，记录 error 日志。下次 tick 重试 |
| workspace 被删除 | stop() 通过 oneshot 发送关闭信号 → 循环优雅退出 → 清理 channels 和 handles |
| HEARTBEAT.md 并发写入 | 接受低风险：文件读写可能 TOCTOU，但 IoT 巡检场景下并发编辑极罕见 |
| DashMap 残留条目 | start() 前清理已有条目（幂等）；定期检查数据库 workspace 列表并移除僵尸条目 |
| enabled=false (全局开关) | HeartbeatManager 不创建任何循环 |

**设计原则：** 心跳 crash 不影响其他 workspace、AI 失败不影响系统正常运行、所有错误路径有日志。

## AgentActionRepository 扩展

`build_prompt()` 需要按 event_type 过滤最近 actions，但当前 `AgentActionQuery`（action_repo.rs:50-58）不支持 event_type 过滤。需要新增方法：

```rust
// action_repo.rs — AgentActionRepository trait 新增
async fn find_recent_by_workspace(
    &self,
    workspace_id: &str,
    event_types: &[&str],  // "heartbeat" | "alarm"
    limit: u32,
) -> Result<Vec<AgentAction>, AgentError>;
```

SQLite 实现：`SELECT ... FROM agent_actions WHERE workspace_id = ? AND event_type IN (?, ?) ORDER BY created_at DESC LIMIT ?`

## 告警风暴保护

旧 AutonomousAgentRunner 的 5s dedup 被删除后，告警风暴（同时 100 个设备报警）可导致：
- WakeSignal channel 填满（64 条被丢弃 — 可接受）
- build_prompt 拼接 64 条告警上下文 → 超过 LLM token 限制

**保护策略：**
1. `format_context()` 中每条告警上下文压缩在 ~200 字符内
2. `build_prompt()` 中 wake signals 上限 5 条（超过的标记 `(+N 条省略)`）
3. HeartbeatManager::wake() 保留 per-alarm-id 去重（同一告警不重复入队）

即在 `drain_channel` 后添加 alarm_id 去重 + cap：
```rust
fn dedup_and_cap(signals: &mut Vec<WakeSignal>, cap: usize) {
    // 1. per alarm_id 去重（保留优先级最高的）
    // 2. 按 priority 排序后截断到 cap
}
```

## format_context 设计

`format_context()` 将告警格式化为 AI 可读的简短文本（~200 字符内）。需要 DeviceCache 来获取设备名称。HeartbeatManager 新增 `device_cache: Arc<DeviceCache>` 字段：

```rust
fn format_context(alarm: &Alarm, event: &Event, cache: &DeviceCache) -> String {
    let device_name = cache.get(device_id)
        .map(|d| d.display_name.unwrap_or(d.name))
        .unwrap_or(device_id);
    format!(
        "设备 {name} 触发 {level} 告警: {msg} (当前值: {val})",
        name = device_name,
        level = alarm.alarm_level.as_str(),
        msg = alarm.message,
        val = value,
    )
}
```

## 文件变更清单

### 新增
| 文件 | 作用 |
|------|------|
| `cloud/src/modules/agent/heartbeat_manager.rs` | HeartbeatManager — per-workspace 循环管理 |

### 修改
| 文件 | 变更 |
|------|------|
| `cloud/src/modules/agent/heartbeat.rs` | 重写：移除 zeroclaw 依赖，新增 heartbeat_loop()、build_prompt()、record_action() |
| `cloud/src/modules/alarm/service.rs` | 替换 autonomous_runner 为 heartbeat_manager，新增 format_context() |
| `cloud/src/shared/service_manager.rs` | 创建 HeartbeatManager，移除 zeroclaw imports |
| `cloud/src/shared/app_state.rs` | 移除 AutonomousAgentRunner 连线，新增 HeartbeatManager 字段 |
| `cloud/src/modules/agent/action_repo.rs` | 新增 `find_recent_by_workspace()` 方法，支持 event_type 过滤 |
| `cloud/src/modules/agent/mod.rs` | 移除 autonomous_runner 模块，导出 heartbeat_manager |
| `cloud/src/shared/paths.rs` | 确认 `heartbeat_file()` 存在（已有），heartbeat_loop 直接使用 |

### 删除
| 文件 | 原因 |
|------|------|
| `cloud/src/modules/agent/autonomous_runner.rs` | 所有逻辑由 HeartbeatManager + 心跳循环接管 |

## 测试策略

- **heartbeat_manager 单元测试** — start/stop/wake 生命周期、channel 满丢弃行为
- **heartbeat_loop 单元测试** — build_prompt 内容正确性（mock action_repo）、HEARTBEAT.md 解析
- **AlarmService 集成测试** — 验证 Error/Critical 告警触发 wake、Info/Warning 不触发
- **LLM 调用通过集成手动验证**（不 mock LLM）

## NOT in scope

| Item | Rationale |
|------|-----------|
| 告警以外的系统事件（设备上下线等） | v2 扩展，当前聚焦告警→心跳通道 |
| AI 操作结果推送通知用户 | v1 只记录 agent_actions |
| 前端 AI 操作日志页 | 已有 API 可查询 agent_actions |
| WakerManager per-workspace 配置 | 使用全局默认配置 |
| 事件流/事件溯源 | 方案 C，v2 考虑 |

## What already exists

| Existing | How plan uses it |
|----------|-----------------|
| `AgentPool::run_single(ws_id, prompt)` | 心跳循环直接调用 |
| `AgentActionRepository` + `agent_actions` 表 | 统一审计日志，新增 event_type="heartbeat" |
| 16 MCP tools（device, alarm, knowledge, driver 等） | 通过 AgentPool::get_or_create() 创建的 Agent 已自动注册所有 MCP 工具，心跳循环无需额外配置 |
| `DeviceCache` (ArcSwap-based) | 心跳 AI 通过 MCP tool 间接访问 |
| `AlarmService::create_alarm()` | Hook 点 — 替换为 wake() 调用 |
| HEARTBEAT.md 解析/构建函数 | 保留并改为 per-workspace |
| `AgentSettings.heartbeat_enabled` + `heartbeat_interval_minutes` | 配置源 |
