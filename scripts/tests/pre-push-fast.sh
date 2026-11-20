#!/usr/bin/env bash
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
HOOK_REL=".githooks/pre-push.local"
TEST_LANE_REL="scripts/test-lane.sh"
TMP=$(mktemp -d)
trap 'rc=$?; if [ "$rc" -ne 0 ]; then printf "FAIL at line %s (exit %s)\n" "$LINENO" "$rc" >&2; [ ! -f "$LOG" ] || tail -n 40 "$LOG" >&2; fi; rm -rf "$TMP"' EXIT
mkdir -p "$TMP/bin"
LOG="$TMP/commands.log"
if [ "${PRE_PUSH_TEST_DELEGATE:-}" = 1 ]; then
  printf '%s\n' 'shell regression delegated' >> "$COMMAND_LOG"
  exit 0
fi

cat > "$TMP/bin/cargo" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = --version ]; then
  printf '%s\n' 'cargo-test-version'
  exit 0
fi
if [ "$1" = clippy ] && [ "${FAIL_CLIPPY:-}" = 1 ]; then
  exit 1
fi
printf 'cargo %s\n' "$*" >> "$COMMAND_LOG"
STUB
cat > "$TMP/bin/npx" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = --version ]; then
  printf '%s\n' 'npx-test-version'
  exit 0
fi
printf 'npx %s cwd=%s\n' "$*" "$PWD" >> "$COMMAND_LOG"
STUB
cat > "$TMP/bin/node" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = --version ]; then
  printf '%s\n' 'node-test-version'
  exit 0
fi
printf 'node %s cwd=%s\n' "$*" "$PWD" >> "$COMMAND_LOG"
STUB
chmod +x "$TMP/bin/cargo" "$TMP/bin/npx" "$TMP/bin/node"

export PATH="$TMP/bin:$PATH"
export COMMAND_LOG="$LOG"
export NODE="$TMP/bin/node"
export NPX="$TMP/bin/npx"
: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" \
  crates/db/src/suppression.rs \
  apps/cloud/tests/harness_core_e2e_tests.rs \
  web/src/ui/app.test.ts

grep -F 'cargo test --manifest-path crates/db/Cargo.toml --lib suppression' "$LOG" >/dev/null
grep -F 'cargo test --manifest-path apps/cloud/Cargo.toml --test harness_core_e2e_tests' "$LOG" >/dev/null
grep -F "npx vitest run src/ui/app.test.ts cwd=$ROOT/web" "$LOG" >/dev/null
if grep -E 'cargo (test|clippy) --workspace|npx vitest run$' "$LOG" >/dev/null; then
  echo 'FAIL: selected files still trigger a full test lane' >&2
  exit 1
fi

many=()
for ((i = 0; i < 101; i++)); do
  many+=("crates/db/src/suppression.rs")
done
: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" "${many[@]}"
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL"
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

set +e
bash "$ROOT/$TEST_LANE_REL" README.md >/dev/null 2>&1
invalid_status=$?
set -e
[ "$invalid_status" -ne 0 ]

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" crates/db/Cargo.toml
grep -F 'cargo test --manifest-path crates/db/Cargo.toml' "$LOG" >/dev/null

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" apps/cloud/tests/deleted.rs
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" \
  web/e2e/assistant.spec.ts \
  web/src/ui/app.test.ts \
  apps/edge/tests/e2e/green_path_test.rs \
  apps/cli/src/commands/config.rs \
  scripts/tests/pre-push-fast.test.js
grep -F "npx playwright test --reporter=list e2e/assistant.spec.ts cwd=$ROOT/web" "$LOG" >/dev/null
grep -F "npx vitest run src/ui/app.test.ts cwd=$ROOT/web" "$LOG" >/dev/null
grep -F 'cargo test --manifest-path apps/edge/Cargo.toml --test e2e_tests' "$LOG" >/dev/null
grep -F 'cargo test --manifest-path apps/cli/Cargo.toml --bins commands::config' "$LOG" >/dev/null
grep -F "node --test scripts/tests/pre-push-fast.test.js cwd=$ROOT" "$LOG" >/dev/null

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" Cargo.lock
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

