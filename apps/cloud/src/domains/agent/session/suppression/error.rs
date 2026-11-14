//! Typed, stable suppression errors.
//!
//! Every new Harness error carries a stable snake_case `code`, a safe `what`
//! and `fix`, a recovery descriptor, an anchor into the developer docs, an
//! allowlisted context object, trace information and an explicit redaction
//! note. Payloads, memory contents, ref internals and credentials never enter
//! [`SuppressionError`].

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tinyiothub_storage::suppression::SuppressionState;

/// Redaction contract shared by every suppression error.
pub const REDACTION_NOTE: &str = "payloads, memory contents, ref internals and credentials omitted";

/// At most three total attempts: the initial call plus two retries.
pub const MAX_TOTAL_ATTEMPTS: u32 = 3;
/// Backoff before retry 1 and retry 2, in milliseconds.
pub const RECOVERY_BACKOFF_MS: [u64; 2] = [1_000, 2_000];
/// Session Events alert type. The Session Events adapter excludes
/// `assistant.*`, so an alert can never become semantic work and re-trigger a
/// wake.
pub const ALERT_EVENT_TYPE: &str = "assistant.suppression.alert";

/// Context keys that may leave the backend. Everything else (payloads, memory
/// and skill bodies, refs, workKeys, revisions, credentials, DB paths and URL
/// query strings) is dropped or sanitized.
const PUBLIC_CONTEXT_KEYS: &[&str] = &[
    "workspaceId",
    "sessionId",
    "turnId",
    "plannedWakeId",
    "eventId",
    "eventIds",
    "attemptId",
    "sourceId",
    "sourceVersion",
    "stateVersion",
    "registryVersion",
    "baselineRegistryVersion",
    "currentRegistryVersion",
    "expectedHash",
    "actualHash",
    "provider",
    "model",
    "contextWindow",
    "countedInputTokens",
    "outputReserveTokens",
    "countingMethod",
    "mode",
    "vetoes",
    "operation",
    "cause",
    "retryAfterMs",
    "consecutiveFailures",
    "baselineStateId",
    "notificationFailed",
    "metrics",
];

/// Every stable suppression error code: spec §11, the Task 7 HTTP operations
/// and the Task 1 source-contract validation codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    SourceUnavailable,
    SourceMissing,
    WorkItemPayloadMissing,
    SourceRevisionConflict,
    SourceRefMismatch,
    BaselineCorrupt,
    InputBuildFailed,
    ContextTooLarge,
    ContextBudgetUnavailable,
    AuditWriteFailed,
    ScheduleBarrierFailed,
    ProjectionReconcileFailed,
    NotificationFailed,
    FalseSkipDetected,
    ShadowGateNotMet,
    SuppressionConfigInvalid,
    WakeInFlight,
    IdempotencyConflict,
    PreflightTimeout,
    WakeBackpressure,
    CanonicalDuplicateKey,
    CanonicalNonFinite,
    CanonicalInvalidJson,
    InvalidRevision,
    InvalidHash,
    DuplicateSourceId,
    DuplicateWorkKey,
    SourceDescriptorMismatch,
    SourceContractInvalid,
}

