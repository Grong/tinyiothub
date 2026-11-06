#!/usr/bin/env bash
# CI 守卫自证（T19/F18）：每个守卫必须在故意违规时失败、清理后通过。
# 坏正则/坏逻辑若静默通过，此脚本即红。全程 trap 恢复现场。
set -u
cd "$(git rev-parse --show-toplevel)"

SELFTEST_FAIL=0
CLEANUP=()
# 对抗性评审 F1：恢复必须基于事前备份——git checkout 会摧毁用户的
# 未提交改动（根清单本脚本从不修改；成员清单即使提前退出也会被
# checkout）。只在真正备份过时才恢复。
AGENT_TOML_BAK=""
DEVICE_JOB_BAK=""
AUTONOMOUS_FACTORY_BAK=""
THING_AGENT_DIR_CREATED=0
cleanup() {
  for path in "${CLEANUP[@]}"; do
    if [ -e "$path" ]; then rm -f "$path"; fi
  done
  if [ "$THING_AGENT_DIR_CREATED" -eq 1 ]; then
    rmdir crates/agent/src/runtime/thing_agent 2>/dev/null || true
  fi
  if [ -n "$AGENT_TOML_BAK" ] && [ -f "$AGENT_TOML_BAK" ]; then
    cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
    rm -f "$AGENT_TOML_BAK"
  fi
  if [ -n "$DEVICE_JOB_BAK" ] && [ -f "$DEVICE_JOB_BAK" ]; then
    cp "$DEVICE_JOB_BAK" crates/db/src/device_job.rs
    rm -f "$DEVICE_JOB_BAK"
  fi
  if [ -n "$AUTONOMOUS_FACTORY_BAK" ] && [ -f "$AUTONOMOUS_FACTORY_BAK" ]; then
    cp "$AUTONOMOUS_FACTORY_BAK" apps/cloud/src/domains/agent/host/autonomous_factory.rs
    rm -f "$AUTONOMOUS_FACTORY_BAK"
  fi
}
trap cleanup EXIT
AGENT_TOML_BAK=$(mktemp)
cp crates/agent/Cargo.toml "$AGENT_TOML_BAK"

expect_fails() {
  local name="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "❌ SELF-TEST: $name did NOT fire on deliberate violation"
    SELFTEST_FAIL=1
  else
    echo "✅ SELF-TEST: $name fired as expected"
  fi
}
expect_passes() {
  local name="$1"; shift
  if "$@" >/dev/null 2>&1; then
    echo "✅ SELF-TEST: $name clean after revert"
  else
    echo "❌ SELF-TEST: $name still failing after revert"
    SELFTEST_FAIL=1
  fi
}

# ── DDL-only ───────────────────────────────────────────────
EVIL_MIG=crates/db/migrations/99999999999999_selftest_evil.sql
printf 'INSERT OR IGNORE INTO evil VALUES (1);\n' > "$EVIL_MIG"
CLEANUP+=("$EVIL_MIG")
expect_fails "ddl-only (INSERT OR IGNORE)" bash scripts/guards/ddl-only.sh
rm -f "$EVIL_MIG"
printf 'CREATE TABLE t (a TEXT); INSERT\nINTO evil VALUES (1);\n' > "$EVIL_MIG"
expect_fails "ddl-only (multiline INSERT)" bash scripts/guards/ddl-only.sh
rm -f "$EVIL_MIG"
printf "CREATE TRIGGER t AFTER x BEGIN SELECT '--'; DELETE FROM evil; END;\n" > "$EVIL_MIG"
expect_fails "ddl-only (string-literal comment smuggle)" bash scripts/guards/ddl-only.sh
rm -f "$EVIL_MIG"
expect_passes "ddl-only" bash scripts/guards/ddl-only.sh

