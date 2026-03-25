#!/usr/bin/env bash
# =============================================================================
# Architecture Check Hook
# 在 git commit 前自动运行，检查违规模式
# =============================================================================

set -e

echo "🔍 运行架构检查..."

# 获取暂存的文件（新增+修改）
CHANGED_FILES=$(git diff --cached --name-only)
NEW_FILES=$(git diff --cached --name-only --diff-filter=A)

# =============================================================================
# 检查 1：禁止在 api/src/ 创建散弹式 utils/helpers
# =============================================================================
echo "  → 检查散弹式 utils/helpers..."
for file in $CHANGED_FILES; do
  case "$file" in
    api/src/utils*.rs)
      echo "    ❌ 禁止在 api/src/ 创建散弹式文件: $file"
      echo "    ✅ 使用 shared/utils/ 或 domain 特定模块"
      exit 1
      ;;
    api/src/helpers*.rs)
      echo "    ❌ 禁止在 api/src/ 创建散弹式文件: $file"
      exit 1
      ;;
    api/src/common*.rs)
      echo "    ❌ 禁止在 api/src/ 创建 common*.rs: $file"
      echo "    ✅ 使用 shared/ 或具体 domain 模块"
      exit 1
      ;;
  esac
done

# =============================================================================
# 检查 2：前端禁止直接 fetch（组件必须走 service layer）
# =============================================================================
echo "  → 检查前端 API 调用规范..."
COMPONENT_FETCH=$(git diff --cached --name-only | grep -E "^web/(app|components|hooks)/.*\.(ts|tsx)$" || true)
if [ -n "$COMPONENT_FETCH" ]; then
  # 检查这些文件里是否有直接 fetch
  for file in $COMPONENT_FETCH; do
    if [ -f "$file" ]; then
      # 允许在 service/ 和 lib/ 里用 fetch，但不允许在组件/hooks里直接用
      case "$file" in
        web/service/*|web/lib/*|web/hooks/use-*.ts|web/hooks/use-*.tsx)
          # 这些地方允许
          ;;
        *)
          # 检查是否有直接 fetch 调用
          if grep -qE "fetch\(|axios\.|useQuery\s*\(" "$file" 2>/dev/null; then
            echo "    ❌ 前端组件禁止直接调用 fetch/axios/useQuery: $file"
            echo "    ✅ 必须使用 service 层 + hooks 层"
            exit 1
          fi
          ;;
      esac
    fi
  done
fi

# =============================================================================
# 检查 3：后端禁止在 handlers 里直接写 SQL
# =============================================================================
echo "  → 检查后端 SQL 规范..."
HANDLER_FILES=$(git diff --cached --name-only | grep "^api/src/api/" || true)
if [ -n "$HANDLER_FILES" ]; then
  for file in $HANDLER_FILES; do
    if [ -f "$file" ]; then
      # 检查是否有直接 SQL 调用（conn.query_row, pool.execute 等）
      if grep -qE "conn\.(query_row|execute|query)|pool\.(query|execute)|\.execute\s*\(" "$file" 2>/dev/null; then
        echo "    ❌ API handler 里禁止直接执行 SQL: $file"
        echo "    ✅ 使用 repository pattern: infrastructure/persistence/repositories/"
        exit 1
      fi
    fi
  done
fi

# =============================================================================
# 检查 4：禁止绕过 ApiResponseBuilder
# =============================================================================
echo "  → 检查 API 响应格式..."
HANDLER_FILES=$(git diff --cached --name-only | grep "^api/src/api/" || true)
if [ -n "$HANDLER_FILES" ]; then
  for file in $HANDLER_FILES; do
    if [ -f "$file" ]; then
      # 检测直接 Json(serde_json::to_value(...)) 模式
      if grep -qE "Json\s*\(\s*serde_json::to_value" "$file" 2>/dev/null; then
        echo "    ❌ 禁止直接使用 Json(serde_json::to_value(...)): $file"
        echo "    ✅ 使用 ApiResponseBuilder::success(data)"
        exit 1
      fi
      # 检测直接返回数字 code（而不是用 builder）
      if grep -qE "\"code\"\s*:\s*[0-9]" "$file" 2>/dev/null; then
        echo "    ❌ 禁止硬编码 JSON 响应格式: $file"
        echo "    ✅ 使用 ApiResponseBuilder"
        exit 1
      fi
    fi
  done
fi

# =============================================================================
# 检查 5：新增 domain 模块必须有对应测试
# =============================================================================
echo "  → 检查测试覆盖..."
NEW_DOMAIN_FILES=$(git diff --cached --name-only --diff-filter=A | grep -E "^api/src/domain/.*\.rs$" || true)
if [ -n "$NEW_DOMAIN_FILES" ]; then
  for file in $NEW_DOMAIN_FILES; do
    dirname=$(dirname "$file")
    # 检查是否有对应的 test 文件或 #[test] 模块
    if [ -f "$file" ]; then
      # 简单检查：如果文件有 #[cfg(test)] 或 #[test] 就认为有测试
      if ! grep -qE "#\[cfg\(test\)\]|#\[test\]" "$file" 2>/dev/null; then
        # 再检查同目录下是否有 *_tests.rs 文件
        base=$(basename "$file" .rs)
        parent=$(dirname "$file")
        if [ ! -f "$parent/${base}_tests.rs" ] && [ ! -f "$parent/tests.rs" ]; then
          echo "    ⚠️  新增 domain 文件建议添加测试: $file"
          # 不强制失败，只警告
        fi
      fi
    fi
  done
fi

# =============================================================================
# 检查 6：commit message 格式
# =============================================================================
echo "  → 检查 commit message 格式..."
COMMIT_MSG_FILE=$1
if [ -n "$COMMIT_MSG_FILE" ] && [ -f "$COMMIT_MSG_FILE" ]; then
  COMMIT_MSG=$(cat "$COMMIT_MSG_FILE")
  # 检查是否符合 <type>(<scope>): <desc> 格式
  if ! echo "$COMMIT_MSG" | grep -qE "^(feat|fix|test|chore|docs|refactor|style|perf|ci|build)\([a-z0-9_-]+\):"; then
    echo "    ⚠️  Commit message 格式不规范"
    echo "    ✅ 格式: <type>(<scope>): <description>"
    echo "    ✅ 示例: feat(device): add temperature monitoring"
    echo "    ✅ type: feat|fix|test|chore|docs|refactor|style|perf|ci|build"
    # commit message 不强制失败，因为可能是 merge commit
  fi
fi

echo ""
echo "✅ 架构检查通过"
