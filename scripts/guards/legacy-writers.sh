#!/usr/bin/env bash
# Legacy writer / registration boundary guard (Task 19 / spec T14).
#
# The Harness Core has one writer per fact:
#   session_events / effect_intents / decisions / commands (workspace DB)
#   task_wake_outbox / device_jobs projection (workspace DB)
#   hub_ledger telemetry (platform DB; remains a pure telemetry ledger)
#
# The pre-Harness Runtime API still has adapters under runtime/ and
# shared/runtime_ports.rs. They are allowed to call the old platform writers so
# the existing Runtime endpoints keep working, but no other production module
# may create a second write path. This guard freezes that boundary: new code in
# workflows/, harness.rs, host/ (except the legacy device recorder), crates/, or
# web/src cannot reach the old writers or reintroduce a hand-maintained tool /
# event / status list.
#
# Regex hits in comments/docs are reported as violations too; that is
# intentional for identifier-derived guards. Migration files are exempt only for
# their own CREATE/DROP DDL.
set -u
cd "$(git rev-parse --show-toplevel)"

FAIL=0
FAILED_CHECKS=()

fail() {
  FAILED_CHECKS+=("$1")
  echo "❌ LEGACY WRITER VIOLATION: $1"
  FAIL=1
}

# check <name> <regex> <allow-regex>
# Scans Rust production code (tests/benches/target excluded); the allow regex
# is matched against "path:line:" prefixes of each hit.
check_rust() {
  local name="$1" pattern="$2" allow="$3"
  local hits
  hits=$(rg -n --glob '*.rs' \
      --glob '!**/target/**' \
      --glob '!**/tests/**' \
      --glob '!**/benches/**' \
      "$pattern" crates apps/cloud 2>/dev/null \
    | grep -vE "$allow" || true)
  if [ -n "$hits" ]; then
    echo "$hits"
    fail "$name"
  fi
}

# ── Old runtime event writer ───────────────────────────────
# `runtime/api_cutover_tests.rs` is a #[cfg(test)]-only module; its assertions
# intentionally name the legacy identifiers. It is allow-listed by exact path,
# not by pattern, so production hits still fail.
# Only the legacy runtime adapters and the low-level db transaction writer may
# append runtime.v1 events. The Harness path uses append_session_event.
check_rust \
  "runtime.v1 append outside legacy adapters" \
  'append_runtime_event(_tx)?' \
  '^(crates/db/src/(runtime_event|runtime_task|task_approval|device_job|watch|media_evidence|agent_runs)\.rs|apps/cloud/src/domains/agent/runtime/(stream|tasks)\.rs):'

check_rust \
  "runtime.v1 kind outside the ledger writer" \
  'RUNTIME_EVENT_KIND|runtime\.v1' \
  '^(crates/db/src/(runtime_event|runtime_task|agent_runs|watch)\.rs|apps/cloud/src/domains/agent/runtime/stream\.rs|apps/cloud/src/domains/agent/runtime/api_cutover_tests\.rs):'

check_rust \
  "runtime event construction outside legacy adapters" \
  'RuntimeEventInput\s*\{' \
  '^(crates/db/src/(runtime_event|runtime_task|task_approval|device_job|watch|media_evidence|agent_runs)\.rs|apps/cloud/src/domains/agent/runtime/(stream|tasks)\.rs|apps/cloud/src/domains/agent/host/tools/thing/device\.rs|apps/cloud/src/domains/agent/tools/capabilities\.rs):'

# ── Duplicate approval writers (platform task_approvals) ───
# decisions is the only Harness approval source (Task 19C2). The platform
# table survives for the legacy read projection only; it has no writers.
check_rust \
  "task approval writer outside the legacy adapter" \
  '(create_task_approval_and_events|decide_task_approval_and_event|expire_task_approvals_and_events|cancel_task_approvals_and_events|submit_approved_job_and_events|task_approval_by_decision_event|list_recoverable_task_approvals)' \
  '^(crates/db/src/task_approval\.rs|apps/cloud/src/domains/agent/runtime/approvals\.rs|apps/cloud/src/shared/runtime_ports\.rs):'

check_rust \
  "platform task_approvals write" \
  '(INSERT INTO task_approvals|UPDATE task_approvals)' \
  '^__no_allow_list__$'

# ── Independent platform Job state mutation ────────────────
# The approved/legacy Job flow must go through the single device_job owner;
# approval code calls the shared projection helper instead of writing SQL.
check_rust \
  "device_jobs write outside the Job owner" \
  '(INSERT INTO device_jobs|UPDATE device_jobs)' \
  '^(crates/db/src/(device_job|harness_effect|harness_job)\.rs|apps/cloud/src/domains/agent/host/autonomous_factory\.rs|apps/cloud/src/domains/agent/runtime/api_cutover_tests\.rs):'