# ── SQL residence ──────────────────────────────────────────
EVIL_RS=apps/cloud/src/domains/selftest_evil.rs
printf 'pub async fn evil(p: &sqlx::SqlitePool) { sqlx::query("SELECT 1").execute(p).await.unwrap(); }\n' > "$EVIL_RS"
CLEANUP+=("$EVIL_RS")
expect_fails "sql-residence (raw query)" bash scripts/guards/sql-residence.sh
rm -f "$EVIL_RS"
printf 'pub async fn evil(p: &sqlx::SqlitePool) { sqlx::query("SELECT 1").execute(p).await.unwrap(); } // guard-exempt\n' > "$EVIL_RS"
expect_fails "sql-residence (reason-less exemption)" bash scripts/guards/sql-residence.sh
rm -f "$EVIL_RS"
# F14 核心场景：测试标记之下的无属性生产 fn 藏 SQL —— 不得隐形。
cat > "$EVIL_RS" << 'RS'
#[cfg(test)]
fn test_helper() {}

pub async fn production_fn(p: &sqlx::SqlitePool) {
    sqlx::query("SELECT 1").execute(p).await.unwrap();
}
RS
expect_fails "sql-residence (production SQL hidden below test marker)" bash scripts/guards/sql-residence.sh
rm -f "$EVIL_RS"
# 回归（scene_instantiator.rs 假阳性）：fn 体内缩进的语句级 #[cfg(test)]
# seam 不是截断锚点；列 0 测试模块内的 SQL 必须放行。
cat > "$EVIL_RS" << 'RS'
pub async fn production_fn(p: &sqlx::SqlitePool) {
    #[cfg(test)]
    maybe_seam(p).await;
}

fn other_prod() {}

#[cfg(test)]
mod tests {
    async fn inject(p: &sqlx::SqlitePool) {
        sqlx::query("INSERT INTO t VALUES (1)").execute(p).await.unwrap();
    }
}
RS
expect_passes "sql-residence (statement-level seam + test-module SQL)" bash scripts/guards/sql-residence.sh
rm -f "$EVIL_RS"
expect_passes "sql-residence" bash scripts/guards/sql-residence.sh

# ── Agent purity ───────────────────────────────────────────
EVIL_AGENT=crates/agent/src/selftest_evil.rs
printf 'use axum::Router;\n' > "$EVIL_AGENT"
CLEANUP+=("$EVIL_AGENT")
expect_fails "agent-purity (source use axum)" bash scripts/guards/agent-purity.sh
printf 'use tinyiothub_runtime::DataServer;\n' > "$EVIL_AGENT"
expect_fails "agent-purity (source use Device Hub)" bash scripts/guards/agent-purity.sh
rm -f "$EVIL_AGENT"
printf '\nweb = { package = "axum" }\n' >> crates/agent/Cargo.toml
expect_fails "agent-purity (renamed dep)" bash scripts/guards/agent-purity.sh
cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
printf '\n[dependencies.axum]\nversion = "0.8"\n' >> crates/agent/Cargo.toml
expect_fails "agent-purity (sub-table dep)" bash scripts/guards/agent-purity.sh
cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
# 注入到 [dependencies] 段内（>> 会落在文件末尾的 [lints] 段下）。
sed -i.bak 's/^\[dependencies\]$/[dependencies]\nsqlx = { workspace = true }/' crates/agent/Cargo.toml && rm -f crates/agent/Cargo.toml.bak
expect_fails "agent-purity (sqlx workspace dep)" bash scripts/guards/agent-purity.sh
if cargo tree -p agent --prefix none --format "{p}" 2>/dev/null | grep -qE "^sqlx"; then
  echo "✅ SELF-TEST: tree guard backstop spots injected sqlx"
else
  echo "❌ SELF-TEST: tree guard missed injected sqlx"
  SELFTEST_FAIL=1
fi
cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
if ! grep -qE '^runtime\s*=' crates/agent/Cargo.toml; then
  sed -i.bak 's/^\[dependencies\]$/[dependencies]\nruntime = { workspace = true }/' crates/agent/Cargo.toml && rm -f crates/agent/Cargo.toml.bak
fi
expect_fails "agent-purity (Device Hub dependency)" bash scripts/guards/agent-purity.sh
cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
THING_AGENT_DIR=crates/agent/src/runtime/thing_agent
if [ ! -d "$THING_AGENT_DIR" ]; then
  mkdir -p "$THING_AGENT_DIR"
  THING_AGENT_DIR_CREATED=1
