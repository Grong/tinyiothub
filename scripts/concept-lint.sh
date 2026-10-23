#!/usr/bin/env bash
# concept-lint.sh — 概念模型 lint（E3，warn-only）
#
# 异名清单同步来源：docs/ai-concept-model.md 词汇表右列（Task 1 落地）——
# 词汇表右列变更时同步下方 scan 调用清单。
#
# 扫描范围（Q4 裁定）：web/src 文案 + apps/cloud/src/domains/agent 注释。
# 实现收窄（误报控制，详见 .superpowers/sdd/2026-10-12-ai-consolidation-phase1-2/task-18-report.md）：
#   - web/src/**/*.ts（排除 *.test.ts，测试断言文案不扫）：中文异名直接命中；
#     英文异名只在「含非 ASCII 的行」或「双引号字符串内」命中——代码标识符里的
#     heartbeat/tick 是合法词，不扫。
#   - apps/cloud/src/domains/agent/**/*.rs：中文异名直接命中；英文异名只在
#     注释行（含 //）或含非 ASCII 的行命中。
#   - tick 豁免：同行含「调查」不命中（调查 run 语境，需求书点名豁免）。
# 不扫的右列异名（误报率过高，人工评审兜底）：verdict/outcome/status/trace/
#   proposal/disposition（英文原形即代码标识符）、待审批（Disposition 合法取值）、
#   UserDirective／指令触发的 run（概念映射描述语）。
#
# 当前 baseline（2026-10-13 全量）：68 处命中（web 33 / agent 35）。
# 首版 warn-only：exit 恒 0；baseline 清零或逐个豁免后转 fail（T19 门禁裁定）。
#
# 依赖：仅 grep（\b 词边界在 macOS BSD grep 与 GNU grep 均已验证）。
set -u
cd "$(git rev-parse --show-toplevel)"

GRAND=0
WEBC=0
RSC=0

# scan <web|rs> <异名> <规范概念> <pattern> <i|-> <direct|ctx> [exclude-pattern]
scan() {
  local scope="$1" alias="$2" concept="$3" pattern="$4" icase="$5" mode="$6"
  local exclude="${7:-}"
  local dir inc exc iflag raw filtered
  if [ "$scope" = web ]; then
    dir="web/src"; inc="--include=*.ts"; exc="--exclude=*.test.ts"
  else
    dir="apps/cloud/src/domains/agent"; inc="--include=*.rs"; exc=""
  fi
  iflag=""; [ "$icase" = i ] && iflag="-i"
  # shellcheck disable=SC2086
  raw=$(grep -rInE $iflag $inc $exc -- "$pattern" "$dir" 2>/dev/null || true)
  [ -z "$raw" ] && return 0
  if [ "$scope" = rs ]; then
    # 测试断言文案不扫：命中行号 ≥ 文件首个 #[cfg(test)] 即丢弃
    #（行内 test 模块约定；首个标记后偶有 prod 代码的误伤可接受，warn-only）
    local line file rest lineno cutoff kept=""
    while IFS= read -r line; do
      file="${line%%:*}"; rest="${line#*:}"; lineno="${rest%%:*}"
      cutoff=$(grep -n '#\[cfg(test)\]' "$file" 2>/dev/null | head -1 | cut -d: -f1)
      if [ -n "$cutoff" ] && [ "$lineno" -ge "$cutoff" ]; then continue; fi
      kept="${kept}${line}
"
    done <<< "$raw"
    raw="${kept%$
}"
    [ -z "$raw" ] && return 0
  fi
  if [ "$mode" = direct ]; then
    filtered="$raw"
  elif [ "$scope" = web ]; then
    # 文案语境：含非 ASCII（中文行）或双引号字符串内；
    # 排除 import/export 模块说明符行（"../xx.js" 路径不是文案，含多行 import 的 from 行）
    filtered=$(printf '%s\n' "$raw" | tr -d '\t' | grep -E $iflag "[^ -~]|\"[^\"]*($pattern)" \
      | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(import[[:space:]].*|export[[:space:]].*|}[[:space:]]*)?from[[:space:]]*["'"'"']' || true)
  else
    # 注释语境：行内含 // 或含非 ASCII（中文注释/中文 format! 文案）
    filtered=$(printf '%s\n' "$raw" | tr -d '\t' | grep -E '//|[^ -~]' || true)
  fi
  if [ -n "$exclude" ] && [ -n "$filtered" ]; then
    filtered=$(printf '%s\n' "$filtered" | grep -vE "$exclude" || true)
  fi
  [ -z "$filtered" ] && return 0
  local n file rest lineno content
  n=$(printf '%s\n' "$filtered" | wc -l | tr -d ' ')
  GRAND=$((GRAND + n))
  if [ "$scope" = web ]; then WEBC=$((WEBC + n)); else RSC=$((RSC + n)); fi
  printf '%s\n' "$filtered" | while IFS= read -r hit; do
    file="${hit%%:*}"; rest="${hit#*:}"; lineno="${rest%%:*}"; content="${rest#*:}"
    printf '[concept-lint] WARNING 异名「%s」→ 规范概念 %s\n  %s:%s: %s\n' \
      "$alias" "$concept" "$file" "$lineno" "$content"
    if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
      # GHA workflow command：message 需转义 % \r \n
      local msg
      msg=$(printf '异名「%s」→ 规范概念 %s: %s' "$alias" "$concept" "$content" \
        | sed -e 's/%/%25/g' -e 's/\r/%0D/g' | tr -d '\n')
      printf '::warning file=%s,line=%s::concept-lint %s\n' "$file" "$lineno" "$msg"
    fi
  done
}

# ---- 直接异名（中文/全短语，两scope 均直接命中）----
scan web "执行历史"   "Trace"       "执行历史"          - direct
scan rs  "执行历史"   "Trace"       "执行历史"          - direct
scan web "证据"       "Trace"       "证据"              - direct
scan rs  "证据"       "Trace"       "证据"              - direct
scan web "调查叙事"   "Narrative"   "调查叙事"          - direct
scan rs  "调查叙事"   "Narrative"   "调查叙事"          - direct
scan web "调查 run"   "Run"         "调查 ?[Rr]un"      - direct
scan rs  "调查 run"   "Run"         "调查 ?[Rr]un"      - direct
scan web "心跳 tick"  "Run"         "心跳 ?tick"        i direct
scan rs  "心跳 tick"  "Run"         "心跳 ?tick"        i direct
scan web "judgment 状态机" "Disposition" "judgment 状态机" i direct
scan rs  "judgment 状态机" "Disposition" "judgment 状态机" i direct

# ---- 歧义异名（英文词，需文案/注释语境过滤）----
scan web "tick"           "Run"        "\btick\b"        i ctx "调查"
scan rs  "tick"           "Run"        "\btick\b"        i ctx "调查"
scan web "evidence"       "Trace"      "\bevidence\b"    i ctx
scan rs  "evidence"       "Trace"      "\bevidence\b"    i ctx
scan web "pending action" "Proposal"   "pending action"  i ctx
scan rs  "pending action" "Proposal"   "pending action"  i ctx
scan web "trust_level"    "Trust Mode" "trust_level"     - ctx
scan rs  "trust_level"    "Trust Mode" "trust_level"     - ctx

echo "----------------------------------------------------------------"
echo "[concept-lint] 命中 ${GRAND} 处（web ${WEBC} / agent ${RSC}）——warn-only，不阻断合并。"
echo "[concept-lint] 清单同步来源：docs/ai-concept-model.md 词汇表右列；转 fail 前请清零或逐个豁免。"
exit 0
