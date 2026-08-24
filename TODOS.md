# TODOS

> **最新完整 TODO 清单已迁移至:** `docs/superpowers/plans/2026-04-14-todo-audit-and-cleanup-plan.md`
> 本文档保留 Edge Intelligence Agent 历史记录，新项目 TODO 请查阅上方计划。

## Crates Reorg Review (from /plan-eng-review 2026-08-03)

### P2 — unwrap/expect 治理（edge 与驱动加载路径优先）
- **What:** 非测试代码 1078 处 `.unwrap()`/`.expect()` 分批治理，优先 `edge/` 与驱动加载路径（registry.rs/loader.rs）。
- **Why:** 边缘设备上 panic = 现场宕机；开源后此密度会被安全审计派点名。错误类型（tinyiothub-error）已存在但形同虚设。
- **Context:** 2026-08-03 统计（grep 非测试代码）。edge 场景优先，cloud handler 层可缓。与 crates 重组解耦，可随时进行。
- **Effort:** L (human: ~1w / CC: ~1d 分批) | **Depends on:** —

### P2 — 跟踪 sqlx 0.9 正式版，迁出 alpha 依赖
- **What:** `Cargo.toml` workspace 依赖 `sqlx = "0.9.0-alpha.1"` → 0.9 正式版发布后迁移；或评估回退 0.8 stable。
- **Why:** 生产+开源项目用 alpha 依赖，下游打包者（distro/Nix）直接放弃；alpha API 漂移风险随每次更新。
- **Context:** 当前使用 features: runtime-tokio/sqlite/chrono/uuid/migrate。迁移前跑全量 db 测试。
- **Effort:** M (human: ~1d / CC: ~1h) | **Depends on:** sqlx 0.9 GA

### TODOS 维护注记（2026-08-03 核实）
- "AI Subsystem → P1 — Wire DropNotifier + DeadLetterQueue" **已实际完成**：`apps/cloud/src/shared/service_manager.rs:151,235-236` 接线 LoggingDropNotifier + SqliteDeadLetterQueue（crates/agent/src/host/dlq_repo.rs）。条目过期，应归档。

## Thing Agent Loop — Deferred (from /plan-ceo-review 2026-07-29, spec v2 O1-O16)

### P3 — agent_runs 保留策略
- **What:** 为 agent_runs 表定义保留策略（按 outcome 分类生命周期：acted+verified 长留、failed 中留、no_action 短留），定期清理。
- **Why:** events 表"无保留策略"的坑 D3.3 才补上；agent_runs 最坏 480 行/天/工作区，不重复踩同一个坑。
- **Context:** 复用 D3.3 occurrence-aware retention 框架；agent_daily_cost 视图依赖历史行，清理时保留聚合结果。
- **Effort:** S (human: ~0.5d / CC: ~30min) | **Depends on:** Thing Agent Loop 主框架落地

### P2 — live SSE 回推自治 Run 结果
- **What:** Run 完成回推 chat 时，向正在观看该会话的客户端实时推送（当前仅 `history::append_message` 落库，在线用户需刷新才可见）。
- **Why:** 用户指令"受理→执行→回报"体验闭环的最后一公里；无实时推送时在线用户感知不到执行完成。
- **Context:** chat 模块当前无 per-session 广播通道（grep 无 broadcast/subscribe）；需新增轻量 session 通知机制或挂到既有 SSE 通道。
- **Effort:** M (human: ~1d / CC: ~1h)

### P2 — TrendAnomalyTrigger（遥测基线异常触发器）
- **What:** 分析遥测趋势发现模型外异常（无事件定义也能发现问题），注册为第四个 Trigger。
- **Why:** 事件驱动只能发现"已定义"的问题；趋势异常覆盖未预见故障模式。
- **Context:** Trigger 接口与调度已预留（spec §二）；需要遥测基线/异常检测算法选型。
- **Effort:** L (human: ~3d / CC: ~4h) | **Depends on:** Thing Agent Loop 主框架落地

