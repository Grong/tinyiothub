#!/usr/bin/env bash
# crates/agent 纯度守卫（G9 源码面 + 终审 F6/对抗 F2 清单面 + 树面）。
# 1. 源码：禁 axum/sqlx/tinyiothub_storage 引用与 crate::domains 回流。
# 2. 依赖树：禁 sqlx/db（含改名包）。axum 树级不可查（zeroclaw 传递引入），
#    由 1+3 承担。
# 3. 成员清单：直名/改名/子表形态均违规（含单引号 TOML 字面量）。
# 4. 根清单：改名声明违规（成员可经 workspace = true 继承）。
set -u
cd "$(git rev-parse --show-toplevel)"

FAIL=0
if grep -rEn --include='*.rs' 'use\s+(axum|sqlx|tinyiothub_storage)|\b(axum|sqlx|tinyiothub_storage)\s*::\s*|crate\s*::\s*domains' crates/agent/src; then
  echo "❌ DEPENDENCY VIOLATION: crates/agent is the pure runtime plane — no axum/sqlx/storage/domains references"
  echo "   ✅ Web/HTTP concerns belong in apps/cloud host; persistence behind port traits"
  FAIL=1
fi
# 对抗性评审 F2：cargo tree 失败（离线/lock 漂移/网络）产生空输出——
# 管道到 grep 会静默通过。工具失败即守卫失败（fail-closed）。
TREE_OUT=$(cargo tree -p agent --prefix none --format "{p}" 2>&1)
TREE_RC=$?
if [ "$TREE_RC" -ne 0 ]; then
  echo "❌ DEPENDENCY GUARD ERROR: cargo tree failed (rc=$TREE_RC) — refusing to pass on tool failure"
  echo "$TREE_OUT" | head -5
  exit 1
fi
if echo "$TREE_OUT" | grep -E "^sqlx|^db "; then
  echo "❌ DEPENDENCY VIOLATION: crates/agent dependency tree must not contain sqlx/db (incl. renamed deps)"
  FAIL=1
fi
if sed 's/#.*$//' crates/agent/Cargo.toml | grep -nE "^\s*(axum|sqlx|tinyiothub-storage|tinyiothub_storage|db)\s*=|package\s*=\s*[\"'](axum|sqlx|tinyiothub-storage|db)[\"']|^\s*\[[a-zA-Z0-9_.-]*\.(axum|sqlx|tinyiothub-storage|tinyiothub_storage|db)\s*\]"; then
  echo "❌ DEPENDENCY VIOLATION: crates/agent/Cargo.toml must not declare axum/sqlx/storage/db (incl. renamed/sub-table)"
  FAIL=1
fi
if sed 's/#.*$//' Cargo.toml | grep -nE "^\s*[a-zA-Z0-9_-]+\s*=\s*\{[^}]*package\s*=\s*[\"'](axum|sqlx|tinyiothub-storage|db)[\"']"; then
  echo "❌ DEPENDENCY VIOLATION: workspace root must not rename axum/sqlx/storage/db (inheritable by crates/agent)"
  FAIL=1
fi
if [ "$FAIL" -ne 0 ]; then exit 1; fi
echo "✅ Agent purity guard (source + tree + manifests) passed"
