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

### P1 — HIGH

- **Gateway e2e test with mock gateway**
  - Core pairing flow crosses 3 systems (gateway → broker → platform), unit tests can't cover it. CI e2e with `tests/e2e/docker-compose.yml` + mosquitto + mock MQTT gateway that sends announce, waits for ack, sends telemetry.
  - **Depends on:** edge/ base implementation complete
  - **Effort:** M (human: 2 days / CC: 30min)
  - **Owner:** TBD

## Completed