### P3 — GoalTrigger（持续目标维持）
- **What:** 用户下达高层目标（"车间温度维持 20-26°C"），AI 长期巡检+事件响应维持，周期报告。
- **Why:** 10x 愿景的核心形态；L4 自治的最终价值。
- **Context:** Trigger 接口已预留；需要目标状态管理（目标表、达成判定、长期记忆）。
- **Effort:** L (human: ~3d / CC: ~4h) | **Depends on:** Thing Agent Loop + X1 历史注入

### P2 — Runs 列表/策略配置 UI 面板 + X5 预填配置页
- **What:** agent_runs 列表页、三态策略配置页、X5 `policy_relax_hint` 预填落地。
- **Why:** 本期 API 完整但无界面；Runs 可见性是"可信自治"叙事的主展示面。
- **Context:** A2UI 子项目（CEO 计划 D3.4）是天然展示层，建议与 A2UI 本体渲染同期做。
- **Effort:** M (human: ~2d / CC: ~2h) | **Depends on:** E2 A2UI 本体渲染

### P3 — heartbeat_trust_config 旧表下线
- **What:** X3 统一策略面适配器稳定后，迁移数据并 DROP heartbeat_trust_config。
- **Why:** 消除最后一个旧治理面，完成三接入面收敛。
- **Context:** X3 适配器读旧表翻译为新引擎输入；下线前需跑适配器等价测试全绿一个迭代。
- **Effort:** S (human: ~0.5d / CC: ~30min) | **Depends on:** X3 统一策略面

### P2 — 心跳 runner 迁入 Trigger 框架
- **What:** HeartbeatRunner 的定时巡检改为 TimerTrigger 的一种配置，统一巡检语义。
- **Why:** 消除心跳与 Thing Agent Loop 两套巡检并存（spec R4）；X6 已架桥，迁徙是自然后续。
- **Context:** 心跳 runner 本期不动（O2 裁决仅加投递出口）；迁徙时保留 TrustEngine 适配路径。
- **Effort:** M (human: ~2d / CC: ~2h) | **Depends on:** Thing Agent Loop 稳定运行一个迭代；重组 P4（2026-08-05 已完成：heartbeat 已归 crates/agent 与 crates/driver）

### P3 — POST /agent/tasks 前端面板
- **What:** 管理 API `POST /api/workspaces/{id}/agent/tasks` 的前端入口（自治任务提交面板）。
- **Why:** chat 工具已覆盖主路径，面板服务"不想开对话直接派任务"的用户。
- **Context:** API 本期交付；可与 Runs 面板同页。
- **Effort:** S (human: ~0.5d / CC: ~30min) | **Depends on:** Runs UI 面板

### P2 — chat 会话 admin 维度（回推防泄漏收窄）
- **What:** `chat_sessions.user_id` 列在写入路径（history.rs ensure_session、session_repository_impl）填值，或加 `metadata.is_admin`；`recent_active_admin_session` 据此真正按 admin 过滤。
- **Why:** O28 要求无会话回推收窄为 admin 会话防多用户泄漏；当前实现是"工作区任意用户最近会话"，多用户工作区下 run 报告可能推入普通用户会话。
- **Context:** 列已存在（20260408000001 迁移）但两个写入路径都不填；单用户形态下风险低（CEO 0E 决议接受）。
- **Effort:** S (human: ~0.5d / CC: ~30min) | **Depends on:** Thing Agent Loop 主框架落地


## AI Subsystem (from /plan-ceo-review 2026-06-30, SCOPE REDUCTION)

### P1 — Wire DropNotifier + DeadLetterQueue
- **What:** 实现 DropNotifier（至少 logging 级别）和 DeadLetterQueue（SQLite 存储），在 ServiceManager 中注入。
- **Why:** 当前 AiEventPublisher 发布失败时事件静默丢失。retry_with_backoff 中的 DLQ 逻辑是死代码（dlq 始终为 None）。
- **Files:** `apps/cloud/src/shared/service_manager.rs:169-170`, `crates/agent/src/loop_/event/bus.rs`, `crates/agent/src/loop_/event/dlq.rs`
- **Effort:** S (human: ~4h / CC: ~30min)

### P2 — ~~Wire TrustConfig DB loading~~（已失效，2026-07-29 核实删除）
- **核实结果：** `crates/agent/src/loop_/heartbeat/runner.rs:341-350` 经 `task_repo.load_trust_config` 从 `workspaces.heartbeat_trust_config` 列加载（heartbeat_repo.rs:167-180，含测试 :593-603）。DB 加载已接线，本条作废。