: > "$LOG"
bash "$ROOT/$TEST_LANE_REL" web/src/api/client.ts
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

cat > "$TMP/bin/git" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = diff ] && { [ "$2" = --quiet ] || [ "$2" = --cached ]; }; then
  [ "${DIRTY:-}" != 1 ]
  exit
fi
if [ "$1" = ls-files ] && [[ "$*" = *--others* ]]; then
  [ "${DIRTY:-}" != 1 ] || printf '%s\n' dirty-file.rs
  exit 0
fi
if [ "$1" = diff ] && [ "$2" = --name-only ]; then
  [ "${DIFF_FAIL:-}" != 1 ] || exit 128
  if [ "${3:-}" = --diff-filter=ACMR ] && [[ "${FAKE_DIFF:-}" = *deleted.rs ]]; then
    exit 0
  fi
  if [ -n "${FAKE_DIFF:-}" ]; then
    printf '%s\n' "$FAKE_DIFF"
  else
    printf '%s\n' 'crates/db/src/suppression.rs'
  fi
  exit 0
fi
if [ "$1" = rev-parse ]; then
  if [ "$2" = HEAD ]; then
    printf '%s\n' 1111111
    exit 0
  fi
  case "$*" in
    *1111111^{commit}*) printf '%s\n' 1111111 ;;
    *2222222^{commit}*) printf '%s\n' 2222222 ;;
    *HEAD^{tree}*) printf '%s\n' 'unexpected HEAD lookup' >&2; exit 64 ;;
    *1111111^{tree}*) printf '%s\n' tree-one ;;
    *2222222^{tree}*) printf '%s\n' tree-two ;;
    *origin/e4-p2-declaration-closeout*) printf '%s\n' test-base ;;
    *upstream/e4-p2-declaration-closeout*) printf '%s\n' upstream-base ;;
  esac
  exit 0
fi
exec /usr/bin/git "$@"
STUB
chmod +x "$TMP/bin/git"
cat > "$TMP/bin/gstack-wtree" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "${WTREE_VALUE:-test-working-tree}"
STUB
chmod +x "$TMP/bin/gstack-wtree"

run_hook() {
  printf '%s\n' "${HOOK_INPUT:-refs/heads/e4 1111111 refs/heads/e4-p2-declaration-closeout 2222222}" | \
    PRE_PUSH_CACHE_DIR="$TMP/${CACHE_NAME:-cache}" FAKE_DIFF="${FAKE_DIFF:-}" GSTACK_WTREE="${GSTACK_WTREE:-$TMP/bin/gstack-wtree}" bash "$ROOT/$HOOK_REL" "${HOOK_REMOTE:-origin}"
}

: > "$LOG"
run_hook
grep -F 'cargo fmt --all -- --check' "$LOG" >/dev/null
grep -F 'cargo clippy --manifest-path crates/db/Cargo.toml --all-targets -- -D warnings' "$LOG" >/dev/null
grep -F 'cargo test --manifest-path crates/db/Cargo.toml --lib suppression' "$LOG" >/dev/null
if grep -E 'cargo (test|clippy) --workspace|cargo test --all' "$LOG" >/dev/null; then
  echo 'FAIL: pre-push still runs full Rust checks' >&2
  exit 1
fi

cp "$LOG" "$TMP/first-run.log"
run_hook > "$TMP/reuse.log"
cmp "$TMP/first-run.log" "$LOG" >/dev/null
grep -F 'reusing verified tree' "$TMP/reuse.log" >/dev/null

CACHE_NAME=runner-change NPX="$TMP/bin/npx" run_hook >/dev/null
cp "$TMP/bin/npx" "$TMP/bin/npx-other"
CACHE_NAME=runner-change NPX="$TMP/bin/npx-other" run_hook > "$TMP/runner-change.log"
if grep -F 'reusing verified tree' "$TMP/runner-change.log" >/dev/null; then
  echo 'FAIL: a runner override changed without invalidating the cache' >&2
  exit 1