/// One catalog entry. Static so the same contract cannot drift between codes.
#[derive(Clone, Copy, Debug)]
pub struct ErrorSpec {
    pub code: &'static str,
    pub what: &'static str,
    pub fix: &'static str,
    pub action: &'static str,
    pub max_attempts: u32,
    pub side_effects: &'static str,
    pub baseline_advanced: bool,
    pub reconcile: &'static str,
    pub channel: &'static str,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 29] = [
        Self::SourceUnavailable,
        Self::SourceMissing,
        Self::WorkItemPayloadMissing,
        Self::SourceRevisionConflict,
        Self::SourceRefMismatch,
        Self::BaselineCorrupt,
        Self::InputBuildFailed,
        Self::ContextTooLarge,
        Self::ContextBudgetUnavailable,
        Self::AuditWriteFailed,
        Self::ScheduleBarrierFailed,
        Self::ProjectionReconcileFailed,
        Self::NotificationFailed,
        Self::FalseSkipDetected,
        Self::ShadowGateNotMet,
        Self::SuppressionConfigInvalid,
        Self::WakeInFlight,
        Self::IdempotencyConflict,
        Self::PreflightTimeout,
        Self::WakeBackpressure,
        Self::CanonicalDuplicateKey,
        Self::CanonicalNonFinite,
        Self::CanonicalInvalidJson,
        Self::InvalidRevision,
        Self::InvalidHash,
        Self::DuplicateSourceId,
        Self::DuplicateWorkKey,
        Self::SourceDescriptorMismatch,
        Self::SourceContractInvalid,
    ];

    pub fn as_str(self) -> &'static str {
        self.spec().code
    }

    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|candidate| candidate.as_str() == code)
    }

    pub fn spec(self) -> ErrorSpec {
        let contract = |code, what, fix| ErrorSpec {
            code,
            what,
            fix,
            action: "fix_then_reconcile",
            max_attempts: 0,
            side_effects: "none",
            baseline_advanced: false,
            reconcile: "not_required",
            channel: "event + report",
        };
        match self {
            Self::SourceUnavailable => ErrorSpec {
                what: "a required suppression Source is unavailable",
                fix: "locate the Source via preflight/report, fix its dependency, then POST reconcile",
                reconcile: "required_before_next_skip",
                ..contract(
                    "source_unavailable",
                    "",
                    "locate the Source via preflight/report, fix its dependency, then POST reconcile",
                )
            },
            Self::SourceMissing => ErrorSpec {
                what: "a required suppression Source is no longer registered",
                fix: "re-register the Source adapter and POST reconcile",
                reconcile: "required_before_next_skip",
                ..contract(
                    "source_missing",
                    "",
                    "re-register the Source adapter and POST reconcile",
                )
            },
            Self::WorkItemPayloadMissing => ErrorSpec {
                what: "a closed work item has no retained payload",
                fix: "restore the closed tombstone payload before a full Turn; switch the workspace off if it cannot be restored",
                reconcile: "required_before_next_skip",
                ..contract(
                    "work_item_payload_missing",
                    "",
                    "restore the closed tombstone payload before a full Turn",
                )
            },
            Self::SourceRevisionConflict => ErrorSpec {
                what: "a Source reported a revision whose state hash conflicts",
                fix: "run the Source conformance test, fix revision generation, then POST reconcile",
                channel: "event + alert",
                reconcile: "required_before_next_skip",
                ..contract(
                    "source_revision_conflict",
                    "",
                    "run the Source conformance test and fix revision generation",
                )
            },
            Self::SourceRefMismatch => ErrorSpec {
                what: "an immutable reference does not match the captured content",
                fix: "rebuild the snapshot once; if it still fails, restore the immutable ref and reconcile without advancing the baseline",
                max_attempts: 1,
                reconcile: "required_before_next_skip",
                ..contract(
                    "source_ref_mismatch",
                    "",
                    "rebuild the snapshot once, then restore the immutable ref",
                )
            },
            Self::BaselineCorrupt => ErrorSpec {
                what: "the stored baseline cannot be parsed or bounded",
                fix: "discard the baseline and run a full Turn; switch off before repairing storage if corruption persists",
                action: "discard_and_full_turn",
                side_effects: "baseline discarded",
                ..contract("baseline_corrupt", "", "discard the baseline and run a full Turn")
            },
            Self::InputBuildFailed => ErrorSpec {
                what: "the complete Turn input could not be assembled",
                fix: "inspect traceId, fix the input source, then run a full Turn; switch off if it cannot be built",
                channel: "event + local trace",
                reconcile: "required_before_retry",
                ..contract("input_build_failed", "", "inspect traceId and fix the input source")
            },
            Self::ContextTooLarge => ErrorSpec {
                what: "assemble scheduled Turn: measured input + output reserve + overhead exceeds provider window",
                fix: "Reduce registered backlog or select an approved larger-context provider, then reconcile",
                max_attempts: MAX_TOTAL_ATTEMPTS,
                reconcile: "required_after_unknown_outcome",
                channel: "typed event + alert",
                ..contract(
                    "context_too_large",
                    "",
                    "reduce registered backlog or select an approved larger-context provider",
                )
            },
            Self::ContextBudgetUnavailable => ErrorSpec {
                what: "the provider context window or token counting method is unknown",
                fix: "configure the provider's real window and tokenizer/counting version, then reconcile; never guess",
                reconcile: "required_after_unknown_outcome",
                channel: "typed event + report",
                ..contract(
                    "context_budget_unavailable",
                    "",
                    "configure the provider's real window and tokenizer/counting version",
                )
            },
            Self::AuditWriteFailed => ErrorSpec {
                what: "the atomic skip audit transaction rolled back",
                fix: "repair the workspace DB, query events by plannedWakeAt, then retry at most three total attempts with 1s/2s backoff",
                action: "reconcile_then_retry",
                max_attempts: MAX_TOTAL_ATTEMPTS,
                side_effects: "rolled_back",
                reconcile: "required_before_retry",
                channel: "schedule audit_failed",
                ..contract(
                    "audit_write_failed",
                    "",
                    "repair the workspace DB and retry at most three total attempts",
                )
            },
            Self::ScheduleBarrierFailed => ErrorSpec {
                what: "the skip committed but the Schedule update is unconfirmed",
                fix: "query the audit by plannedWakeAt, reconcile the committed skip, then restore the Schedule; never blindly replay an unknown outcome",
                action: "reconcile_committed_skip",
                max_attempts: MAX_TOTAL_ATTEMPTS,
                side_effects: "committed_partial",
                reconcile: "required_before_retry",
                channel: "report + local trace",
                ..contract(
                    "schedule_barrier_failed",
                    "",
                    "reconcile the committed skip before restoring the Schedule",
                )
            },
            Self::ProjectionReconcileFailed => ErrorSpec {
                what: "the session projection could not be reconciled after a committed skip",
                fix: "read the committed events and reconcile the projection in at most three total attempts; persistent failure alerts and blocks further skips",
                max_attempts: MAX_TOTAL_ATTEMPTS,
                side_effects: "committed",
                reconcile: "required_before_retry",
                channel: "report + alert",
                ..contract(
                    "projection_reconcile_failed",
                    "",
                    "read the committed events and reconcile the projection",
                )
            },
            Self::NotificationFailed => ErrorSpec {
                what: "the post-commit SSE notification failed",
                fix: "clients resume from the high watermark; the server does not retry indefinitely, and persistent failures alert",
                max_attempts: MAX_TOTAL_ATTEMPTS,
                side_effects: "committed",
                reconcile: "required_before_retry",
                channel: "report + tracing",
                ..contract("notification_failed", "", "clients resume from the high watermark")
            },
            Self::FalseSkipDetected => ErrorSpec {
                what: "a dispatched skip was later proven to hide material changes",
                fix: "immediately PUT suppressionMode=off, then POST reconcile and compare baseline/current snapshots; re-enable only after root cause, two clean preflights and the shadow gate",
                action: "off_then_reconcile",
                side_effects: "unknown until reconciled",
                reconcile: "required_before_re_enable",
                channel: "alert + local trace",
                ..contract("false_skip_detected", "", "PUT suppressionMode=off then POST reconcile")
            },
            Self::ShadowGateNotMet => ErrorSpec {
                what: "enforce was requested without a passing shadow gate artifact",
                fix: "stay in shadow and inspect the gate metrics; never force enforce",
                channel: "HTTP 409 problem",
                ..contract("shadow_gate_not_met", "", "stay in shadow and inspect the gate metrics")
            },
            Self::SuppressionConfigInvalid => ErrorSpec {
                what: "the suppression configuration is outside the allowed range",
                fix: "correct the field and retry PUT; the previous configuration is unchanged and off with no other fields is always accepted",
                channel: "HTTP 400 problem",
                ..contract("suppression_config_invalid", "", "correct the field and retry PUT")
            },
            Self::WakeInFlight => ErrorSpec {
                what: "a scheduled or manual Turn is already in flight for this workspace",
                fix: "read the current Turn, wait for completion, then retry with the same Idempotency-Key; never wake concurrently",
                side_effects: "existing wake only",
                channel: "HTTP 409 problem",
                ..contract(
                    "wake_in_flight",
                    "",
                    "wait for the current Turn, then retry with the same Idempotency-Key",
                )
            },
            Self::IdempotencyConflict => ErrorSpec {
                what: "the same Idempotency-Key names a different operation or request fingerprint",
                fix: "use a new Idempotency-Key for a different intent; never reuse a key",
                channel: "HTTP 409 problem",
                ..contract(
                    "idempotency_conflict",
                    "",
                    "use a new Idempotency-Key for a different intent",
                )
            },
            Self::PreflightTimeout => ErrorSpec {
                what: "preflight Source capture exceeded its bound",
                fix: "inspect the slow Source, restore it, then run a full Turn; a timeout never advances the baseline and persistent timeouts alert",
                action: "fix_then_full_turn",
                channel: "event + report",
                ..contract("preflight_timeout", "", "inspect the slow Source and run a full Turn")
            },
            Self::WakeBackpressure => ErrorSpec {
                what: "wake capacity is saturated",
                fix: "retry with the same Idempotency-Key after retryAfterMs; do not enqueue additional requests",
                channel: "HTTP 429 problem",
                ..contract(
                    "wake_backpressure",
                    "",
                    "retry with the same Idempotency-Key after retryAfterMs",
                )
            },
            Self::CanonicalDuplicateKey => contract(
                "canonical_duplicate_key",
                "canonical JSON contains duplicate object keys",
                "emit each key exactly once and reconcile",
            ),
            Self::CanonicalNonFinite => contract(
                "canonical_non_finite",
                "canonical JSON contains a non-finite number",
                "emit finite JSON numbers only and reconcile",
            ),
            Self::CanonicalInvalidJson => contract(
                "canonical_invalid_json",
                "raw Source bytes are not valid JSON",
                "emit valid canonical JSON and reconcile",
            ),
            Self::InvalidRevision => contract(
                "invalid_revision",
                "the opaque revision length is outside 1..=256 UTF-8 bytes",
                "emit a revision within 1..=256 bytes and reconcile",
            ),
            Self::InvalidHash => contract(
                "invalid_hash",
                "the hash is not sha256: followed by 64 lowercase hex characters",
                "emit a sha256 hash and reconcile",
            ),
            Self::DuplicateSourceId => contract(
                "duplicate_source_id",
                "the Source registry contains duplicate sourceId values",
                "register each sourceId exactly once and reconcile",
            ),
            Self::DuplicateWorkKey => contract(
                "duplicate_work_key",
                "the Source snapshot contains duplicate workKey values",
                "emit each workKey exactly once and reconcile",
            ),
            Self::SourceDescriptorMismatch => contract(
                "source_descriptor_mismatch",
                "the captured snapshot does not match its registered descriptor",
                "fix the adapter descriptor/snapshot identity and reconcile",
            ),
            Self::SourceContractInvalid => contract(
                "source_contract_invalid",
                "the Source snapshot violates the shared contract",
                "fix the Source adapter contract and reconcile",
            ),
        }
    }
}