# task_approval.rs deliberately has no device_jobs SQL: it calls
# crate::device_job::insert_job_projection inside its own transaction.

# ── Retired platform Job writer stays test-gated (19C3 M3) ─
# device_job.rs is allow-listed above only because it holds the retired writer.
# Every INSERT/UPDATE device_jobs there must sit in an item guarded by
# #[cfg(any(test, feature = "testing"))]; otherwise un-gating the writer would
# silently reintroduce a production platform write.
DEVICE_JOB_UNGATED=$(awk '
  # Attributes accumulate: an extra #[allow]/#[inline] between the gate and
  # the item must not clear the pending gate. Only the item line consumes it.
  /^[[:space:]]*#\[/ {
    if ($0 ~ /#\[cfg\(any\(test, feature = "testing"\)\)\]/) pending = 1
    next
  }
  /^[[:space:]]*$/ { next }
  {
    match($0, /^[[:space:]]*/)
    indent = RLENGTH
    is_fn = ($0 ~ /^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]/)
    is_item = ($0 ~ /^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?(const|static|type|struct|enum|trait|impl|mod|use|macro_rules!)[[:space:]!]/)
    # Only module/impl-level items change the gate. Statements inside a
    # function body keep the gate of their enclosing fn.
    if (indent <= 4 && (is_fn || is_item)) {
      gated = pending
      pending = 0
    }
    if ($0 ~ /(INSERT INTO device_jobs|UPDATE device_jobs)/ && !gated) {
      print FILENAME ":" FNR ":" $0
    }
  }
' crates/db/src/device_job.rs)
if [ -n "$DEVICE_JOB_UNGATED" ]; then
  echo "$DEVICE_JOB_UNGATED"
  fail "platform device_jobs writer outside cfg(any(test, feature = \"testing\"))"
fi

# autonomous_factory.rs may write device_jobs only inside its #[cfg(test)]
# module (its fixture seeds legacy Jobs).
AUTONOMOUS_FIRST_TEST=$(grep -nE '^#\[cfg\(test\)\]' apps/cloud/src/domains/agent/host/autonomous_factory.rs | head -1 | cut -d: -f1)
if [ -z "$AUTONOMOUS_FIRST_TEST" ]; then
  fail "autonomous_factory.rs lost its #[cfg(test)] module; device_jobs writes must stay test-only"
  AUTONOMOUS_FIRST_TEST=0
fi
AUTONOMOUS_WRITES=$(rg -n '(INSERT INTO device_jobs|UPDATE device_jobs)' \
    apps/cloud/src/domains/agent/host/autonomous_factory.rs 2>/dev/null \
  | awk -F: -v limit="$AUTONOMOUS_FIRST_TEST" '$1 < limit {print}' || true)
if [ -n "$AUTONOMOUS_WRITES" ]; then
  echo "$AUTONOMOUS_WRITES"
  fail "device_jobs write in autonomous_factory outside its #[cfg(test)] module"
fi

check_rust \
  "Job writer outside the legacy runtime adapter" \
  '(create_job_and_event|mark_running_and_event|finish_job_and_event|update_progress_revision_and_event|request_stop_and_event)' \
  '^(crates/db/src/(device_job|task_approval)\.rs|apps/cloud/src/shared/runtime_ports\.rs):'

# ── Independently transitioned platform Task facts ─────────
check_rust \
  "Task writer outside the legacy runtime adapter" \
  '(create_task_with_wake_and_event|transition_task_and_event|begin_run_and_event|finish_run_and_event|bind_current_run_and_event)' \
  '^(crates/db/src/(runtime_task|runtime_event)\.rs|apps/cloud/src/domains/agent/runtime/(mod|stream)\.rs):'

if [ "$FAIL" -ne 0 ]; then
  echo "   ✅ New facts must be written through Harness transitions:"
  echo "      Db::apply_workspace_transition / append_session_event / effect_intents / decisions"
  echo "   ✅ Legacy Runtime adapters stay in runtime/ and shared/runtime_ports.rs; do not add a new caller."
  exit 1
fi

# ── Hand-maintained web tool / event / state lists ─────────
# Status copy is single-sourced in web/src/ui/views/harness-status.ts and
# exposed through runtime-status.ts. Tool/event allowlists are derived from
# event facts, not a frontend Set.
WEB_HITS=$(rg -n --glob '*.ts' --glob '!**/*.test.ts' \
    '(const associatedTools|const toolEvents|const readOnlyTools|const eventTitles|const taskStatusLabels|const taskStatusTones|taskStatuses|jobStates|approvalStates|terminalTasks|pausableStatuses|cancellableStatuses|terminalStatuses)' \
    web/src 2>/dev/null || true)
if [ -n "$WEB_HITS" ]; then
  echo "$WEB_HITS"
  fail "hand-maintained web tool/event/status list"
fi
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi

echo "✅ Legacy writer boundary guard passed"