### P2 — Add dynamic task/config refresh to heartbeat loop (Outside Voice)
- **What:** 运行中的心跳循环无法获取最新的任务列表或 TrustConfig。任务增删需完整 stop/restart。TrustConfig 更新对运行中的循环不生效。
- **Why:** Outside Voice 发现的设计限制。当前 stop/restart 模式可工作但不够优雅。
- **Files:** `crates/agent/src/loop_/heartbeat/loop_.rs`, `runner.rs`
- **Effort:** M (human: ~1d / CC: ~1h)

### P3 — Cache regex in extract_json
- **What:** `report.rs:37` 的 `Regex::new` 每次调用重新编译，改为 `std::sync::LazyLock` 缓存。
- **Why:** 微优化，心跳每 15min tick 一次，对性能无明显影响。纯粹代码质量改进。
- **Files:** `crates/agent/src/loop_/heartbeat/report.rs:37`
- **Effort:** S (human: ~5min / CC: ~1min)

## AI Deep Review — Deferred (from /ship pre-landing review 2026-07-21)

以下 12 项为 `fix/ai-deep-review` 发 PR 前评审确认的低优先级发现，当日评审决定不修，记入此清单。

### A3 — chat_history 不应向普通会话暴露 toolCalls
- **What:** `chat_history` 返回的消息 JSON 携带 `toolCalls` 明细；评审建议对非管理端裁剪。
- **Files:** `crates/agent/src/host/chat/history.rs`

### S4 — trust config 端点缺角色校验
- **What:** heartbeat trust 配置读写端点仅校验 workspace 归属，未区分 admin/member 角色。
- **Files:** `crates/agent/src/host/handler/workspace_heartbeat.rs`

### P1 — json_extract 查询无表达式索引
- **What:** `json_extract(content, '$.proposalId')` 查询走全表扫描；可加表达式索引或独立列。
- **Files:** `crates/agent/src/host/handler/workspace_heartbeat.rs`（approve/reject 查询）

### P2 — get_or_create 缺 single-flight
- **What:** AgentPool::get_or_create 并发下可能重复构建 agent（double-checked DashMap 已缓解但未完全消除）。
- **Files:** `crates/agent/src/host/agent.rs`

### A2 — abort 首事件前窗口
- **What:** run_id 由首个 SSE 事件带回客户端；此前客户端无法 abort 该 run。
- **Files:** `crates/agent/src/chat/handler/proxy.rs`

### A5 — 部署历史重置说明
- **What:** agent deploy 后历史上下文重置的行为需在 API 文档/前端明示。

### M2–M6 — 重复代码与魔法常量
- **What:** 三个 PendingProposal 前端接口重复（DRY）；审批状态/优先级等字符串常量散落多处；部分 handler 错误消息重复拼接模式。
- **Files:** `web/src/ui/views/{heartbeat,agents-heartbeat-tab,ai-ops}.ts`, `crates/agent/src/host/handler/workspace_heartbeat.rs`

### T3–T6 — 补充测试
- **What:** chat/service.rs reseed+persist 降级路径、get_or_create 并发竞态（需真实 provider/LLM，单测成本高）。
- **Files:** `crates/agent/src/host/chat/service.rs`, `crates/agent/src/host/agent.rs`

---

> Organized by skill/component, then priority (P0 at top through P4, then Completed at bottom)

## Edge Intelligence Agent

**Completed:**

- **Correct tool count in plan doc:** The plan doc already reflected 28 tools (was updated in prior session). Log message in `mod.rs` incorrectly said "13 device tools" — fixed to "12".
  **Completed:** (fix: change 13 to 12 in mod.rs:93)

- **MCP tool call logging:** Add structured logging for every MCP tool invocation: tool name, user ID, tenant ID, sanitized args, latency, result/error. Without this, production debugging of AI → MCP interactions is impossible.
  - Source: `/plan-ceo-review` on `feature/edge-agent-phase1` (2026-04-04)
  **Completed:** (ead10f6)

