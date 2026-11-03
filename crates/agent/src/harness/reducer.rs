use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::{
    Decision, DecisionKind, DecisionMutation, DecisionState, EffectIntent, EffectMutation, EffectState,
    HarnessEventInput, HarnessFact, HarnessState, JobProjection, RunTaskProjection, Transition, TransitionPrecondition,
    WakeProjection,
};

use super::types::{HarnessError, ReconcileOutcome, can_transition};

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryFact {
    Fence,
    EnsureReconciliation,
    Reconcile(ReconcileOutcome),
    ConsumeAnswer,
    Cancel,
    ConsumeApproval,
}

pub fn reduce(state: &HarnessState, fact: &HarnessFact) -> Result<Transition, HarnessError> {
    let precondition = TransitionPrecondition {
        effect_id: state.effect_id.clone(),
        expected_state: state.status,
        expected_version: state.state_version,
    };
    if fact.effect_id != state.effect_id {
        return Err(HarnessError::IdentityMismatch);
    }
    if state.status.is_terminal() {
        return Ok(Transition {
            transition_id: fact.transition_id.clone(),
            precondition,
            effect: None,
            event: None,
            additional_events: Vec::new(),
            decision: None,
            run_task: None,
            job: None,
            wake: None,
        });
    }
    if !can_transition(state.status, fact.next) {
        return Err(HarnessError::InvalidTransition {
            from: state.status.as_str(),
            to: fact.next.as_str(),
        });
    }
    if fact.next == EffectState::Completed && fact.result.is_none() {
        return Err(HarnessError::MissingResult);
    }

    let next_version = state.state_version + 1;
    let payload = json!({
        "effectId": state.effect_id,
        "state": fact.next.as_str(),
        "result": fact.result,
    });
    let payload_digest = format!("{:x}", Sha256::digest(payload.to_string().as_bytes()));
    let event = HarnessEventInput {
        event_id: format!("{}:event", fact.transition_id),
        workspace_id: state.workspace_id.clone(),
        session_id: state.session_id.clone(),
        execution_mode: state.execution_mode,
        run_id: Some(state.run_id.clone()),
        task_id: Some(state.task_id.clone()),
        event_type: fact.next.event_type().to_string(),
        occurred_at: fact.occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest,
        payload_json: payload,
    };
    Ok(Transition {
        transition_id: fact.transition_id.clone(),
        precondition,
        effect: Some(EffectMutation {
            effect_id: state.effect_id.clone(),
            state: fact.next,
            recovery_from: None,
            state_version: next_version,
            external_ref: fact.external_ref.clone(),
            result: fact.result.clone(),
            updated_at: fact.occurred_at,
        }),
        event: Some(event),
        additional_events: Vec::new(),
        decision: None,
        run_task: None,
        job: fact.job.clone(),
        wake: fact.wake.clone(),
    })
}

fn recovery_event(
    intent: &EffectIntent,
    transition_id: &str,
    suffix: &str,
    event_type: &str,
    payload: Value,
    occurred_at: DateTime<Utc>,
) -> HarnessEventInput {
    HarnessEventInput {
        event_id: format!("{transition_id}:{suffix}"),
        workspace_id: String::new(),
        session_id: intent.session_id.clone(),
        execution_mode: tinyiothub_core::harness::ExecutionMode::Production,
        run_id: Some(intent.run_id.clone()),
        task_id: Some(intent.task_id.clone()),
        event_type: event_type.to_string(),
        occurred_at,
        cause_id: None,
        schema_v: 1,
        payload_digest: format!("{:x}", Sha256::digest(payload.to_string().as_bytes())),
        payload_json: payload,
    }
}

fn reconciliation_decision(intent: &EffectIntent, now: DateTime<Utc>) -> Decision {
    Decision {
        decision_id: format!("reconciliation:{}", intent.effect_id),
        kind: DecisionKind::EffectReconciliation,
        state: DecisionState::Requested,
        session_id: intent.session_id.clone(),
        run_id: intent.run_id.clone(),
        task_id: intent.task_id.clone(),
        effect_id: Some(intent.effect_id.clone()),
        subject: json!({"toolName": intent.tool_name, "requestDigest": intent.request_digest}),
        options: json!(["applied", "not_applied"]),
        created_at: now,
        updated_at: now,
        expires_at: None,
        answer: None,
        answer_actor: None,
        answer_event_id: None,
        answer_evidence: None,
        consumed_at: None,
        consumed_by: None,
    }
}

