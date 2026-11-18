# Assistant Error Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retry recoverable model-request failures and expose accurate errors, cancellation reasons, and retry progress in Assistant.

**Architecture:** Retry one provider request inside the rig adapter so rig never sees a failed attempt's terminal tool calls. A per-turn retry channel emits durable session events; chat cancellation becomes token-based so the runner can finish normally. The frontend consumes the same event contract and discards partial output from a failed attempt.

**Tech Stack:** Rust 2024, rig-agent 0.42, reqwest, Tokio, Axum SSE, Vite 7, Lit 3, Vitest.

**Spec:** `docs/designs/assistant-error-recovery.md`

## Global Constraints

- Retry at most 3 times after the initial request, with delays of 1, 2, and 4 seconds.
- Retry only recoverable system/provider failures; user cancellation and non-recoverable 4xx/configuration/validation failures stop immediately.
- Never restart a Turn or replay a successful tool. Device writes still require existing permission, approval, policy, and Effect gates.
- `turn.retry` payload is `{ turnId, attempt, maxRetries, delayMs, error }`; retry numbers are 1-based.
- Failed and cancelled `turn.completed` events include `error` and `source`; cancellation sources are `user`, `schedule`, or `system`.
- Chat SSE errors use `errorMessage`, not the currently mismatched `error`.
- Preserve the current-session history reseeding in wake/chat and all unrelated working-tree changes.
- Web commands require `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"`.

## Review Focus

- A stream fails after emitting partial text: the next attempt must not duplicate final or rendered content; Task 1 and Task 3 pin backend and frontend rollback.
- A failed attempt contains tool-call deltas: no tool executes until an attempt succeeds; Task 1 pins one tool execution after retry.
- Cancellation during backoff: stop promptly without another provider request; Task 1 pins cancellation behavior.
- Retry events arrive after history reload: the timeline must apply the same partial-output rollback; Task 3 tests event ingestion rather than live-only state.
- Provider error bodies may contain sensitive details: retain the existing 300-character truncation and do not log authorization headers; Task 1 pins bounded messages.

---

### Task 1: Provider retry policy and model-attempt events

**Files:**
- Modify: `crates/agent/src/port/provider.rs`
- Modify: `crates/agent/src/port/events.rs`
- Modify: `crates/agent/src/adapters/rig/provider.rs`
- Modify: `crates/agent/src/adapters/rig/loop_.rs`
- Test: `crates/agent/src/adapters/rig/tests.rs`

**Interfaces:**
- Produces: `ProviderError::{Http { status, message }, Stream { code, message }, Internal(String)}` and `ProviderError::is_retryable()`.
- Produces: `RetryNotice { attempt: u32, max_retries: u32, delay_ms: u64, error: String }` and `TurnEvent::Retry(RetryNotice)`.
- Produces: a per-turn retry-notice channel shared by `PortModelAsRig` and `RigAgentLoop`, together with the active turn's optional `CancellationToken`; no public provider signature changes.
- Produces: retry inside `PortModelAsRig::stream`, using the same `ChatRequest` for each attempt and emitting no terminal response for failed attempts.

- [ ] **Step 1: Write failing provider classification and retry tests**