fi
expect_fails "agent-purity (thing-agent application dir)" bash scripts/guards/agent-purity.sh
if [ "$THING_AGENT_DIR_CREATED" -eq 1 ]; then
  rmdir "$THING_AGENT_DIR"
  THING_AGENT_DIR_CREATED=0
fi
expect_passes "agent-purity" bash scripts/guards/agent-purity.sh

# ── Legacy writer boundary ─────────────────────────────────
EVIL_LEGACY_RS=apps/cloud/src/domains/agent/workflows/selftest_legacy_evil.rs
printf 'pub fn evil() { append_runtime_event(); }\n' > "$EVIL_LEGACY_RS"
CLEANUP+=("$EVIL_LEGACY_RS")
expect_fails "legacy-writers (old runtime event append)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_RS"
printf '%s\n' 'pub fn evil() { let _ = "UPDATE device_jobs SET state = running"; }' > "$EVIL_LEGACY_RS"
expect_fails "legacy-writers (independent Job state write)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_RS"
printf 'pub fn evil() { task_approval_by_decision_event(); }\n' > "$EVIL_LEGACY_RS"
expect_fails "legacy-writers (duplicate approval writer)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_RS"
printf '%s\n' 'pub fn evil() { let _ = "INSERT INTO task_approvals VALUES (1)"; }' > "$EVIL_LEGACY_RS"
expect_fails "legacy-writers (platform approval write)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_RS"
EVIL_LEGACY_WEB=web/src/ui/views/selftest_legacy_evil.ts
printf 'const associatedTools = new Set(["read_device"]);\n' > "$EVIL_LEGACY_WEB"
CLEANUP+=("$EVIL_LEGACY_WEB")
expect_fails "legacy-writers (web tool allowlist)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_WEB"
printf 'const taskStatusLabels = { active: "Active" };\n' > "$EVIL_LEGACY_WEB"
expect_fails "legacy-writers (web duplicate status map)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_WEB"
printf 'const jobStates = ["accepted"];\n' > "$EVIL_LEGACY_WEB"
expect_fails "legacy-writers (removed job state list)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_WEB"
printf 'const terminalTasks = [];\n' > "$EVIL_LEGACY_WEB"
expect_fails "legacy-writers (removed terminal task list)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_WEB"
# 19C3 M3: un-gating the retired platform writer must fail, and a raw write
# added to autonomous_factory before its #[cfg(test)] module must fail.
DEVICE_JOB_BAK=$(mktemp)
cp crates/db/src/device_job.rs "$DEVICE_JOB_BAK"
printf '\npub fn guard_selftest_ungated() { let _ = "INSERT INTO device_jobs VALUES (1)"; }\n' >> crates/db/src/device_job.rs
expect_fails "legacy-writers (un-gated platform Job writer)" bash scripts/guards/legacy-writers.sh
cp "$DEVICE_JOB_BAK" crates/db/src/device_job.rs
rm -f "$DEVICE_JOB_BAK"
DEVICE_JOB_BAK=""
# 19C3 round 2: a non-fn item between gated functions must not inherit the
# previous fn's gate.
DEVICE_JOB_BAK=$(mktemp)
cp crates/db/src/device_job.rs "$DEVICE_JOB_BAK"
printf '\n#[cfg(any(test, feature = "testing"))]\npub fn guard_selftest_gated() {}\nconst GUARD_SELFTEST: &str = "INSERT INTO device_jobs VALUES (1)";\n' >> crates/db/src/device_job.rs
expect_fails "legacy-writers (write in a non-fn item after a gated fn)" bash scripts/guards/legacy-writers.sh
cp "$DEVICE_JOB_BAK" crates/db/src/device_job.rs
rm -f "$DEVICE_JOB_BAK"
DEVICE_JOB_BAK=""
# 19C3 round 2: an extra attribute between the gate and the item must keep the
# gate (otherwise gated code false-positives).
DEVICE_JOB_BAK=$(mktemp)
cp crates/db/src/device_job.rs "$DEVICE_JOB_BAK"
printf '\n#[cfg(any(test, feature = "testing"))]\n#[allow(dead_code)]\npub fn guard_selftest_extra_attr() { let _ = "INSERT INTO device_jobs VALUES (1)"; }\n' >> crates/db/src/device_job.rs
expect_passes "legacy-writers (extra attribute after the test gate)" bash scripts/guards/legacy-writers.sh
cp "$DEVICE_JOB_BAK" crates/db/src/device_job.rs
rm -f "$DEVICE_JOB_BAK"
DEVICE_JOB_BAK=""
AUTONOMOUS_FACTORY_BAK=$(mktemp)
cp apps/cloud/src/domains/agent/host/autonomous_factory.rs "$AUTONOMOUS_FACTORY_BAK"
sed -i.bak '1a\
pub fn guard_selftest_ungated() { let _ = "UPDATE device_jobs SET state = 1"; }
' apps/cloud/src/domains/agent/host/autonomous_factory.rs && rm -f apps/cloud/src/domains/agent/host/autonomous_factory.rs.bak
expect_fails "legacy-writers (autonomous_factory production Job write)" bash scripts/guards/legacy-writers.sh
cp "$AUTONOMOUS_FACTORY_BAK" apps/cloud/src/domains/agent/host/autonomous_factory.rs
rm -f "$AUTONOMOUS_FACTORY_BAK"
AUTONOMOUS_FACTORY_BAK=""
# 19C4: every retired platform Task/Run/Wake/Job table must reject an injected
# production write and an injected production read, in a file with no
# #[cfg(test)] module. Real fixtures live inside test modules and are stripped.
for table in agent_tasks agent_runs task_wake_outbox task_approvals task_finalize_intents device_jobs; do
  printf 'pub fn evil() { let _ = "INSERT INTO %s VALUES (1)"; }\n' "$table" > "$EVIL_LEGACY_RS"
  expect_fails "legacy-writers (platform $table write)" bash scripts/guards/legacy-writers.sh
  printf 'pub fn evil() { let _ = "SELECT * FROM %s"; }\n' "$table" > "$EVIL_LEGACY_RS"
  expect_fails "legacy-writers (platform $table read)" bash scripts/guards/legacy-writers.sh