- **Fix weak pagination test:** `test_list_devices_respects_pagination` in `api/src/api/mcp/tests/integration_tests.rs` accepts both `Ok` and `Err` outcomes, meaning it doesn't actually verify the pagination clamp behavior. Should assert a specific outcome.
  - Source: `/plan-ceo-review` on `feature/edge-agent-phase1` (2026-04-04)
  **Completed:** (ead10f6 — fixed camelCase args to match schema, added page_size validation)

**Completed:**

- **L3 self-heal approval enforcement:** Verified as false positive — enforcement IS implemented in `self_heal.rs:120-128`. L3 has `require_approval: true` in default policy. No gap.
  **Verified:** (2026-04-04)

- Phase 1: Embedded MCP Server in API crate with 29 tools
  **Completed:** v1.0.0 (2026-03-28)

- Phase 2: Self-Healing Engine with Probe Scheduler
  - Domain module: SeverityLevel, RecoveryActionType, SelfHealingPolicy, PolicyEvaluator, ActionExecutor
  - ProbeScheduler: system/device/task probes with configurable intervals
  - REST API: /self-healing/policies, /self-healing/actions/:level, /self-healing/executions, /self-healing/probes
  - MCP tools: execute_self_heal_action, get_recovery_history, get_self_heal_policy (fully functional)
  - DB migration: healing_executions table
  **Completed:** v1.1.0 (2026-03-28)

## Device Ecosystem v0.2 (PR #39) — Follow-ups

Source: `/plan-ceo-review` on `feat/device-ecosystem-v0.2` (2026-05-08)

### P0 — CRITICAL

