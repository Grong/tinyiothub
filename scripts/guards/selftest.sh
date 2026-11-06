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
AUTONOMOUS_FACTORY_BAK=$(mktemp)
cp apps/cloud/src/domains/agent/host/autonomous_factory.rs "$AUTONOMOUS_FACTORY_BAK"
sed -i.bak '1a\
pub fn guard_selftest_ungated() { let _ = "UPDATE device_jobs SET state = 1"; }
' apps/cloud/src/domains/agent/host/autonomous_factory.rs && rm -f apps/cloud/src/domains/agent/host/autonomous_factory.rs.bak
expect_fails "legacy-writers (autonomous_factory production Job write)" bash scripts/guards/legacy-writers.sh
cp "$AUTONOMOUS_FACTORY_BAK" apps/cloud/src/domains/agent/host/autonomous_factory.rs
rm -f "$AUTONOMOUS_FACTORY_BAK"
AUTONOMOUS_FACTORY_BAK=""
expect_passes "legacy-writers" bash scripts/guards/legacy-writers.sh

if [ "$SELFTEST_FAIL" -ne 0 ]; then
  echo "❌ Guard self-test FAILED — a guard is broken and silently green"
  exit 1
fi
echo "✅ All guard self-tests passed"