fi

: > "$LOG"
set +e
CACHE_NAME=dirty DIRTY=1 run_hook >/dev/null 2>&1
dirty_status=$?
set -e
[ "$dirty_status" -ne 0 ]
[ ! -e "$TMP/dirty" ]

: > "$LOG"
set +e
CACHE_NAME=failed FAIL_CLIPPY=1 run_hook
failed_status=$?
set -e
[ "$failed_status" -ne 0 ]
[ ! -e "$TMP/failed" ]

: > "$LOG"
CACHE_NAME=diff-failure DIFF_FAIL=1 run_hook
grep -F 'cargo clippy --workspace --exclude zeroclaw --all-targets -- -D warnings' "$LOG" >/dev/null
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=wrapper FAKE_DIFF=scripts/tests/pre-push-fast.test.js run_hook
grep -F "node --test scripts/tests/pre-push-fast.test.js cwd=$ROOT" "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=deleted FAKE_DIFF=apps/cloud/tests/deleted.rs run_hook
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=lockfile FAKE_DIFF=Cargo.lock run_hook
grep -F 'cargo test --workspace --exclude zeroclaw --lib --bins --tests' "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=web-source FAKE_DIFF=web/src/api/client.ts run_hook
grep -F "npx vitest run cwd=$ROOT/web" "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=self-change FAKE_DIFF=.githooks/pre-push.local PRE_PUSH_TEST_DELEGATE=1 run_hook
grep -F 'shell regression delegated' "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=package-manifest FAKE_DIFF=crates/db/Cargo.toml run_hook
grep -F 'cargo clippy --manifest-path crates/db/Cargo.toml --all-targets -- -D warnings' "$LOG" >/dev/null
grep -F 'cargo test --manifest-path crates/db/Cargo.toml' "$LOG" >/dev/null

: > "$LOG"
CACHE_NAME=custom-remote HOOK_REMOTE=upstream run_hook > "$TMP/custom-remote.log"
grep -F 'against 2222222' "$TMP/custom-remote.log" >/dev/null

: > "$LOG"
HOOK_INPUT='refs/heads/delete 0000000000000000000000000000000000000000 refs/heads/e4-p2-declaration-closeout 2222222' \
  CACHE_NAME=zero-oid run_hook
[ ! -s "$LOG" ]
[ ! -e "$TMP/zero-oid" ]

: > "$LOG"
HOOK_INPUT=$'refs/heads/one 1111111 refs/heads/e4-p2-declaration-closeout 2222222\nrefs/heads/two 1111111 refs/heads/e4-p2-declaration-closeout 2222222' \
  CACHE_NAME=multi run_hook
[ "$(grep -cF 'cargo fmt --all -- --check' "$LOG")" -eq 1 ]
cp "$LOG" "$TMP/multi-first.log"
HOOK_INPUT=$'refs/heads/one 1111111 refs/heads/e4-p2-declaration-closeout 2222222\nrefs/heads/two 1111111 refs/heads/e4-p2-declaration-closeout 2222222' \
  CACHE_NAME=multi run_hook > "$TMP/multi-reuse.log"
cmp "$TMP/multi-first.log" "$LOG" >/dev/null
[ "$(grep -cF 'reusing verified tree' "$TMP/multi-reuse.log")" -eq 2 ]

: > "$LOG"
set +e
HOOK_INPUT='refs/heads/other 2222222 refs/heads/e4-p2-declaration-closeout 2222222' \
  CACHE_NAME=non-head run_hook > "$TMP/non-head.log" 2>&1
non_head_status=$?
set -e
[ "$non_head_status" -ne 0 ]
grep -F 'does not match HEAD' "$TMP/non-head.log" >/dev/null
[ ! -e "$TMP/non-head" ]

echo 'PASS: targeted/broad/deleted dispatch, fail-closed cache, and per-OID verification'
