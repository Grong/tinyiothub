#!/usr/bin/env bash
# Legacy writer / registration boundary guard (Task 19 / spec T14).
#
# The Harness Core has one writer per fact:
#   session_events / effect_intents / decisions / commands (workspace DB)
#   task_wake_outbox / device_jobs projection (workspace DB)
#   hub_ledger telemetry (platform DB; remains a pure telemetry ledger)
#
# Task 19C4 retired the platform Task fact plane: the platform `agent_tasks`,
# `task_wake_outbox`, `device_jobs`, and `task_approvals` tables have no
# production reader or writer. The only exceptions are the harness owner
# modules (workspace DB SQL), the read-only cross-workspace admin sweep, the
# cfg(test)-gated legacy Job fixture owner, and the autonomous agent-loop run
# history. `hub_ledger` (runtime.v1) is device/watch/evidence telemetry only.
# This guard freezes that boundary and rejects hand-maintained tool / event /
# status lists in web/src.
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

# production_lines <file>
# Emit `path:line:content` for the production scope only. Inline `#[cfg(test)]`
# modules AND single test-gated items (mod/use/fn/impl/struct/...) are tracked
# by scope — not by stopping at the first token — so production code after a
# test block is still scanned. Items gated with
# `#[cfg(any(test, feature = "testing"))]` remain in scope; device_job.rs is
# verified separately by DEVICE_JOB_UNGATED.
production_lines() {
  awk -v f="$1" '
    BEGIN { in_test = 0; opened = 0; depth = 0; test_pending = 0; prev = ""; prev_no = 0 }
    in_test {
      n = gsub(/{/, "{"); m = gsub(/}/, "}")
      depth += n - m
      if (n > 0) opened = 1
      if (opened && depth <= 0) { in_test = 0; opened = 0; depth = 0 }
      next
    }
    /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/ { test_pending = 1; next }
    test_pending {
      if ($0 ~ /^[[:space:]]*$/) next
      if ($0 ~ /^[[:space:]]*#\[/) next
      if ($0 ~ /^[[:space:]]*mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*\{/) {
        test_pending = 0; in_test = 1; opened = 0; depth = 0
        n = gsub(/{/, "{"); m = gsub(/}/, "}")
        depth += n - m; if (n > 0) opened = 1
        next
      }
      if ($0 ~ /^[[:space:]]*(mod[[:space:]]+[A-Za-z0-9_]+[[:space:]]*;|use[[:space:]].*;|[A-Za-z_].*;)[[:space:]]*$/) {
        test_pending = 0; next
      }
      # A test-only fn/impl/struct/const: skip its brace block.
      test_pending = 0; in_test = 1; opened = 0; depth = 0
      n = gsub(/{/, "{"); m = gsub(/}/, "}")
      depth += n - m; if (n > 0) opened = 1
      if (opened && depth <= 0) { in_test = 0; opened = 0; depth = 0 }
      next
    }
    {
      # Multi-line SQL: keyword at end of the previous production line and a
      # retired table at the start of this one.
      if (prev ~ /(INSERT[[:space:]]+(OR[[:space:]]+[A-Z]+[[:space:]]+)?INTO|UPDATE|DELETE[[:space:]]+FROM|FROM|JOIN)[[:space:]]*$/ \
          && $0 ~ /^[[:space:]]*[\\"]*(agent_tasks|agent_runs|task_wake_outbox|task_approvals|task_finalize_intents|device_jobs)[\\"]*/) {
        print f ":" prev_no ": (multi-line) " prev " " $0
      }
      print f ":" FNR ":" $0
      prev = $0; prev_no = FNR
    }
  ' "$1"
}

# check_production <name> <kind:read|write> <allow map "file:table" lines>
# Fails on a production read/write of a retired table that is not in the
# per-file/per-table allow map. Test fixtures inside test scope are ignored.
check_production() {
  local name="$1" kind="$2" allow="$3"
  local tables='(agent_tasks|agent_runs|task_wake_outbox|task_approvals|task_finalize_intents|device_jobs)'
  # Optional Rust-string escaping and SQL quoting around an identifier.
  local q='[\\"]*'
  local pattern
  if [ "$kind" = "write" ]; then
    pattern="(INSERT[[:space:]]+(OR[[:space:]]+[A-Z]+[[:space:]]+)?INTO[[:space:]]+${q}${tables}${q}|UPDATE[[:space:]]+${q}${tables}${q}|DELETE[[:space:]]+FROM[[:space:]]+${q}${tables}${q})"
  else
    pattern="(FROM|JOIN)[[:space:]]+${q}${tables}${q}"
  fi
  local hits
  hits=$(for file in $(rg --files crates apps/cloud -g '*.rs' \
        -g '!**/target/**' -g '!**/tests/**' -g '!**/benches/**' -g '!**/*_tests.rs'); do
      production_lines "$file"
    done | grep -E "$pattern" || true)
  local violations=""
  while IFS= read -r hit; do
    [ -z "$hit" ] && continue
    local file="${hit%%:*}"
    local table
    table=$(printf '%s' "$hit" \
      | grep -oE "(INSERT[[:space:]]+(OR[[:space:]]+[A-Z]+[[:space:]]+)?INTO|UPDATE|DELETE[[:space:]]+FROM|FROM|JOIN)[[:space:]]+${q}${tables}${q}" \
      | grep -oE '(agent_tasks|agent_runs|task_wake_outbox|task_approvals|task_finalize_intents|device_jobs)' \
      | head -1)
    if [ -n "$table" ] && printf '%s\n' "$allow" | grep -qx "$file:$table"; then
      continue
    fi
    violations="${violations:+$violations
}$hit"
  done <<< "$hits"
  if [ -n "$violations" ]; then
    echo "$violations"
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
  "runtime.v1 append outside telemetry writers" \
  'append_runtime_event(_tx)?' \
  '^(crates/db/src/(runtime_event|task_approval|device_job|watch|media_evidence|agent_runs)\.rs|apps/cloud/src/domains/agent/runtime/stream\.rs):'

check_rust \
  "runtime.v1 kind outside the ledger writer" \
  'RUNTIME_EVENT_KIND|runtime\.v1' \
  '^(crates/db/src/(runtime_event|agent_runs|watch)\.rs|apps/cloud/src/domains/agent/runtime/stream\.rs|apps/cloud/src/domains/agent/runtime/api_cutover_tests\.rs):'

check_rust \
  "runtime event construction outside telemetry writers" \
  'RuntimeEventInput\s*\{' \
  '^(crates/db/src/(runtime_event|task_approval|device_job|watch|media_evidence|agent_runs)\.rs|apps/cloud/src/domains/agent/runtime/(stream|tasks)\.rs|apps/cloud/src/domains/agent/host/tools/thing/device\.rs|apps/cloud/src/domains/agent/tools/capabilities\.rs):'

# ── Retired platform Task/Job fact plane (Task 19C4) ───────
# Task/Run/Wake/Job facts live only in each workspace Harness generation.
# The platform tables survive for migrations and test fixtures, but production
# code must not read or write them. `agent_runs` is exempt only for the
# autonomous agent-loop persistence (trigger_type/outcome rows) which is not
# the Task plane; policy.rs/ticket.rs read that same autonomous history.
WRITE_OWNERS="crates/db/src/harness_task.rs:agent_tasks
crates/db/src/harness_task.rs:agent_runs
crates/db/src/harness_task.rs:task_wake_outbox
crates/db/src/harness_task.rs:task_finalize_intents
crates/db/src/harness_workflow.rs:agent_tasks
crates/db/src/harness_workflow.rs:agent_runs
crates/db/src/harness_workflow.rs:task_wake_outbox
crates/db/src/harness_effect.rs:agent_tasks
crates/db/src/harness_effect.rs:agent_runs
crates/db/src/harness_effect.rs:device_jobs
crates/db/src/harness_effect.rs:task_wake_outbox
crates/db/src/harness_job.rs:device_jobs
crates/db/src/harness_job.rs:task_wake_outbox
crates/db/src/harness_command.rs:agent_tasks
crates/db/src/harness_command.rs:agent_runs
crates/db/src/harness_command.rs:task_wake_outbox
crates/db/src/harness_runtime_api.rs:task_wake_outbox
crates/db/src/harness_supervisor.rs:task_wake_outbox
crates/db/src/harness_event.rs:agent_tasks
crates/db/src/harness_event.rs:agent_runs
crates/db/src/harness_simulation.rs:agent_tasks
crates/db/src/harness_simulation.rs:agent_runs
crates/db/src/runtime_admin.rs:task_wake_outbox
crates/db/src/device_job.rs:device_jobs
crates/db/src/device_job.rs:task_wake_outbox
crates/db/src/agent_runs.rs:agent_runs"

READ_OWNERS="crates/db/src/harness_task.rs:agent_tasks
crates/db/src/harness_task.rs:agent_runs
crates/db/src/harness_task.rs:task_wake_outbox
crates/db/src/harness_task.rs:task_finalize_intents
crates/db/src/harness_workflow.rs:agent_tasks
crates/db/src/harness_workflow.rs:agent_runs
crates/db/src/harness_workflow.rs:task_wake_outbox
crates/db/src/harness_workflow.rs:device_jobs
crates/db/src/harness_effect.rs:agent_tasks
crates/db/src/harness_effect.rs:agent_runs
crates/db/src/harness_effect.rs:device_jobs
crates/db/src/harness_effect.rs:task_wake_outbox
crates/db/src/harness_job.rs:device_jobs
crates/db/src/harness_job.rs:task_wake_outbox
crates/db/src/harness_command.rs:agent_tasks
crates/db/src/harness_command.rs:agent_runs
crates/db/src/harness_command.rs:task_wake_outbox
crates/db/src/harness_runtime_api.rs:agent_tasks
crates/db/src/harness_runtime_api.rs:agent_runs
crates/db/src/harness_runtime_api.rs:device_jobs
crates/db/src/harness_runtime_api.rs:task_wake_outbox
crates/db/src/harness_supervisor.rs:agent_tasks
crates/db/src/harness_supervisor.rs:task_wake_outbox
crates/db/src/harness_event.rs:agent_tasks
crates/db/src/harness_event.rs:agent_runs
crates/db/src/harness_simulation.rs:agent_tasks
crates/db/src/harness_simulation.rs:agent_runs
crates/db/src/runtime_admin.rs:agent_tasks
crates/db/src/runtime_admin.rs:agent_runs
crates/db/src/runtime_admin.rs:task_wake_outbox
crates/db/src/device_job.rs:device_jobs
crates/db/src/device_job.rs:task_wake_outbox
crates/db/src/agent_runs.rs:agent_runs
crates/db/src/policy.rs:agent_runs
crates/db/src/ticket.rs:agent_runs"

check_production \
  "platform Task/Run/Wake/Job write outside the Harness owner" \
  write "$WRITE_OWNERS"

check_production \
  "platform Task/Run/Wake/Job read outside the Harness owner" \
  read "$READ_OWNERS"

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
  # Inline test modules are gated as a whole; test SQL is not production.
  # Selftest injections are appended AFTER the module, so the module boundary
  # must be tracked instead of stopping at the first #[cfg(test)].
  /^#\[cfg\(test\)\]$/ { test_pending = 1; next }
  in_tests {
    if ($0 ~ /^\}/) { in_tests = 0 }
    next
  }
  test_pending && /^mod [A-Za-z0-9_]+ \{/ { in_tests = 1; test_pending = 0; next }
  test_pending { test_pending = 0 }
  # Attributes accumulate: an extra #[allow]/#[inline] between the gate and
  # the item must not clear the pending gate. Only the item line consumes it.
  /^[[:space:]]*#\[/ {
    if ($0 ~ /#\[cfg\((any\(test, feature = "testing"\)|test)\)\]/) pending = 1
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
    if ($0 ~ /(INSERT INTO device_jobs|UPDATE device_jobs|FROM device_jobs|INSERT INTO task_wake_outbox|UPDATE task_wake_outbox|FROM task_wake_outbox)/ && !gated) {
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

if [ "$FAIL" -ne 0 ]; then
  echo "   ✅ New facts must be written through Harness transitions:"
  echo "      Db::apply_workspace_transition / append_session_event / effect_intents / decisions"
  echo "   ✅ Task/Run/Wake/Job facts live in the owning workspace Harness generation."
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
