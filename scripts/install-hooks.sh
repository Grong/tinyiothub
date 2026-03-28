#!/usr/bin/env bash
# =============================================================================
# 安装架构检查 pre-commit hook
# 运行方式: bash scripts/install-hooks.sh
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOK_DIR="$REPO_ROOT/.git/hooks"
PRECOMMIT_HOOK="$HOOK_DIR/pre-commit"

echo "🔧 安装架构检查 pre-commit hook..."

# 创建 hooks 目录（如果不存在）
mkdir -p "$HOOK_DIR"

# 写入 hook 脚本
cat > "$PRECOMMIT_HOOK" << 'HOOK_EOF'
#!/usr/bin/env bash
# =============================================================================
# Architecture Pre-commit Hook
# 自动检测架构违规，防止违规代码进入版本控制
# =============================================================================

set -e

echo "🔍 运行架构检查..."

# 获取暂存的文件
CHANGED_FILES=$(git diff --cached --name-only)
NEW_FILES=$(git diff --cached --name-only --diff-filter=A)

# =============================================================================
# 检查 1：禁止在 api/src/ 创建散弹式 utils/helpers
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    api/src/utils*.rs|api/src/helpers*.rs|api/src/common*.rs)
      echo "❌ 禁止在 api/src/ 创建散弹式文件: $file"
      echo "   ✅ 使用 shared/ 或具体 domain 模块"
      exit 1
      ;;
  esac
done

# =============================================================================
# 检查 2：前端禁止在组件里直接 fetch
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    web/app/*|web/components/*)
      if grep -qE "fetch\(|axios\.|\.get\(|\.post\(" "$file" 2>/dev/null; then
        echo "❌ 前端组件禁止直接调用 API: $file"
        echo "   ✅ 必须使用 service 层"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# 检查 3：后端禁止在 handlers 里直接 SQL
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    api/src/api/*.rs)
      if grep -qE "conn\.(query_row|execute|query)|pool\.(query|execute)" "$file" 2>/dev/null; then
        echo "❌ API handler 里禁止直接执行 SQL: $file"
        echo "   ✅ 使用 repository pattern"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# 检查 4：禁止直接 Json(serde_json::to_value(...))
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    api/src/api/*.rs)
      if grep -qE "Json\s*\(\s*serde_json::to_value" "$file" 2>/dev/null; then
        echo "❌ 禁止直接使用 Json(serde_json::to_value(...)): $file"
        echo "   ✅ 使用 ApiResponseBuilder::success(data)"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# 检查 5：新文件必须有测试（domain 层）
# =============================================================================
for file in $NEW_FILES; do
  case "$file" in
    api/src/domain/**/*.rs)
      if ! grep -qE "#\[cfg\(test\)\]|#\[test\]" "$file" 2>/dev/null; then
        echo "⚠️  警告：新增 domain 文件建议添加测试: $file"
        # 不强制失败，只警告
      fi
      ;;
  esac
done

# =============================================================================
# 检查 6：commit message 格式
# =============================================================================
COMMIT_MSG_FILE=$1
if [ -n "$COMMIT_MSG_FILE" ] && [ -f "$COMMIT_MSG_FILE" ]; then
  COMMIT_MSG=$(cat "$COMMIT_MSG_FILE")
  # 跳过 merge commits
  if echo "$COMMIT_MSG" | grep -qE "^Merge"; then
    exit 0
  fi
  # 检查格式
  if ! echo "$COMMIT_MSG" | head -1 | grep -qE "^(feat|fix|test|chore|docs|refactor|style|perf|ci|build)\([a-z0-9_-]+\):"; then
    echo "❌ Commit message 格式不规范:"
    echo "   $COMMIT_MSG"
    echo "   ✅ 格式: <type>(<scope>): <description>"
    echo "   ✅ 例如: feat(device): add temperature monitoring"
    exit 1
  fi
fi

echo "✅ 架构检查通过"
HOOK_EOF

# 设置执行权限
chmod +x "$PRECOMMIT_HOOK"

echo "✅ Pre-commit hook 已安装: $PRECOMMIT_HOOK"
echo ""
echo "📋 每次 commit 将自动检查："
echo "   1. 禁止散弹式 utils/helpers"
echo "   2. 前端组件必须走 service 层"
echo "   3. 后端 handler 必须走 repository"
echo "   4. API 响应必须用 ApiResponseBuilder"
echo "   5. commit message 格式"
echo ""
echo "💡 要跳过检查？用 git commit --no-verify"