Add tests proving HTTP 408/429/5xx, reqwest transport failures, stream failures, and `Internal` are retryable; 400/401/403/404/422, authentication/invalid-model stream codes, and `ImageUnavailable` are not. Add a paused-time adapter test where two attempts fail and the third succeeds, asserting three provider calls and retry notices `(1, 1000)` and `(2, 2000)`. Add a four-failure test asserting four total calls and the final real error. Add a non-recoverable HTTP 400 test asserting one call.

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p agent retry -- --nocapture`
Expected: FAIL because structured errors, retry notices, and retry behavior do not exist.

- [ ] **Step 3: Implement structured provider errors and bounded retry**

Extend `ProviderError` without preserving its unused `Copy` bound. Map non-success HTTP responses and in-band stream errors to structured errors while retaining the 300-character body/message truncation. Retry in `PortModelAsRig::stream` around `inner.chat_stream`; use `tokio::time::sleep` for 1/2/4-second delays and send `RetryNotice` before each delay. Treat reqwest transport/body errors as retryable. Do not add a retry loop around tool execution or the outer Turn.

- [ ] **Step 4: Wire retry notices into the active turn and roll back partial final text**

Give `PortModelAsRig` and `RigAgentLoop` a shared optional per-turn retry sender plus the active cancellation token, installed for `turn_streamed` and cleared by an RAII guard on every exit. Retry backoff uses `tokio::select!` against that token so cancellation prevents the next request. In the rig stream select loop, forward `TurnEvent::Retry` and remove the current model call's accumulated partial text from `final_text`; clear that per-call accumulator on `CompletionCall`. Existing callers that construct `PortModelAsRig` directly keep working with no event sink.

Add tests proving a retry after partial text emits `TurnEvent::Retry`, removes the failed attempt from the final text, and does not execute a returned tool more than once. Add a cancelled-during-backoff test using the existing cancellation token path.

- [ ] **Step 5: Run agent verification**

Run: `cargo test -p agent`
Expected: all agent tests pass, including the new retry, rollback, non-retry, and cancellation cases.

- [ ] **Step 6: Commit**

```bash
git add crates/agent/src/port/provider.rs crates/agent/src/port/events.rs crates/agent/src/adapters/rig/provider.rs crates/agent/src/adapters/rig/loop_.rs crates/agent/src/adapters/rig/tests.rs
git commit -m "feat(agent): retry recoverable model requests"
```

### Task 2: Durable retry, failure, and cancellation contracts

**Files:**
- Modify: `apps/cloud/src/domains/agent/session/runner.rs`
- Modify: `apps/cloud/src/domains/agent/session/chat/service.rs`
- Modify: `apps/cloud/src/domains/agent/session/chat/subagent.rs`
- Modify: `apps/cloud/src/domains/agent/session/wake.rs`
- Modify: `apps/cloud/src/domains/agent/session/chat_types.rs`
- Modify: `apps/cloud/src/domains/agent/pool/pool.rs`
- Modify: `apps/cloud/src/domains/agent/pool/chat.rs`

**Interfaces:**
- Consumes: `TurnEvent::Retry(RetryNotice)` from Task 1.
- Produces: `TurnCancellation { token, reason, source }` with constructors for user/schedule/system cancellation; `SessionRunner::run_turn` accepts `Option<TurnCancellation>` while `AgentLoop` still receives only its token.
- Produces: `TurnOutcome::Cancelled { reason, source }`; failed outcomes remain `Failed { error }`.
- Produces: `ChatEvent::Error { runId, sessionKey, errorMessage, source }`.
- Changes: `chat_handles` stores cancellation tokens instead of `JoinHandle<()>`; abort cancels rather than aborting the task.

- [ ] **Step 1: Write failing runner and chat-contract tests**

Add runner tests asserting `TurnEvent::Retry` writes `turn.retry` with the exact approved payload, failed turns include `error` and `source: "system"`, and cancelled turns include their readable reason and source. Add chat serialization tests asserting the wire field is `errorMessage`. Add a chat abort test proving cancellation produces `turn.completed(outcome=cancelled, source=user)` instead of silently dropping the runner.

- [ ] **Step 2: Run the tests and verify RED**

Run: `cargo test -p tinyiothub-cloud domains::agent::session --lib`
Expected: FAIL on the missing retry event, cancellation metadata, and `errorMessage` wire field.

- [ ] **Step 3: Implement the session and chat contracts**

Persist retry notices in `SessionRunner`; keep observer forwarding for existing event consumers. Add cancellation metadata and include it in terminal events and `TurnOutcome`. Replace chat's stored `JoinHandle` with a `CancellationToken`, pass user cancellation metadata into `run_turn`, and keep handle removal on every return. Wake supplies schedule cancellation metadata; subagent and other non-cancellable callers pass `None`. Send actual failure text or cancellation reason through `ChatEvent::Error` with its source.

- [ ] **Step 4: Run cloud verification**

Run: `cargo test -p tinyiothub-cloud domains::agent::session --lib`
Expected: all session tests pass, including existing approval, question, history isolation, wake cancellation, and subagent tests.

- [ ] **Step 5: Commit only this task's changes**

Stage the listed files selectively where they already contain unrelated work. Do not include existing streaming, UI, prompt, or lockfile changes.

```bash
git commit -m "fix(agent): expose retry and cancellation details"
```

### Task 3: Frontend retry and error presentation

**Files:**
- Modify: `web/src/ui/controllers/chat.ts`
- Modify: `web/src/ui/controllers/chat.test.ts`
- Modify: `web/src/ui/chat/timeline-model.ts`
- Modify: `web/src/ui/chat/timeline-model.test.ts`
- Modify: `web/src/ui/chat/timeline-render.ts`
- Modify: `web/src/ui/chat/timeline-render.test.ts`
- Modify: `web/src/styles/chat/codex.css`

**Interfaces:**
- Consumes: chat `{ state: "error", errorMessage, source }` and session `turn.retry` / `turn.completed` contracts from Task 2.
- Produces: timeline `retry` block with `turnId`, `attempt`, `maxRetries`, `delayMs`, `error`, and `at`.

- [ ] **Step 1: Write failing frontend tests**

Add a chat controller test proving `{ state: "error", errorMessage: "HTTP 500: upstream unavailable" }` sets that exact `lastError`. Add timeline tests proving `turn.retry` renders attempt/count/delay/error and removes only trailing streaming assistant/reasoning blocks from the failed model attempt, while prior tool results remain. Add a cancelled-turn test proving its reason and source survive reload.

- [ ] **Step 2: Run the tests and verify RED**

Run: `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && cd web && npx vitest run src/ui/controllers/chat.test.ts src/ui/chat/timeline-model.test.ts src/ui/chat/timeline-render.test.ts`
Expected: FAIL because the live field and retry timeline behavior are missing.

- [ ] **Step 3: Implement the frontend contract and retry block**

Read `errorMessage` in the chat controller. Add the retry block and renderer with concise Chinese copy for the real error, retry count, and delay. On retry, discard trailing streaming assistant/reasoning blocks for the same turn before inserting the retry block; do not remove tool blocks or earlier completed content. Keep final failure/cancellation errors in the existing turn error area and use existing design-system colors/spacing for the retry notice.

- [ ] **Step 4: Run frontend verification**

Run: `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && cd web && npx vitest run src/ui/controllers/chat.test.ts src/ui/chat/timeline-model.test.ts src/ui/chat/timeline-render.test.ts && npm run build`
Expected: focused tests and production build pass.

- [ ] **Step 5: Commit only this task's changes**

Stage only the listed frontend hunks; preserve unrelated Assistant UI changes already in the working tree.

```bash
git commit -m "fix(web): show assistant retries and real errors"
```

## Final Verification

- `cargo test -p agent`
- `cargo test -p tinyiothub-cloud domains::agent::session --lib`
- `cargo fmt --all -- --check`
- `export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH" && cd web && npx vitest run src/ui/controllers/chat.test.ts src/ui/chat/timeline-model.test.ts src/ui/chat/timeline-render.test.ts && npm run build`
- Run `/verify` before the final commit; report any pre-existing or environment failures without claiming a clean full suite.