impl SuppressionError {
    /// Build a fully catalogued error for a stable code.
    pub fn from_code(code: ErrorCode) -> Self {
        let spec = code.spec();
        Self {
            code: spec.code.to_string(),
            what: spec.what.to_string(),
            fix: spec.fix.to_string(),
            recovery: Recovery {
                action: spec.action.to_string(),
                max_attempts: spec.max_attempts,
                side_effects: spec.side_effects.to_string(),
                baseline_advanced: spec.baseline_advanced,
                reconcile: spec.reconcile.to_string(),
            },
            code_anchor: format!("docs/dev/assistant-suppression.md#{}", spec.code),
            context: Value::Null,
            trace: TraceContext {
                trace_id: String::new(),
                local_path: None,
            },
            redaction: REDACTION_NOTE.to_string(),
            channel: spec.channel.to_string(),
        }
    }

    pub fn baseline_advanced(&self) -> bool {
        self.recovery.baseline_advanced
    }

    pub fn with_context(mut self, context: Value) -> Self {
        self.context = context;
        self
    }

    pub fn with_trace(mut self, trace_id: impl Into<String>, local_path: Option<impl Into<String>>) -> Self {
        self.trace = TraceContext {
            trace_id: trace_id.into(),
            local_path: local_path.map(Into::into),
        };
        self
    }

