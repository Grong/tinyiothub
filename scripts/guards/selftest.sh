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
cleanup() {
  for path in "${CLEANUP[@]}"; do
    if [ -e "$path" ]; then rm -f "$path"; fi
  done
  if [ -n "$AGENT_TOML_BAK" ] && [ -f "$AGENT_TOML_BAK" ]; then
    cp "$AGENT_TOML_BAK" crates/agent/Cargo.toml
    rm -f "$AGENT_TOML_BAK"
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
expect_passes "sql-residence" bash scripts/guards/sql-residence.sh

# ── Agent purity ───────────────────────────────────────────
EVIL_AGENT=crates/agent/src/selftest_evil.rs
printf 'use axum::Router;\n' > "$EVIL_AGENT"
CLEANUP+=("$EVIL_AGENT")
expect_fails "agent-purity (source use axum)" bash scripts/guards/agent-purity.sh
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
expect_passes "agent-purity" bash scripts/guards/agent-purity.sh

if [ "$SELFTEST_FAIL" -ne 0 ]; then
  echo "❌ Guard self-test FAILED — a guard is broken and silently green"
  exit 1
fi
echo "✅ All guard self-tests passed"
