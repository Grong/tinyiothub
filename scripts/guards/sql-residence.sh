#!/usr/bin/env bash
# SQL 住所守卫（Task 13 + 终审 F1/F14）。
# sqlx::(query|QueryBuilder|raw_sql) 只允许出现在 crates/db。豁免：
# - 文件名含 test_utils / _test，或路径在 tests/ 下；
# - 内联 #[cfg(test)] 测试模块（仅当标记后 3 行内有 mod 声明才截断，
#   否则整文件扫描——截断点以下的生产 SQL 不得隐形）；
# - 行内 `guard-exempt: <reason>`（必须带理由；无理由豁免即失败，
#   有理由豁免打印备查）。
# 多行构造：任何以 `sqlx::` 结尾的行即违规线索。
# 扫描面：apps + 全部 crates（db 与 vendor 除外）。
set -u
cd "$(git rev-parse --show-toplevel)"

FAIL=0
SCAN_ROOTS="apps/cloud/src apps/edge/src crates"
for f in $(grep -rEl --include='*.rs' -E 'sqlx::(query|QueryBuilder|raw_sql)|sqlx::[[:space:]]*$' $SCAN_ROOTS 2>/dev/null | grep -v '^crates/db/' | grep -v '^vendor/'); do
  case "$f" in
    *test_utils*|*_test*|*tests/*) continue ;;
  esac
  # 豁免判定：pattern 行本身或其 3 行内（多行调用的参数区）带
  # guard-exempt 注释。先剥豁免，再查违规。
  HEAD_ONLY=0
  MARKER_LINE=$(grep -nE '#\[cfg\(test\)\]' "$f" | head -1 | cut -d: -f1 || true)
  if [ -n "$MARKER_LINE" ]; then
    # 可疑生产代码探测：首个标记之后、任何 mod 声明之前，存在列 0 起
    # 始且 3 行内无 #[cfg(test)] 属性的 fn 定义——测试标记被借来藏
    # 生产 SQL（F14），整文件扫描；否则安全截断（覆盖模块形态与
    # 逐项属性形态两种测试布局）。
    FIRST_MOD_REL=$(sed -n "$((MARKER_LINE+1)),\$p" "$f" | grep -nE '^\s*(pub(\(crate\))?\s+)?mod\s' | head -1 | cut -d: -f1 || true)
    if [ -n "$FIRST_MOD_REL" ]; then
      TAIL_END=$((MARKER_LINE + FIRST_MOD_REL - 1))
    else
      TAIL_END=$(wc -l < "$f" | tr -d ' ')
    fi
    SUSPECT=0
    TOP_FNS=$(sed -n "$((MARKER_LINE+1)),${TAIL_END}p" "$f" | grep -nE '^(pub(\(crate\))?\s+)?(async\s+)?fn\s' | cut -d: -f1 || true)
    for rel in $TOP_FNS; do
      abs=$((MARKER_LINE + rel))
      # 属性只作用于紧随其后的那一个 item：fn 正上方最近的非空行
      # 必须恰是 #[cfg(test)]，否则该 fn 是生产代码。
      prev=$(sed -n "1,$((abs-1))p" "$f" | grep -vE '^\s*$' | tail -1)
      if [ "$prev" != "#[cfg(test)]" ]; then
        SUSPECT=1
        break
      fi
    done
    [ "$SUSPECT" = "0" ] && HEAD_ONLY=1
  fi
  if [ "$HEAD_ONLY" = "1" ]; then
    SCAN=$(sed '/#\[cfg(test)\]/,$d' "$f")
  else
    SCAN=$(cat "$f")
  fi
  # 逐命中行判定豁免：命中行起 3 行内出现 guard-exempt 即豁免
  # （覆盖单行尾注释与多行调用参数区注释两种写法）。
  HITS=$(echo "$SCAN" | grep -nE 'sqlx::(query|QueryBuilder|raw_sql)|sqlx::[[:space:]]*$' | cut -d: -f1 || true)
  VIOLATIONS=""
  EXEMPTIONS=""
  for ln in $HITS; do
    if echo "$SCAN" | sed -n "${ln},$((ln+3))p" | grep -q 'guard-exempt'; then
      EXEMPTIONS="$EXEMPTIONS $ln"
    else
      VIOLATIONS="$VIOLATIONS $ln"
    fi
  done
  for ln in $EXEMPTIONS; do
    # 无理由豁免即失败；有理由打印备查（F14 guard-exempt 监管）。
    if echo "$SCAN" | sed -n "${ln},$((ln+3))p" | grep -E 'guard-exempt' | grep -vqE 'guard-exempt:\s*\S'; then
      echo "$SCAN" | sed -n "${ln},$((ln+3))p" | grep -E 'guard-exempt' | grep -vE 'guard-exempt:\s*\S'
      echo "❌ guard-exempt without reason in $f (format: guard-exempt: <reason>)"
      FAIL=1
    else
      echo "   ℹ️  exemption: $f:$ln"
    fi
  done
  for ln in $VIOLATIONS; do
    echo "$SCAN" | sed -n "${ln}p"
  done
  if [ -n "$VIOLATIONS" ]; then
    echo "❌ raw SQL outside crates/db: $f"
    echo "   ✅ Move the query into a crates/db domain file and expose a Db delegate"
    FAIL=1
  fi
done
if [ "$FAIL" -ne 0 ]; then exit 1; fi
echo "✅ DB SQL residence guard passed"