done
# H1: production code AFTER an inline test module is still scanned.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
mod tests {
    fn fixture() { let _ = "INSERT INTO agent_tasks VALUES (1)"; }
}
pub fn evil_after_tests() { let _ = "SELECT * FROM agent_tasks"; }
RS
expect_fails "legacy-writers (production read after an inline test module)" bash scripts/guards/legacy-writers.sh
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
mod tests {
    fn fixture() { let _ = "SELECT * FROM agent_tasks"; }
}
pub fn evil_after_tests() { let _ = "UPDATE agent_tasks SET status = 'x'"; }
RS
expect_fails "legacy-writers (production write after an inline test module)" bash scripts/guards/legacy-writers.sh
# H1: a fixture INSIDE an inline test module is ignored.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
mod tests {
    fn fixture() { let _ = "INSERT INTO agent_tasks VALUES (1)"; }
    #[test]
    fn nested() {
        let sql = "SELECT * FROM task_wake_outbox";
        assert!(!sql.is_empty());
    }
}
RS
expect_passes "legacy-writers (test-module fixtures ignored)" bash scripts/guards/legacy-writers.sh
# H1 round 2: a brace token inside a test-module string must not swallow the
# rest of the file.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
mod tests { fn brace() { let s = "{"; let raw = r#"}"#; } }
pub fn evil_after_tests() { let _ = "UPDATE agent_tasks SET status = 'x'"; }
RS
expect_fails "legacy-writers (brace token in a test string)" bash scripts/guards/legacy-writers.sh
# H1 round 2: a statement-level multi-line #[cfg(test)] expression must skip
# only its own statement, not the remainder of the function.
cat > "$EVIL_LEGACY_RS" <<'RS'
pub fn prod() {
    #[cfg(test)]
    let _x = compute(
        1,
        2,
    );
    let _ = "INSERT INTO agent_tasks VALUES (1)";
}
RS
expect_fails "legacy-writers (statement-level test gate)" bash scripts/guards/legacy-writers.sh
# H1 round 3: a block-macro item has no trailing `;`; the scanner must stop at
# the macro's own delimiter, not at the next top-level statement boundary.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
thread_local! {
    static X: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
pub fn evil_after_macro() { let _ = "INSERT INTO agent_tasks VALUES (1)"; }
RS
expect_fails "legacy-writers (block-macro test gate)" bash scripts/guards/legacy-writers.sh
# H1 round 3: `#[cfg(test)] if/else` fixtures are ignored; production after is
# still emitted.
cat > "$EVIL_LEGACY_RS" <<'RS'
pub fn prod() {
    #[cfg(test)]
    if c { let _ = "INSERT INTO agent_tasks VALUES (1)"; } else { let _ = "SELECT * FROM task_wake_outbox"; }
    let _ = 1;
}
RS
expect_passes "legacy-writers (if/else fixtures ignored)" bash scripts/guards/legacy-writers.sh
cat > "$EVIL_LEGACY_RS" <<'RS'
pub fn prod() {
    #[cfg(test)]
    if c { let _ = 1; } else { let _ = 2; }
    let _ = "UPDATE agent_tasks SET status = 'x'";
}
RS
expect_fails "legacy-writers (production after if/else test gate)" bash scripts/guards/legacy-writers.sh
# H1 round 4: a test-gated `macro_rules!` body must not swallow the file.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
macro_rules! helper { () => { let _ = "INSERT INTO agent_tasks"; }; }
pub fn evil_after_macro_rules() { let _ = "UPDATE agent_tasks SET status = 'x'"; }
RS
expect_fails "legacy-writers (macro_rules test gate)" bash scripts/guards/legacy-writers.sh
# H1 round 4: a qualified-path macro item is an item too.
cat > "$EVIL_LEGACY_RS" <<'RS'
#[cfg(test)]
foo::bar! { fixture }
pub fn evil_after_qualified_macro() { let _ = "DELETE FROM task_wake_outbox"; }
RS
expect_fails "legacy-writers (qualified-path macro test gate)" bash scripts/guards/legacy-writers.sh
# The block-macro scope bug hid real production lines in dynamic_adapter.rs.
if PYTHONDONTWRITEBYTECODE=1 python3 scripts/guards/production_scope.py crates/runtime/src/driver/dynamic_adapter.rs \
    | grep -q "fn own_source_response" \
  && PYTHONDONTWRITEBYTECODE=1 python3 scripts/guards/production_scope.py crates/runtime/src/driver/dynamic_adapter.rs \
    | grep -q "fn run_describe_job"; then
  echo "✅ SELF-TEST: scanner emits dynamic_adapter.rs production lines"
else
  echo "❌ SELF-TEST: scanner still hides dynamic_adapter.rs production lines"
  SELFTEST_FAIL=1
fi
# L5: quoted identifiers and multi-line SQL are normalized.
printf 'pub fn evil() { let _ = "INSERT INTO \\"agent_tasks\\" VALUES (1)"; }\n' > "$EVIL_LEGACY_RS"
expect_fails "legacy-writers (quoted retired table)" bash scripts/guards/legacy-writers.sh
cat > "$EVIL_LEGACY_RS" <<'RS'
pub fn evil() {
    let sql = "SELECT *
FROM
agent_tasks WHERE id = 'x'";
    let _ = sql;
}
RS
expect_fails "legacy-writers (multi-line retired table read)" bash scripts/guards/legacy-writers.sh
rm -f "$EVIL_LEGACY_RS"
expect_passes "legacy-writers" bash scripts/guards/legacy-writers.sh

if [ "$SELFTEST_FAIL" -ne 0 ]; then
  echo "❌ Guard self-test FAILED — a guard is broken and silently green"
  exit 1
fi
echo "✅ All guard self-tests passed"
