# TODOS

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

## Completed