fn job_state(state: EffectState) -> &'static str {
    match state {
        EffectState::Prepared => "accepted",
        EffectState::Dispatching | EffectState::Cancelling => "running",
        EffectState::Completed => "done",
        EffectState::Failed => "failed",
        EffectState::NotApplied => "rejected",
        EffectState::Unknown => "interrupted",
        EffectState::Cancelled => "cancelled",
        EffectState::CancelledUnknown => "cancelled_unknown",
    }
}

pub fn reduce_recovery(
    intent: &EffectIntent,
    decision: Option<&Decision>,
    job_id: Option<&str>,
    transition_id: &str,
    occurred_at: DateTime<Utc>,
    current_task_state: &str,
    fact: RecoveryFact,
) -> Result<Transition, HarnessError> {
    if intent.state.is_terminal() {
        return Err(HarnessError::InvalidRecovery {
            state: intent.state.as_str(),
            outcome: "terminal",
        });
    }
    let effective_state = intent.recovery_from.unwrap_or(intent.state);
    let (next, result, outcome_name) = match &fact {
        RecoveryFact::Fence => (EffectState::Unknown, None, "fence"),
        RecoveryFact::EnsureReconciliation => (EffectState::Unknown, None, "ensure_reconciliation"),
        RecoveryFact::Cancel if intent.state == EffectState::Prepared => (
            EffectState::NotApplied,
            Some(json!({"reason":"cancel_before_send"})),
            "cancel",
        ),
        RecoveryFact::Cancel => (EffectState::Cancelling, None, "cancel"),
        RecoveryFact::ConsumeApproval => match decision.map(|value| (value.state, value.answer.as_deref())) {
            Some((DecisionState::Answered, Some("approved"))) => (EffectState::Dispatching, None, "approved"),
            Some((DecisionState::Answered, Some("denied"))) => {
                (EffectState::NotApplied, Some(json!({"reason":"denied"})), "denied")
            }
            Some((DecisionState::Expired | DecisionState::Withdrawn, _)) => (
                EffectState::NotApplied,
                Some(json!({"reason":"approval_closed"})),
                "closed",
            ),
            _ => return Err(HarnessError::DecisionConflict),
        },
        RecoveryFact::ConsumeAnswer => match decision.map(|value| (value.state, value.answer.as_deref())) {
            Some((DecisionState::Answered, Some("applied"))) => (
                EffectState::Completed,
                Some(json!({
                    "actor": decision.and_then(|value| value.answer_actor.as_deref()),
                    "evidence": decision.and_then(|value| value.answer_evidence.clone()),
                })),
                "applied",
            ),
            Some((DecisionState::Answered, Some("not_applied"))) => (
                EffectState::NotApplied,
                Some(json!({
                    "actor": decision.and_then(|value| value.answer_actor.as_deref()),
                    "evidence": decision.and_then(|value| value.answer_evidence.clone()),
                })),
                "not_applied",
            ),
            _ => return Err(HarnessError::DecisionConflict),
        },
        RecoveryFact::Reconcile(outcome) => match outcome {
            ReconcileOutcome::Settled(_) | ReconcileOutcome::DefinitelyNotApplied
                if effective_state == EffectState::Cancelling =>
            {
                return Err(HarnessError::InvalidRecovery {
                    state: intent.state.as_str(),
                    outcome: "execution_result_during_cancellation",
                });
            }
            ReconcileOutcome::Settled(result) => (EffectState::Completed, Some(result.clone()), "settled"),
            ReconcileOutcome::Failed(result) => (EffectState::Failed, Some(result.clone()), "failed"),
            ReconcileOutcome::Cancelled(result) if effective_state == EffectState::Cancelling => {
                (EffectState::Cancelled, Some(result.clone()), "cancelled")
            }
            ReconcileOutcome::DefinitelyNotApplied => (
                EffectState::NotApplied,
                Some(json!({"confirmedNotApplied":true})),
                "not_applied",
            ),
            ReconcileOutcome::Unknown { .. } if effective_state == EffectState::Cancelling => (
                EffectState::CancelledUnknown,
                Some(json!({"reason":"cancel_unknown"})),
                "cancel_unknown",
            ),
            ReconcileOutcome::Unknown { .. } => (EffectState::Unknown, None, "unknown"),
            ReconcileOutcome::Cancelled(_) => {
                return Err(HarnessError::InvalidRecovery {
                    state: intent.state.as_str(),
                    outcome: "cancelled",
                });
            }
        },
    };
    if next == EffectState::Unknown && matches!(fact, RecoveryFact::Reconcile(_)) {
        return Err(HarnessError::InvalidRecovery {
            state: intent.state.as_str(),
            outcome: outcome_name,
        });
    }
    if next == EffectState::Completed && result.is_none() {
        return Err(HarnessError::MissingResult);
    }

    let payload = json!({
        "effectId": intent.effect_id,
        "toolName": intent.tool_name,
        "requestDigest": intent.request_digest,
        "state": next.as_str(),
        "result": result,
    });
    let event = recovery_event(intent, transition_id, "event", next.event_type(), payload, occurred_at);
    let precondition = TransitionPrecondition {
        effect_id: intent.effect_id.clone(),
        expected_state: intent.state,
        expected_version: intent.state_version,
    };
    let mut additional_events = Vec::new();
    let decision_mutation = match &fact {
        RecoveryFact::Fence | RecoveryFact::EnsureReconciliation => {
            let existing = decision.filter(|value| value.kind == DecisionKind::EffectReconciliation);
            let requested = existing
                .cloned()
                .unwrap_or_else(|| reconciliation_decision(intent, occurred_at));
            if existing.is_none() {
                additional_events.push(recovery_event(
                    intent,
                    transition_id,
                    "decision",
                    "decision.requested",
                    json!({"decisionId":requested.decision_id,"kind":requested.kind.as_str()}),
                    occurred_at,
                ));
            }
            Some(DecisionMutation {
                decision: requested,
                expected_state: None,
                insert_if_missing: true,
            })
        }
        RecoveryFact::Reconcile(_) if intent.state == EffectState::Unknown => {
            let mut closed = decision.ok_or(HarnessError::DecisionConflict)?.clone();
            if closed.kind != DecisionKind::EffectReconciliation || closed.state != DecisionState::Requested {
                return Err(HarnessError::DecisionConflict);
            }
            closed.state = DecisionState::Withdrawn;
            closed.updated_at = occurred_at;
            additional_events.push(recovery_event(
                intent,
                transition_id,
                "decision",
                "decision.withdrawn",
                json!({"decisionId":closed.decision_id,"kind":closed.kind.as_str(),"reason":"auto_reconciled"}),
                occurred_at,
            ));
            Some(DecisionMutation {
                decision: closed,
                expected_state: Some(DecisionState::Requested),
                insert_if_missing: false,
            })
        }
        RecoveryFact::ConsumeAnswer | RecoveryFact::ConsumeApproval
            if decision.is_some_and(|value| value.state == DecisionState::Answered) =>
        {
            let mut consumed = decision.ok_or(HarnessError::DecisionConflict)?.clone();
            let expected = consumed.state;
            consumed.state = DecisionState::Consumed;
            consumed.updated_at = occurred_at;
            consumed.consumed_at = Some(occurred_at);
            consumed.consumed_by = Some("harness_recovery".to_string());
            additional_events.push(recovery_event(
                intent,
                transition_id,
                "decision",
                "decision.consumed",
                json!({"decisionId":consumed.decision_id,"kind":consumed.kind.as_str(),"answer":consumed.answer}),
                occurred_at,
            ));
            Some(DecisionMutation {
                decision: consumed,
                expected_state: Some(expected),
                insert_if_missing: false,
            })
        }
        RecoveryFact::ConsumeApproval => None,
        RecoveryFact::Cancel if intent.state == EffectState::Prepared => match decision {
            None => None,
            Some(existing)
                if existing.kind == DecisionKind::ToolApproval
                    && matches!(existing.state, DecisionState::Expired | DecisionState::Withdrawn) =>
            {
                None
            }
            Some(existing)
                if existing.kind == DecisionKind::ToolApproval
                    && (existing.state == DecisionState::Requested
                        || (existing.state == DecisionState::Answered
                            && matches!(existing.answer.as_deref(), Some("approved" | "denied")))) =>
            {
                let expected = existing.state;
                let mut withdrawn = existing.clone();
                withdrawn.state = DecisionState::Withdrawn;
                withdrawn.updated_at = occurred_at;
                additional_events.push(recovery_event(
                    intent,
                    transition_id,
                    "decision",
                    "decision.withdrawn",
                    json!({"decisionId":withdrawn.decision_id,"kind":withdrawn.kind.as_str(),"reason":"cancelled"}),
                    occurred_at,
                ));
                Some(DecisionMutation {
                    decision: withdrawn,
                    expected_state: Some(expected),
                    insert_if_missing: false,
                })
            }
            _ => return Err(HarnessError::DecisionConflict),
        },
        _ => None,
    };
    if next.is_terminal() {
        additional_events.push(recovery_event(
            intent,
            transition_id,
            "tool-result",
            "step.tool_result",
            json!({"effectId":intent.effect_id,"state":next.as_str(),"result":result}),
            occurred_at,
        ));
    }
    let task_state = match next {
        EffectState::Cancelled => "cancelled",
        EffectState::CancelledUnknown => "failed",
        EffectState::Cancelling => "cancelling",
        EffectState::NotApplied if matches!(&fact, RecoveryFact::Cancel) => "cancelled",
        EffectState::Completed | EffectState::Failed | EffectState::NotApplied
            if matches!(&fact, RecoveryFact::Reconcile(_)) && current_task_state != "running" =>
        {
            current_task_state
        }
        EffectState::Unknown if effective_state == EffectState::Cancelling => "cancelling",
        EffectState::Unknown => "paused",
        _ => "running",
    };
    let wake = (next.is_terminal() && !matches!(fact, RecoveryFact::ConsumeAnswer)).then(|| WakeProjection {
        task_id: intent.task_id.clone(),
        wake_id: format!("{transition_id}:wake"),
        signal_json: json!({"effectId":intent.effect_id,"state":next.as_str()}).to_string(),
        created_at: occurred_at,
    });
    Ok(Transition {
        transition_id: transition_id.to_string(),
        precondition,
        effect: Some(EffectMutation {
            effect_id: intent.effect_id.clone(),
            state: next,
            recovery_from: match &fact {
                RecoveryFact::Fence if intent.state == EffectState::Cancelling => Some(EffectState::Cancelling),
                RecoveryFact::Fence => None,
                RecoveryFact::EnsureReconciliation => intent.recovery_from,
                _ => None,
            },
            state_version: intent.state_version + 1,
            external_ref: intent.external_ref.clone(),
            result: result.clone(),
            updated_at: occurred_at,
        }),
        event: Some(event),
        additional_events,
        decision: decision_mutation,
        run_task: Some(RunTaskProjection {
            run_id: intent.run_id.clone(),
            run_state: task_state.to_string(),
            task_id: intent.task_id.clone(),
            task_state: task_state.to_string(),
        }),
        job: job_id.map(|job_id| JobProjection {
            job_id: job_id.to_string(),
            task_id: intent.task_id.clone(),
            run_id: intent.run_id.clone(),
            state: job_state(next).to_string(),
            result_json: result.clone(),
            external_ref: intent.external_ref.clone(),
            ended_at: next.is_terminal().then_some(occurred_at),
        }),
        wake,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use tinyiothub_core::harness::{
        EffectState, ExecutionMode, HarnessFact, HarnessState, JobProjection, WakeProjection,
    };

    use super::reduce;
    use crate::harness::HarnessError;

    fn at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-03T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    fn state(status: EffectState) -> HarnessState {
        HarnessState {
            workspace_id: "ws-1".to_string(),
            effect_id: "effect-1".to_string(),
            session_id: "session-1".to_string(),
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            execution_mode: ExecutionMode::Production,
            status,
            state_version: 7,
        }
    }

    fn fact(next: EffectState) -> HarnessFact {
        HarnessFact {
            transition_id: "transition-1".to_string(),
            effect_id: "effect-1".to_string(),
            next,
            occurred_at: at(),
            external_ref: Some("driver-call-1".to_string()),
            result: Some(json!({"confirmed": true})),
            job: Some(JobProjection {
                job_id: "job-1".to_string(),
                task_id: "task-1".to_string(),
                run_id: "run-1".to_string(),
                state: next.as_str().to_string(),
                result_json: Some(json!({"confirmed": true})),
                external_ref: Some("driver-call-1".to_string()),
                ended_at: next.is_terminal().then_some(at()),
            }),
            wake: Some(WakeProjection {
                task_id: "task-1".to_string(),
                wake_id: "wake-1".to_string(),
                signal_json: json!({"effectId": "effect-1"}).to_string(),
                created_at: at(),
            }),
        }
    }

    #[test]
    fn reducer_emits_versioned_atomic_transition() {
        let transition = reduce(&state(EffectState::Prepared), &fact(EffectState::Dispatching))
            .expect("prepared effect can acquire dispatch permit");
        let effect = transition.effect.expect("effect write");
        assert_eq!(effect.state, EffectState::Dispatching);
        assert_eq!(effect.state_version, 8);
        assert_eq!(transition.precondition.expected_version, 7);
        assert_eq!(transition.precondition.expected_state, EffectState::Prepared);
        let event = transition.event.expect("event write");
        assert_eq!(event.event_type, "effect.dispatching");
        assert_eq!(event.event_id, "transition-1:event");
        assert_eq!(event.payload_json["effectId"], "effect-1");
        assert!(transition.job.is_some());
        assert!(transition.wake.is_some());
    }

    #[test]
    fn reducer_accepts_only_declared_edges() {
        for (from, to) in [
            (EffectState::Prepared, EffectState::Dispatching),
            (EffectState::Prepared, EffectState::NotApplied),
            (EffectState::Dispatching, EffectState::Completed),
            (EffectState::Dispatching, EffectState::Failed),
            (EffectState::Dispatching, EffectState::Unknown),
            (EffectState::Dispatching, EffectState::Cancelling),
            (EffectState::Cancelling, EffectState::Cancelled),
            (EffectState::Cancelling, EffectState::CancelledUnknown),
            (EffectState::Unknown, EffectState::Completed),
            (EffectState::Unknown, EffectState::NotApplied),
        ] {
            reduce(&state(from), &fact(to)).unwrap_or_else(|error| panic!("{from:?} -> {to:?}: {error}"));
        }

        for (from, to) in [
            (EffectState::Prepared, EffectState::Completed),
            (EffectState::Prepared, EffectState::Unknown),
            (EffectState::Unknown, EffectState::Dispatching),
            (EffectState::Cancelling, EffectState::Prepared),
        ] {
            assert!(matches!(
                reduce(&state(from), &fact(to)),
                Err(HarnessError::InvalidTransition { .. })
            ));
        }
    }

    #[test]
    fn reducer_never_overwrites_terminal_state_with_late_facts() {
        for terminal in [
            EffectState::Completed,
            EffectState::Failed,
            EffectState::NotApplied,
            EffectState::Cancelled,
            EffectState::CancelledUnknown,
        ] {
            let transition = reduce(&state(terminal), &fact(EffectState::Failed))
                .expect("late fact returns the current terminal state");
            assert!(transition.effect.is_none());
            assert!(transition.event.is_none());
            assert!(transition.job.is_none());
            assert!(transition.wake.is_none());
            assert_eq!(transition.precondition.expected_state, terminal);
            assert_eq!(transition.precondition.expected_version, 7);
        }
    }

    #[test]
    fn reducer_rejects_mismatched_identity_and_resultless_completion() {
        let mut wrong_identity = fact(EffectState::Dispatching);
        wrong_identity.effect_id = "effect-2".to_string();
        assert!(matches!(
            reduce(&state(EffectState::Prepared), &wrong_identity),
            Err(HarnessError::IdentityMismatch)
        ));

        let mut completed = fact(EffectState::Completed);
        completed.result = None;
        assert!(matches!(
            reduce(&state(EffectState::Dispatching), &completed),
            Err(HarnessError::MissingResult)
        ));
    }

    #[test]
    fn reducer_source_has_no_runtime_storage_provider_or_device_io() {
        let source = include_str!("reducer.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        for forbidden in [
            concat!("to", "kio"),
            concat!("sql", "x"),
            concat!("pro", "vider"),
            concat!("device", "_hub"),
            "async fn",
        ] {
            assert!(!source.contains(forbidden), "reducer contains {forbidden}");
        }
    }
}