    /// Public, redacted projection. `trace.localPath` is local-only and never
    /// serialized here.
    pub fn public_value(&self) -> Value {
        json!({
            "code": self.code,
            "what": sanitize_public_text(&self.what),
            "fix": sanitize_public_text(&self.fix),
            "recovery": self.recovery,
            "codeAnchor": self.code_anchor,
            "context": public_context(&self.context),
            "trace": { "traceId": self.trace.trace_id },
            "redaction": self.redaction,
            "channel": self.channel,
        })
    }

    pub fn to_public_json(&self) -> String {
        self.public_value().to_string()
    }

    /// Async failure channel: the typed public error plus the safe summary the
    /// existing frontend renders as `errorMessage`.
    pub fn async_failure_payload(&self) -> Value {
        let summary = format!(
            "{} (code: {}, traceId: {})",
            sanitize_public_text(&self.what),
            self.code,
            self.trace.trace_id
        );
        json!({
            "error": self.public_value(),
            "errorMessage": summary,
        })
    }
}

fn safe_code(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
    .then_some(value)
}

fn public_context(value: &Value) -> Value {
    let Some(object) = value.as_object() else {
        return Value::Null;
    };
    let mut out = Map::new();
    for (key, value) in object {
        if PUBLIC_CONTEXT_KEYS.contains(&key.as_str()) {
            out.insert(key.clone(), sanitize_value(value));
        }
    }
    Value::Object(out)
}

fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(sanitize_public_text(text)),
        Value::Array(items) => Value::Array(items.iter().map(sanitize_value).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), sanitize_value(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Strip URL query strings, user-directory paths and credential shapes from
/// any text that can reach a public response.
///
/// Credentials are stateful: in `Authorization: Bearer <secret>` the secret is
/// the token after the scheme, so redaction must continue past the scheme
/// token. `api_key=`, `token=`, `access_token=`, `password=` and similar
/// single-token shapes are redacted in place.
pub fn sanitize_public_text(input: &str) -> String {
    let mut parts = Vec::new();
    let mut redact_next = false;
    for token in input.split_whitespace() {
        let mut token = token.to_string();
        if let Some(position) = token.find('?') {
            token.truncate(position);
        }
        let lower = token.to_ascii_lowercase();

        if redact_next {
            // Keep consuming scheme tokens so the real secret after `Bearer`
            // is redacted too.
            if matches!(
                lower.as_str(),
                "authorization" | "authorization:" | "bearer" | "bearer:" | "basic" | "basic:"
            ) {
                parts.push("<credentials>".to_string());
                continue;
            }
            parts.push("<credentials>".to_string());
            redact_next = false;
            continue;
        }

        if matches!(
            lower.as_str(),
            "authorization" | "authorization:" | "bearer" | "bearer:" | "basic" | "basic:"
        ) || lower.starts_with("authorization:")
            || lower.starts_with("bearer:")
        {
            parts.push("<credentials>".to_string());
            redact_next = true;
            continue;
        }

        if let Some((key, value)) = token.split_once('=') {
            let key_lower = key.to_ascii_lowercase();
            if matches!(
                key_lower.as_str(),
                "api_key"
                    | "apikey"
                    | "token"
                    | "access_token"
                    | "refresh_token"
                    | "password"
                    | "secret"
                    | "client_secret"
                    | "authorization"
            ) {
                parts.push(format!("{key}=<credentials>"));
                if matches!(value.to_ascii_lowercase().as_str(), "bearer" | "bearer:") {
                    redact_next = true;
                }
                continue;
            }
        }

        for prefix in ["/Users/", "/home/", "/root/"] {
            if let Some(position) = token.find(prefix) {
                token.truncate(position);
                token.push_str("<local-path>");
                break;
            }
        }
        parts.push(token);
    }
    parts.join(" ")
}

/// One attempt's classification for the bounded recovery loop.
#[derive(Clone, Debug, PartialEq)]
pub enum Attempt<T, E> {
    Succeeded(T),
    /// Storage-level failure: retried within the three-total-attempt bound.
    Transient(E),
    /// Configuration or contract failure: never auto-retried.
    Permanent(E),
    /// The commit result is unknown; reconcile by event/attempt identity first.
    Unknown(E),
}

/// Result of reconciling an unknown outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Reconcile<T> {
    /// Event/attempt identity proves the transition; no retry is needed.
    Settled(T),
    /// Reconciliation found no committed transition; a bounded retry may run.
    RetryAllowed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RecoveryOutcome<T, E> {
    Succeeded { value: T, attempts: u32, reconciled: bool },
    Permanent { error: E, attempts: u32 },
    UnknownUnreconciled { error: E, attempts: u32 },
    Exhausted { error: E, attempts: u32 },
}

/// Run one operation with the global recovery bound.
///
/// At most [`MAX_TOTAL_ATTEMPTS`] calls (initial + two retries) with 1s/2s
/// backoff are made, and only [`Attempt::Transient`] failures are retried
/// without reconciliation. An [`Attempt::Unknown`] first asks the caller to
/// reconcile by event/attempt identity; only `RetryAllowed` permits a retry.
pub async fn recover<Op, Fut, T, E, Rec, RecFut>(mut operation: Op, mut reconcile: Rec) -> RecoveryOutcome<T, E>
where
    Op: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Attempt<T, E>>,
    Rec: FnMut(u32) -> RecFut,
    RecFut: std::future::Future<Output = Reconcile<T>>,
{
    let mut attempts = 0;
    loop {
        attempts += 1;
        match operation(attempts).await {
            Attempt::Succeeded(value) => {
                return RecoveryOutcome::Succeeded {
                    value,
                    attempts,
                    reconciled: false,
                };
            }
            Attempt::Permanent(error) => return RecoveryOutcome::Permanent { error, attempts },
            Attempt::Unknown(error) => {
                if attempts >= MAX_TOTAL_ATTEMPTS {
                    return RecoveryOutcome::UnknownUnreconciled { error, attempts };
                }
                match reconcile(attempts).await {
                    Reconcile::Settled(value) => {
                        return RecoveryOutcome::Succeeded {
                            value,
                            attempts,
                            reconciled: true,
                        };
                    }
                    Reconcile::RetryAllowed => sleep_backoff(attempts).await,
                }
            }
            Attempt::Transient(error) => {
                if attempts >= MAX_TOTAL_ATTEMPTS {
                    return RecoveryOutcome::Exhausted { error, attempts };
                }
                sleep_backoff(attempts).await;
            }
        }
    }
}

async fn sleep_backoff(attempts: u32) {
    let index = (attempts.saturating_sub(1) as usize).min(RECOVERY_BACKOFF_MS.len() - 1);
    tokio::time::sleep(Duration::from_millis(RECOVERY_BACKOFF_MS[index])).await;
}

/// Public alert activity derived from real errors and audit fences.
///
/// `full_failed` alone is the baseline-ineligibility marker, not an alert: a
/// deliberate off-mode reconcile completes with no error and must not leave
/// `alerts.active` true.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertActivity {
    pub active: bool,
    pub code: Option<String>,
}

