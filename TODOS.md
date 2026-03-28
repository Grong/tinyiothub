# TODOS

> Organized by skill/component, then priority (P0 at top through P4, then Completed at bottom)

## Edge Intelligence Agent

**Completed:**

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

