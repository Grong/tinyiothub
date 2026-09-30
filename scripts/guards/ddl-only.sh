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
# 白名单（2026-09-30，CI 政策裁定）：重建表自拷贝 `INSERT INTO <x>_new SELECT
# …FROM <x>` 不算违规——SQLite 改 CHECK 约束的唯一合法路径就是建新表+拷
# 数据+换名（20260914000001 先例），它是确定性 DDL 变体，不是环境/数据相关
# 的真实 DML。裸 INSERT（非 _new 目标）依然一律拒绝。
# 实现注：用循环而非 xargs -I{}——macOS xargs -I 的替换命令行有 255 字节
# 上限，过滤链一长就报 "cannot be assembled, too long" 并静默空结果（2026-09-30 实测）。
OFFENDERS=""
for f in $NON_BASELINE; do
  if grep -vE "^\s*--" "$f" | grep -vE "\bINSERT[[:space:]]+INTO[[:space:]]+[A-Za-z0-9_]+_new\b" | grep -qiE "\b(INSERT|REPLACE|UPDATE|DELETE)\b"; then
    OFFENDERS="${OFFENDERS:+$OFFENDERS
}$f"
  fi
done
if [ -n "$OFFENDERS" ]; then
  echo "$OFFENDERS"
  echo "❌ migrations must be DDL-only (seeds go to seed.rs; INSERT/UPDATE/DELETE/REPLACE incl. INSERT OR IGNORE are forbidden)"
  exit 1
fi
echo "✅ Migration DDL-only guard passed"