pub fn alert_activity(state: &SuppressionState) -> AlertActivity {
    // Cancellation and partial completion are durable failure fences, not
    // errors; every other lastError is an alert.
    let non_error = matches!(state.last_error.as_deref(), Some("partial" | "cancelled"));
    let error_active = state.last_error.is_some() && !non_error;
    // Sync barriers are real committed_partial/degraded fences: a persistent
    // consumer failure must stay visible in the report until it is cleared.
    let barrier_code = if state.audit_in_progress {
        Some("audit_in_progress")
    } else if state.schedule_sync_pending {
        Some("schedule_barrier_failed")
    } else if state.projection_sync_pending {
        Some("projection_reconcile_failed")
    } else if state.notification_pending {
        Some("notification_failed")
    } else {
        None
    };
    if !error_active && barrier_code.is_none() {
        return AlertActivity {
            active: false,
            code: None,
        };
    }
    let code = state
        .last_error
        .as_deref()
        .and_then(safe_code)
        .map(str::to_string)
        .or_else(|| barrier_code.map(str::to_string));
    AlertActivity { active: true, code }
}

/// `assistant.suppression.alert` payload. Only allow-listed correlation data;
/// never payloads, memory/skill bodies or credentials.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionAlert {
    pub code: String,
    pub workspace_id: String,
    pub turn_id: Option<String>,
    pub baseline_state_id: Option<String>,
    pub current_registry_version: Option<String>,
    pub baseline_registry_version: Option<String>,
    pub vetoes: Vec<String>,
    pub consecutive_failures: u32,
    pub metrics: Value,
    pub trace_id: String,
}

impl SuppressionAlert {
    pub fn payload(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Recovery guidance attached to an error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovery {
    pub action: String,
    pub max_attempts: u32,
    pub side_effects: String,
    pub baseline_advanced: bool,
    pub reconcile: String,
}

/// Correlation data. `local_path` is only ever populated locally.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceContext {
    pub trace_id: String,
    pub local_path: Option<String>,
}

/// Stable typed error used by the suppression gate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressionError {
    pub code: String,
    pub what: String,
    pub fix: String,
    pub recovery: Recovery,
    pub code_anchor: String,
    /// Allowlisted context only (IDs, versions, hashes, budgets, mode).
    pub context: Value,
    pub trace: TraceContext,
    pub redaction: String,
    pub channel: String,
}