- **[#40] Driver loading needs sandbox or admin-only gate**
  - `crates/runtime/src/driver/registry.rs:50-65` loads arbitrary `.so` and calls `init()` with full process privileges
  - `crates/runtime/src/driver/validator.rs:19-24` dry-load triggers `__attribute__((constructor))` before any validation
  - **Action:** Implement admin-only gate for driver installation (quick fix), plan subprocess sandbox for v0.2.x
  - **Owner:** TBD

### P1 — HIGH

- **[#41] TemplateExporter (`apps/cloud/src/domains/thing/template/exporter.rs`) secret stripping is shallow**
  - Only strips top-level keys; nested JSON like `{"auth": {"password": "secret"}}` leaks
  - Missing variants: `passwd`, `key`, `credential`, `cert`
  - **Action:** Recursive JSON traversal + expanded sensitive key list
  - **Owner:** TBD

### P2 — MEDIUM

- **[#42] Exported templates lose device properties and commands**
  - `apps/cloud/src/domains/thing/template/exporter.rs` created empty `properties` and `commands` vectors
  - Users export a configured device and get a hollow template
  - **Action:** Map `device.properties` → `PropertyTemplate`, `device.commands` → `CommandTemplate`
  - **Owner:** TBD

- **[#44] Add unit tests for DriverRegistry (`crates/runtime/src/driver/registry.rs`, `runtime::driver`) failure paths**
  - Zero coverage for: ABI mismatch, null vtable, null init, missing symbols, duplicate driver, ref_count blocking unload
  - Single integration test only checks "empty registry returns empty list"
  - **Action:** Craft mock/minimal `.so` files or use `libloading` mocking to test each failure path
  - **Owner:** TBD

### P3 — LOW

- **[#43] `workspace_driver_preferences` migration has zero code references**
  - Migration exists but no Rust code reads or writes this table
  - **Action:** Either remove migration or add TODO comment explaining future use
  - **Owner:** TBD

## MQTT Gateway Pairing (v0.1)

Source: `/plan-eng-review` on `main` (2026-05-11)

Source: `/plan-eng-review` on `feature/mqtt-gateway-pairing` (2026-05-13)
### P1 — HIGH

- **Gateway e2e test with mock gateway**
  - Core pairing flow crosses 3 systems (gateway → broker → platform), unit tests can't cover it. CI e2e with `tests/e2e/docker-compose.yml` + mosquitto + mock MQTT gateway that sends announce, waits for ack, sends telemetry.
  - **Depends on:** edge/ base implementation complete
  - **Effort:** M (human: 2 days / CC: 30min)
  - **Owner:** TBD

- **Edge Docker image CI/CD build and publish**
  - `deploy/docker/Dockerfile.edge` exists but `release.yml` doesn't build/push it. Users can't `docker pull` the edge image as documented. Extend `release.yml` to build multi-arch (amd64 + arm64) edge image and push to Docker Hub.
  - **Depends on:** — (CI workflow already supports multi-arch builds for main image)
  - **Effort:** S (human: 1h / CC: 15min)
  - **Owner:** TBD

### P1 — HIGH (continued)

- **Gateway offline detection and data message handling**
  - `PlatformMqttClient` subscribes to gateway status/telemetry/event/discover topics but event loop drops all messages with `Ok(_) => {}`. Implement basic message routing (status→offline detection, discover→sub-device creation). Offline detection: track last heartbeat, mark gateway+sub-devices offline on timeout.
  - **Source:** Outside voice (`/plan-eng-review`, 2026-05-13)
  - **Depends on:** Gateway data message handling framework (eng review, current PR)
  - **Effort:** M (human: 1.5 days / CC: 20min)
  - **Owner:** TBD

### P2 — MEDIUM

- **Batch INSERT optimization for handle_device_discover**
  - `service.rs:218-238` loops individual INSERTs per sub-device. Switch to single batch INSERT (`VALUES (row1), (row2), ...`) for N SQL round-trips → 1. Current approach fine for < 20 sub-devices; optimize when gateway reports 50+.
  - **Depends on:** Device Repository extension (eng review Issue 4)
  - **Effort:** S (human: 1h / CC: 10min)
  - **Owner:** TBD

- **Implement DeviceScanner with real protocol drivers**
  - `edge/src/device_discovery.rs:scan()` returns empty `Vec::new()`. `load_from_config()` never called from main.rs. Implement actual auto-discovery: scan local Modbus/ONVIF buses, or at minimum load devices from local JSON config file and report via device_discover MQTT message.
  - **Source:** Outside voice (`/plan-eng-review`, 2026-05-13)
  - **Depends on:** Device discover message handling on platform side
  - **Effort:** M (human: 2 days / CC: 30min)
  - **Owner:** TBD
## Agent Config Simplification (v0.3)

Source: `/plan-eng-review` on `feat/ai-agent-v0.3` (2026-05-19)

### P2 — MEDIUM

- **Post-Conversation Pipeline** — 对话后异步分析对话，更新 IDENTITY.md / MEMORY.md
  - AgentMemoryItem::conversation_summary() 已存在（types.rs:477），可作为起点
  - **Why:** Agent 身份和记忆随对话演进，完成「系统自动管理」闭环
  - **Effort:** M (human: ~4h / CC: ~30min)
  - **Depends on:** —

- **TOOLS.md Auto-Generation** — 工具权限变更时重新生成 TOOLS.md
  - tool_label() / tool_group() 已存在（service.rs:196-248），薄包装即可
  - **Why:** 为 Agent 提供当前可用工具的可读清单，提升工具选择准确性
  - **Effort:** S (human: ~1h / CC: ~10min)
  - **Depends on:** —

### P3 — LOW

- **Workspace Description Templates** — 文本框下方 2-3 个填空式模板（"这是___园区，面积___平米"）
  - CEO 评审 (SELECTIVE EXPANSION) 接受
  - **Why:** 降低非技术用户写作门槛
  - **Effort:** S (human: ~30min / CC: ~10min)
  - **Depends on:** T6 (工作区设定 Tab)

- **Zero-Config Agent** — 首次对话自动询问工作区背景，根据回答生成 USER.md
  - CEO 评审推迟
  - **Why:** 终极零摩擦体验
  - **Effort:** M (human: ~3h / CC: ~20min)
  - **Depends on:** T6 (工作区设定 Tab)

- **Preview Role** — 保存后展示模拟对话，确认 Agent 身份
  - CEO 评审推迟
  - **Why:** 低成本加分项，降低不确定性
  - **Effort:** S (human: ~30min / CC: ~5min)
  - **Depends on:** T6 (工作区设定 Tab)

## Scene3D + Workspace Resources Ship (v0.3)

Source: `/plan-eng-review` on `feat/scene3d-workspace-resources-ship` (2026-06-05)

### P3 — LOW

- **修正 unify_resources.sql 注释 (F7)**
  - 迁移注释声称 knowledge_parse_jobs.document_id 已指向 resources.id，但实际未实现 ALTER TABLE
  - **Why:** 误导性注释会让后续读者误解 schema 的完整性状态
  - **Action:** 更新注释反映实际状态
  - **Effort:** S (human: 5min / CC: 2min)

- **重命名 knowledge_entities.source_document_id 为 source_resource_id (F8)**
  - 删除 knowledge_documents 后，该列实际存储的是 resources.id，列名已误导
  - **Why:** 新加入的开发者会困惑「source_document_id」指向哪个表
  - **Action:** 新 migration 中重命名列 + 更新所有引用
  - **Effort:** S (human: 30min / CC: 5min)

## Alarm System (v0.1)

Source: `/plan-eng-review` on `feature/alarm` (2026-06-06)

### P2 — MEDIUM

- **告警保留策略 (Alarm Retention Policy)**
  - `alarms` 表无清理机制，随 IoT 设备持续上报数据会无限增长。需添加定期清理 cron 任务：`DELETE FROM alarms WHERE status = 'Resolved' AND created_at < datetime('now', '-90 days')`。
  - **Why:** 防止 alarms 表无限增长影响查询性能
  - **Action:** 在 cron 框架中注册周期任务，默认 90 天保留期可配置
  - **Effort:** S (human: 30min / CC: 10min)
  - **Owner:** TBD

## AI Event Integration (v0.1)

Source: `/plan-eng-review` on `main` (2026-06-15)

### P2 — MEDIUM

- **agent_actions 保留策略 (Agent Actions Retention Policy)**
  - `agent_actions` 表无清理机制，随告警触发 AI 处理会持续增长。需添加定期清理 cron 任务：`DELETE FROM agent_actions WHERE created_at < datetime('now', '-90 days')`。
  - **Why:** 防止 agent_actions 表无限增长影响查询性能
  - **Action:** 在 cron 框架中注册周期任务，和 alarm retention 使用相同模式
  - **Effort:** S (human: 20min / CC: 5min)
  - **Owner:** TBD

## Thing Ontology (from /plan-eng-review 2026-07-22)

### P1 — 物列表 list|tree 视图切换 + 拖拽换父（设计 D3 未交付）
- **What:** 物列表页「列表｜树」视图切换：树视图=全量层级树（默认展开 2 层，当前工作区根起），两视图共享过滤条件；拖拽换父（成环目标实时红框拒绝，合法落点即调更新 API）。
- **Why:** 设计评审 D3 裁决的形态，实现为 table|grid；树数据 API（get_thing_tree）已就绪，只差视图。
- **Context:** /ship 2026-07-27 plan completion 裁决延期。树交互 D12：单击节点进详情，箭头独立展开/收起。后端 parent_id 换父 API（update_thing + cycle 校验）已存在。
- **Effort:** M (human: ~1d / CC: ~1-2h)

### P2 — 物操作审计日志（设计「可观测性」节未交付）
- **What:** 创建/删除/改父/invoke_action 记审计日志（操作者、时间、目标物）。
- **Why:** 设计要求；当前只有 tracing 日志，无持久化审计表。
- **Context:** /ship 2026-07-27 延期项。可参考 alarm/agent_actions 的审计表模式。
- **Effort:** S (human: ~2h / CC: ~20min)

### P2 — E3 WoT Thing Description 导出端点未接
- **What:** `/things/templates/{id}/export/wot` 导出（当前只有 DTDL 导出）。
- **Why:** E3 要求 DTDL/WoT 双向；import 双向已实现，export 只有 DTDL。
- **Context:** import_export.rs 已有 WoT import;导出函数缺 wo­t 序列化。
- **Effort:** S (human: ~2h / CC: ~15min)


### ~~P2 — Events 表保留策略（occurrence-aware）~~ ✅ Completed v0.4.5.0 (2026-07-27)
- EventRetentionExecutor + 全局 cron job（每日 03:17, 90 天）, is_status=0 发生类行按时间清理, is_status=1 状态行永不按时间清除；两个错误形状存量清除函数（cleanup_old_events/clear_acknowledged_events）同步修复

### P2 — E2 A2UI 本体驱动渲染（后续独立分支）
- **What:** 本体驱动的 A2UI 渲染：get_thing_profile 驱动 DeviceCard/DataChart/ControlPanel；invoke_action 前渲染确认面板。
- **Why:** E2 为 CEO 审查（2026-07-22）接受的扩展项，但 mega-branch 中的 a2ui.rs 是死代码且 build_control_panel 硬编码了不存在的控件（电源开关/重启设备，actions:[]），工程评审（2026-07-27 D10）裁决删除。无此 TODO 则已接受的扩展项静默消亡。
- **Context:** 设计文档要求"落地时先验证渲染成熟度"。当前 canvas 工具路径（agent/tools/canvas.rs）消费 LLM 原生 JSONL，builder 方式需要先决定集成点（LLM 直出 vs 服务端 builder）。
- **Depends on:** Thing Ontology mega-branch 落地。
- **Effort:** M (human: ~2d / CC: ~2h)

### P3 — search_knowledge 升级 FTS5 trigram
- **What:** thing_resources 全文检索从 `LIKE '%q%'` 扫描升级为 SQLite FTS5 trigram 虚拟表（含同步触发器）。
- **Why:** 工程评审 D14 裁决本期维持 LIKE（预发布文档量级几十篇无感）；但 search_knowledge 是 Agent 高频调用路径，文档上千篇后全表扫描劣化。FTS5 默认 unicode61 分词对中文无效，需 trigram tokenizer（SQLite ≥3.34）。
- **Context:** 现有 LIKE 实现见 `crates/tenant/src/workspace/repo.rs:636`（图谱拆除后平移至 thing_resources repo）。升级点：建 fts 虚拟表 + INSERT/UPDATE/DELETE 同步触发器 + repo 查询改 MATCH。
- **Depends on:** Thing Ontology mega-branch 落地（thing_resources 表存在后）。
- **Effort:** M (human: ~1d / CC: ~1h)

## Completed


## Thing Ontology Architecture Follow-up (from /plan-ceo-review 2026-07-25)

### ~~P2 — Move thing service SQL to storage layer~~ ✅ Completed 2026-07-27 (eng-review T9, commit "refactor(thing): move service-layer SQL to storage/repo")

## Completed

### P2 — Add dynamic task/config refresh to heartbeat loop (Outside Voice)
- **Completed:** v0.5.0.0 (2026-08-24) — 共享信任句柄写穿（crates/agent/src/runtime/heartbeat/runner.rs trust_handles），运行中 loop 即时生效；live-chain 测试 test_update_trust_config_reaches_running_loop。

### P1 — Wire DropNotifier + DeadLetterQueue
- **Completed:** v0.5.0.0 (2026-08-24) — 2026-08-03 已核实接线（service_manager.rs LoggingDropNotifier + SqliteDeadLetterQueue），本条归档。

### P3 — Cache regex in extract_json
- **Completed:** v0.5.0.0 (2026-08-24) — crates/agent/src/runtime/heartbeat/report.rs 已用 `static JSON_FENCE_RE: LazyLock<Regex>`。

## Documentation Debt (from /ship document-release 2026-08-24)

### P3 — scripts/guards/ 使用与扩展 how-to
- **What:** 为 sql-residence / ddl-only / agent-purity / selftest 写本地运行与新增守卫的指南。
- **Why:** 目前只有 AGENTS.md/CI 的引用级覆盖；新贡献者不知道如何本地跑守卫或添加新守卫。
- **Context:** 守卫脚本在 scripts/guards/，selftest.sh 是自证范例；落点建议 docs/ 或 AGENTS.md 增补一节。

### P3 — crates/authn 消费方文档
- **What:** 为 crates/authn 写 README/how-to（构造注入用法、JwtService、HarmonyOS 变体）。
- **Why:** 目前只有 AGENTS.md/CHANGELOG 的引用级覆盖，消费方无入口文档。

### P3 — [seed] demo_data 进 configuration 参考
- **What:** 把 `[seed] demo_data` 配置键写进 docs/getting-started/configuration.md（或等效参考文档）。
- **Why:** 目前只在 CHANGELOG 与 app_settings.example.toml 有记录；0.5.0.0 起默认 false，用户需要权威参考。
