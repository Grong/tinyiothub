# TODOS

> **最新完整 TODO 清单已迁移至:** `docs/superpowers/plans/2026-04-14-todo-audit-and-cleanup-plan.md`
> 本文档保留 Edge Intelligence Agent 历史记录，新项目 TODO 请查阅上方计划。

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
  - `registry.rs:48-50` loads arbitrary `.so` and calls `init()` with full process privileges
  - `validator.rs:20-22` dry-load triggers `__attribute__((constructor))` before any validation
  - **Action:** Implement admin-only gate for driver installation (quick fix), plan subprocess sandbox for v0.2.x
  - **Owner:** TBD

### P1 — HIGH

- **[#41] TemplateExporter secret stripping is shallow**
  - Only strips top-level keys; nested JSON like `{"auth": {"password": "secret"}}` leaks
  - Missing variants: `passwd`, `key`, `credential`, `cert`
  - **Action:** Recursive JSON traversal + expanded sensitive key list
  - **Owner:** TBD

### P2 — MEDIUM

- **[#42] Exported templates lose device properties and commands**
  - `exporter.rs:31-32` creates empty `properties` and `commands` vectors
  - Users export a configured device and get a hollow template
  - **Action:** Map `device.properties` → `PropertyTemplate`, `device.commands` → `CommandTemplate`
  - **Owner:** TBD

- **[#44] Add unit tests for DriverRegistry failure paths**
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

## Completed