impl SuppressionError {
    /// Build an error with safe defaults. Callers may replace `context`,
    /// `recovery` or `trace` afterwards.
    pub fn new(code: impl Into<String>, what: impl Into<String>, fix: impl Into<String>) -> Self {
        let code = code.into();
        // Producers keep their caller-specific `what`/`fix`, but a stable code
        // always receives its catalog recovery bound, side effects and channel.
        let (recovery, channel) = match ErrorCode::parse(&code) {
            Some(known) => {
                let spec = known.spec();
                (
                    Recovery {
                        action: spec.action.to_string(),
                        max_attempts: spec.max_attempts,
                        side_effects: spec.side_effects.to_string(),
                        baseline_advanced: spec.baseline_advanced,
                        reconcile: spec.reconcile.to_string(),
                    },
                    spec.channel.to_string(),
                )
            }
            None => (
                Recovery {
                    action: "none".to_string(),
                    max_attempts: 0,
                    side_effects: "none".to_string(),
                    baseline_advanced: false,
                    reconcile: "not_required".to_string(),
                },
                "event + report".to_string(),
            ),
        };
        Self {
            code_anchor: format!("docs/dev/assistant-suppression.md#{code}"),
            code,
            what: what.into(),
            fix: fix.into(),
            recovery,
            context: Value::Null,
            trace: TraceContext {
                trace_id: String::new(),
                local_path: None,
            },
            redaction: REDACTION_NOTE.to_string(),
            channel,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use serde_json::json;
    use tinyiothub_storage::suppression::SuppressionState;

    use super::{
        ALERT_EVENT_TYPE, Attempt, ErrorCode, MAX_TOTAL_ATTEMPTS, RECOVERY_BACKOFF_MS, REDACTION_NOTE, Reconcile,
        RecoveryOutcome, SuppressionError, alert_activity, recover,
    };

    fn context_too_large_fixture() -> SuppressionError {
        SuppressionError::from_code(ErrorCode::ContextTooLarge)
            .with_context(json!({
                "workspaceId": "ws-1",
                "turnId": "turn-1",
                "provider": "approved-provider",
                "model": "approved-model",
                "contextWindow": 128000,
                "outputReserveTokens": 8192,
                "countingMethod": "provider-tokenizer-v1"
            }))
            .with_trace("trace-1", Some("/Users/synthetic-secret/trace.json"))
    }

    #[test]
    fn error_serializes_with_camel_case_and_stable_anchor() {
        let error = SuppressionError::new("source_unavailable", "capture source", "restore source");
        let value = serde_json::to_value(&error).unwrap();
        assert_eq!(value["code"], "source_unavailable");
        assert_eq!(
            value["codeAnchor"],
            "docs/dev/assistant-suppression.md#source_unavailable"
        );
        assert_eq!(value["redaction"], REDACTION_NOTE);
        assert!(value.get("maxAttempts").is_none());
        assert_eq!(value["recovery"]["maxAttempts"], 0);
        assert_eq!(error.recovery.action, "fix_then_reconcile");
        assert_eq!(error.channel, "event + report");

        let audit = SuppressionError::new("audit_write_failed", "audit rolled back", "repair and retry");
        assert_eq!(audit.recovery.max_attempts, MAX_TOTAL_ATTEMPTS);
        assert_eq!(audit.recovery.side_effects, "rolled_back");
        assert_eq!(audit.channel, "schedule audit_failed");
    }

    #[test]
    fn catalog_covers_every_required_code_with_actionable_fields() {
        let expected = [
            "source_unavailable",
            "source_missing",
            "work_item_payload_missing",
            "source_revision_conflict",
            "source_ref_mismatch",
            "baseline_corrupt",
            "input_build_failed",
            "context_too_large",
            "context_budget_unavailable",
            "audit_write_failed",
            "schedule_barrier_failed",
            "projection_reconcile_failed",
            "notification_failed",
            "false_skip_detected",
            "shadow_gate_not_met",
            "suppression_config_invalid",
            "wake_in_flight",
            "idempotency_conflict",
            "preflight_timeout",
            "wake_backpressure",
            "canonical_duplicate_key",
            "canonical_non_finite",
            "canonical_invalid_json",
            "invalid_revision",
            "invalid_hash",
            "duplicate_source_id",
            "duplicate_work_key",
            "source_descriptor_mismatch",
            "source_contract_invalid",
        ];
        assert_eq!(ErrorCode::ALL.len(), expected.len());
        for (code, expected) in ErrorCode::ALL.iter().zip(expected) {
            let error = SuppressionError::from_code(*code);
            assert_eq!(error.code, expected);
            assert!(!error.what.trim().is_empty(), "code={expected}");
            assert!(!error.fix.trim().is_empty(), "code={expected}");
            assert!(!error.recovery.action.trim().is_empty(), "code={expected}");
            assert!(!error.recovery.side_effects.trim().is_empty(), "code={expected}");
            assert!(!error.recovery.reconcile.trim().is_empty(), "code={expected}");
            assert!(error.recovery.max_attempts <= MAX_TOTAL_ATTEMPTS, "code={expected}");
            assert!(!error.channel.trim().is_empty(), "code={expected}");
            assert_eq!(
                error.code_anchor,
                format!("docs/dev/assistant-suppression.md#{expected}")
            );
            assert_eq!(error.redaction, REDACTION_NOTE);
            assert_eq!(error.trace.trace_id, "");
            assert!(error.trace.local_path.is_none());
            assert!(error.context.is_null());
        }
    }

    #[test]
    fn recovery_side_effects_match_the_spec_table() {
        let cases = [
            (ErrorCode::SourceUnavailable, "none", "event + report"),
            (ErrorCode::WorkItemPayloadMissing, "none", "event + report"),
            (ErrorCode::SourceRevisionConflict, "none", "event + alert"),
            (ErrorCode::SourceRefMismatch, "none", "event + report"),
            (ErrorCode::BaselineCorrupt, "baseline discarded", "event + report"),
            (ErrorCode::InputBuildFailed, "none", "event + local trace"),
            (ErrorCode::ContextTooLarge, "none", "typed event + alert"),
            (ErrorCode::ContextBudgetUnavailable, "none", "typed event + report"),
            (ErrorCode::AuditWriteFailed, "rolled_back", "schedule audit_failed"),
            (
                ErrorCode::ScheduleBarrierFailed,
                "committed_partial",
                "report + local trace",
            ),
            (ErrorCode::ProjectionReconcileFailed, "committed", "report + alert"),
            (ErrorCode::NotificationFailed, "committed", "report + tracing"),
            (
                ErrorCode::FalseSkipDetected,
                "unknown until reconciled",
                "alert + local trace",
            ),
            (ErrorCode::ShadowGateNotMet, "none", "HTTP 409 problem"),
            (ErrorCode::SuppressionConfigInvalid, "none", "HTTP 400 problem"),
            (ErrorCode::WakeInFlight, "existing wake only", "HTTP 409 problem"),
            (ErrorCode::IdempotencyConflict, "none", "HTTP 409 problem"),
            (ErrorCode::PreflightTimeout, "none", "event + report"),
            (ErrorCode::WakeBackpressure, "none", "HTTP 429 problem"),
        ];
        for (code, side_effects, channel) in cases {
            let error = SuppressionError::from_code(code);
            assert_eq!(error.recovery.side_effects, side_effects, "code={}", error.code);
            assert_eq!(error.channel, channel, "code={}", error.code);
            assert!(!error.baseline_advanced(), "code={}", error.code);
        }
        assert_eq!(
            SuppressionError::from_code(ErrorCode::ContextTooLarge)
                .recovery
                .max_attempts,
            MAX_TOTAL_ATTEMPTS
        );
        for code in [ErrorCode::ShadowGateNotMet, ErrorCode::SuppressionConfigInvalid] {
            assert_eq!(
                SuppressionError::from_code(code).recovery.max_attempts,
                0,
                "permanent config errors must not be auto-retried"
            );
        }
    }

    #[test]
    fn context_error_is_actionable_and_redacted() {
        let error = context_too_large_fixture();
        assert_eq!(error.code, "context_too_large");
        assert_eq!(error.recovery.side_effects, "none");
        assert!(!error.baseline_advanced());
        assert!(!error.to_public_json().contains("synthetic-secret"));
    }

    #[test]
    fn public_error_omits_secrets_refs_paths_and_query_strings() {
        let error = context_too_large_fixture().with_context(json!({
            "workspaceId": "ws-1",
            "turnId": "turn-1",
            "payload": "synthetic-secret-payload",
            "memory": "synthetic-secret-memory",
            "skillBody": "synthetic-secret-skill",
            "workKey": "synthetic-secret-work-key",
            "revision": "synthetic-secret-revision",
            "dbPath": "/Users/synthetic-secret/harness.db",
            "sourceUrl": "https://example.test/state?synthetic-secret=1",
            "credential": "synthetic-secret-credential"
        }));
        let public = error.to_public_json();
        assert!(!public.contains("synthetic-secret"), "public={public}");
        assert!(!public.contains("localPath"), "local trace stays local");
        assert!(!public.contains("workKey"));
        assert!(!public.contains("revision"));
        assert!(public.contains("ws-1"));
        assert!(public.contains("turn-1"));
        assert!(public.contains("docs/dev/assistant-suppression.md#context_too_large"));
    }

    #[test]
    fn redacts_user_paths_and_url_queries_from_dynamic_text() {
        let error = SuppressionError::new(
            "input_build_failed",
            "read /Users/synthetic-secret/workspace/file.md from https://example.test/x?synthetic-secret=1",
            "inspect /home/synthetic-secret/state.db then retry",
        );
        let public = error.to_public_json();
        assert!(!public.contains("synthetic-secret"), "public={public}");
        assert!(!public.contains("/Users/"));
        assert!(!public.contains("/home/"));
    }

    #[test]
    fn redacts_authorization_bearer_and_key_value_credentials() {
        let error = SuppressionError::new(
            "input_build_failed",
            "Authorization: Bearer synthetic-bearer-secret \
             Bearer synthetic-standalone-secret \
             Authorization: Basic synthetic-basic-secret",
            "api_key=synthetic-api-key token=synthetic-token \
             access_token=synthetic-access-token password=synthetic-password \
             client_secret=synthetic-client-secret",
        );
        let public = error.to_public_json();
        let async_public = error.async_failure_payload().to_string();
        for secret in [
            "synthetic-bearer-secret",
            "synthetic-standalone-secret",
            "synthetic-basic-secret",
            "synthetic-api-key",
            "synthetic-token",
            "synthetic-access-token",
            "synthetic-password",
            "synthetic-client-secret",
        ] {
            assert!(!public.contains(secret), "public leaked {secret}: {public}");
            assert!(
                !async_public.contains(secret),
                "async failure leaked {secret}: {async_public}"
            );
        }
    }

    #[test]
    fn async_failure_uses_typed_error_and_safe_error_message() {
        let error = context_too_large_fixture().with_context(json!({
            "workspaceId": "ws-1",
            "payload": "synthetic-secret-payload"
        }));
        let payload = error.async_failure_payload();
        assert_eq!(payload["error"]["code"], "context_too_large");
        assert_eq!(payload["error"]["trace"]["traceId"], "trace-1");
        assert!(payload["error"].get("trace").unwrap().get("localPath").is_none());
        let message = payload["errorMessage"].as_str().expect("safe summary");
        assert!(!message.contains("synthetic-secret"));
        assert!(message.contains("context_too_large"));
        assert!(message.contains("trace-1"));
    }

    #[test]
    fn alert_activity_ignores_a_bare_full_failed_baseline_fence() {
        let mut state = SuppressionState::initial("ws-1", "agent:ws-1:__assistant__ws-1/main");
        assert!(!alert_activity(&state).active);

        // Off-mode reconcile completes without an error but keeps the old
        // baseline ineligible through `full_failed`; that alone is not an alert.
        state.full_failed = true;
        state.latest_outcome = "full_completed".to_string();
        let activity = alert_activity(&state);
        assert!(!activity.active);
        assert!(activity.code.is_none());

        state.notification_pending = true;
        let activity = alert_activity(&state);
        assert!(activity.active, "a persistent sync barrier is a real alert");
        assert_eq!(activity.code.as_deref(), Some("notification_failed"));
        state.notification_pending = false;

        state.last_error = Some("audit_write_failed".to_string());
        let activity = alert_activity(&state);
        assert!(activity.active);
        assert_eq!(activity.code.as_deref(), Some("audit_write_failed"));

        state.last_error = Some("cancelled".to_string());
        assert!(!alert_activity(&state).active, "cancellation is not an error alert");
        state.last_error = Some("partial".to_string());
        assert!(
            !alert_activity(&state).active,
            "partial completion is not an error alert"
        );

        state.last_error = None;
        state.audit_in_progress = true;
        let activity = alert_activity(&state);
        assert!(activity.active);
        assert_eq!(activity.code.as_deref(), Some("audit_in_progress"));
    }

    #[test]
    fn alert_event_type_stays_in_the_excluded_harness_namespace() {
        // The Session Events adapter excludes `assistant.*`; this alert must
        // never become semantic work and re-trigger a wake.
        assert_eq!(ALERT_EVENT_TYPE, "assistant.suppression.alert");
        assert!(ALERT_EVENT_TYPE.starts_with("assistant."));
    }

    #[test]
    fn alert_payload_has_camel_case_correlation_fields() {
        let alert = super::SuppressionAlert {
            code: "notification_failed".to_string(),
            workspace_id: "ws-1".to_string(),
            turn_id: Some("turn-1".to_string()),
            baseline_state_id: Some("state-1".to_string()),
            current_registry_version: Some("sha256:current".to_string()),
            baseline_registry_version: Some("sha256:baseline".to_string()),
            vetoes: vec!["new_user_input".to_string()],
            consecutive_failures: 3,
            metrics: json!({ "wouldSkip": false, "mode": "enforce" }),
            trace_id: "trace-1".to_string(),
        };
        let payload = alert.payload();
        assert_eq!(payload["code"], "notification_failed");
        assert_eq!(payload["workspaceId"], "ws-1");
        assert_eq!(payload["turnId"], "turn-1");
        assert_eq!(payload["baselineStateId"], "state-1");
        assert_eq!(payload["currentRegistryVersion"], "sha256:current");
        assert_eq!(payload["baselineRegistryVersion"], "sha256:baseline");
        assert_eq!(payload["vetoes"][0], "new_user_input");
        assert_eq!(payload["consecutiveFailures"], 3);
        assert_eq!(payload["metrics"]["wouldSkip"], false);
        assert_eq!(payload["traceId"], "trace-1");
    }

    #[tokio::test(start_paused = true)]
    async fn transient_failures_use_at_most_three_total_attempts_with_backoff() {
        assert_eq!(MAX_TOTAL_ATTEMPTS, 3);
        assert_eq!(RECOVERY_BACKOFF_MS, [1000, 2000]);
        let calls = Arc::new(AtomicU32::new(0));
        let op_calls = calls.clone();
        let started = tokio::time::Instant::now();
        let outcome: RecoveryOutcome<(), &'static str> = recover(
            move |_attempt| {
                let calls = op_calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Attempt::Transient("database locked")
                }
            },
            |_attempt| async { Reconcile::RetryAllowed },
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 3, "initial + 2 retries");
        assert!(matches!(outcome, RecoveryOutcome::Exhausted { attempts: 3, .. }));
        assert!(started.elapsed() >= Duration::from_secs(3), "1s + 2s backoff");
    }

