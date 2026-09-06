#!/usr/bin/env bash
# DDL-only 迁移守卫（Task 13 + 终审 F5/F3）。
# baseline 豁免；其余迁移不得携带 DML——只剥"整行注释"（行首 --），
# 代码行全量扫描（行内 -- 可能藏在字符串字面量里，剥离它会把同行后面的
# DML 一并抹掉）。大小写不敏感、词边界。
#
# Grandfathered：v0.5.0.0 发布窗口（CI 因 ci.yml YAML 语法错误瘫痪期间）
# 合入的 4 个存量数据迁移。它们已随 release 应用到真实库，sqlx 校验
# checksum，改写会炸存量部署；下次迁移基线化时并入 baseline 并从此清单
# 移除（见 TODOS.md）。新迁移一律 DDL-only，不得加入此清单。
GRANDFATHERED="20260825000001_rename_device_to_thing.sql
20260826000001_thing_contract_data.sql
20260828000001_policy_action_rename.sql
20260831000001_tool_override_rename.sql"
set -u
cd "$(git rev-parse --show-toplevel)"

NON_BASELINE=$(ls crates/db/migrations/*.sql | grep -v baseline || true)
for g in $GRANDFATHERED; do
  NON_BASELINE=$(echo "$NON_BASELINE" | grep -v "/$g\$" || true)
done
NON_BASELINE=$(echo "$NON_BASELINE" | grep -v '^\s*$' || true)
if [ -z "$NON_BASELINE" ]; then
  echo "✅ Migration DDL-only guard passed (no non-baseline migrations)"
  exit 0
fi
OFFENDERS=$(echo "$NON_BASELINE" | xargs -I{} sh -c 'grep -vE "^\s*--" "{}" | grep -qiE "\b(INSERT|REPLACE|UPDATE|DELETE)\b" && echo "{}"' || true)
if [ -n "$OFFENDERS" ]; then
  echo "$OFFENDERS"
  echo "❌ migrations must be DDL-only (seeds go to seed.rs; INSERT/UPDATE/DELETE/REPLACE incl. INSERT OR IGNORE are forbidden)"
  exit 1
fi
echo "✅ Migration DDL-only guard passed"
