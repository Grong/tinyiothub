#!/usr/bin/env bash
# DDL-only 迁移守卫（Task 13 + 终审 F5/F3）。
# baseline 豁免；其余迁移不得携带 DML——只剥"整行注释"（行首 --），
# 代码行全量扫描（行内 -- 可能藏在字符串字面量里，剥离它会把同行后面的
# DML 一并抹掉）。大小写不敏感、词边界。
set -u
cd "$(git rev-parse --show-toplevel)"

NON_BASELINE=$(ls crates/db/migrations/*.sql | grep -v baseline || true)
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