    #[tokio::test(start_paused = true)]
    async fn permanent_config_or_contract_errors_are_not_retried() {
        let calls = Arc::new(AtomicU32::new(0));
        let op_calls = calls.clone();
        let outcome: RecoveryOutcome<(), &'static str> = recover(
            move |_attempt| {
                let calls = op_calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Attempt::Permanent("suppression_config_invalid")
                }
            },
            |_attempt| async { Reconcile::RetryAllowed },
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            outcome,
            RecoveryOutcome::Permanent {
                attempts: 1,
                error: "suppression_config_invalid"
            }
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_outcome_reconciles_by_attempt_identity_before_retry() {
        let calls = Arc::new(AtomicU32::new(0));
        let reconciles = Arc::new(AtomicU32::new(0));
        let op_calls = calls.clone();
        let reconcile_calls = reconciles.clone();
        let outcome: RecoveryOutcome<u8, &'static str> = recover(
            move |attempt| {
                let calls = op_calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    if attempt == 1 {
                        Attempt::Unknown("commit result unknown")
                    } else {
                        Attempt::Succeeded(7)
                    }
                }
            },
            move |_attempt| {
                let reconciles = reconcile_calls.clone();
                async move {
                    reconciles.fetch_add(1, Ordering::SeqCst);
                    Reconcile::RetryAllowed
                }
            },
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            reconciles.load(Ordering::SeqCst),
            1,
            "reconciliation must happen before any retry"
        );
        assert!(matches!(
            outcome,
            RecoveryOutcome::Succeeded {
                value: 7,
                attempts: 2,
                reconciled: false
            }
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn reconciliation_that_settles_the_unknown_outcome_stops_retrying() {
        let calls = Arc::new(AtomicU32::new(0));
        let op_calls = calls.clone();
        let outcome: RecoveryOutcome<u8, &'static str> = recover(
            move |_attempt| {
                let calls = op_calls.clone();
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Attempt::Unknown("commit result unknown")
                }
            },
            |_attempt| async { Reconcile::Settled(42) },
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(
            outcome,
            RecoveryOutcome::Succeeded {
                value: 42,
                attempts: 1,
                reconciled: true
            }
        ));
    }
}
