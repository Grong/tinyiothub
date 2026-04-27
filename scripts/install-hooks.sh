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

echo "Installing pre-commit hook..."

mkdir -p "$HOOK_DIR"

cat > "$PRECOMMIT_HOOK" << 'HOOK_EOF'
#!/usr/bin/env bash
# =============================================================================
# TinyIoTHub Pre-commit Hook
# Checks formatting, clippy, and architecture violations
# =============================================================================

set -e

echo "Running pre-commit checks..."

CHANGED_FILES=$(git diff --cached --name-only)
NEW_FILES=$(git diff --cached --name-only --diff-filter=A)

# =============================================================================
# Check 1: Rust formatting (only staged .rs files)
# =============================================================================
STAGED_RS=$(echo "$CHANGED_FILES" | grep '\.rs$' || true)
if [ -n "$STAGED_RS" ]; then
  echo "  Checking Rust formatting..."
  if ! cargo fmt --check -- $STAGED_RS 2>/dev/null; then
    echo "Formatting issues found. Run: cargo fmt"
    exit 1
  fi
fi

# =============================================================================
# Check 2: No scatter-shot utils/helpers in cloud/src/
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    cloud/src/utils*.rs|cloud/src/helpers*.rs|cloud/src/common*.rs)
      echo "Cannot create scatter-shot utils/helpers: $file"
      echo "   Use shared/ or specific domain modules"
      exit 1
      ;;
  esac
done

# =============================================================================
# Check 3: No direct fetch in frontend components
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    web/src/ui/*)
      if grep -qE "fetch\(|axios\.|\.get\(|\.post\(" "$file" 2>/dev/null; then
        echo "Direct API call in component: $file"
        echo "   Must use web/src/api/ layer"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# Check 4: No direct SQL in handlers
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    cloud/src/modules/*/handler*.rs)
      if grep -qE "sqlx::query|pool\.(query|execute)" "$file" 2>/dev/null; then
        echo "Direct SQL in handler: $file"
        echo "   Use repository pattern"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# Check 5: No manual JSON responses (must use ApiResponseBuilder)
# =============================================================================
for file in $CHANGED_FILES; do
  case "$file" in
    cloud/src/modules/*/handler*.rs)
      if grep -qE "Json\s*\(\s*serde_json::to_value" "$file" 2>/dev/null; then
        echo "Manual JSON response in handler: $file"
        echo "   Use ApiResponseBuilder::success(data)"
        exit 1
      fi
      ;;
  esac
done

# =============================================================================
# Check 6: New module files should have tests
# =============================================================================
for file in $NEW_FILES; do
  case "$file" in
    cloud/src/modules/**/*.rs)
      if ! grep -qE "#\[cfg\(test\)\]|#\[test\]" "$file" 2>/dev/null; then
        echo "  Warning: New module file has no tests: $file"
      fi
      ;;
  esac
done

echo "Pre-commit checks passed"
HOOK_EOF

chmod +x "$PRECOMMIT_HOOK"

echo "Pre-commit hook installed: $PRECOMMIT_HOOK"
echo ""
echo "Checks on every commit:"
echo "  1. Rust formatting (cargo fmt)"
echo "  2. No scatter-shot utils/helpers"
echo "  3. No direct fetch in frontend components"
echo "  4. No direct SQL in handlers"
echo "  5. API responses use ApiResponseBuilder"
echo "  6. New module files should have tests"
echo ""
echo "Skip with: git commit --no-verify"
