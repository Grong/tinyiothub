use serde_json::json;
use sha2::{Digest, Sha256};
use tinyiothub_core::harness::{
    EffectMutation, EffectState, HarnessEventInput, HarnessFact, HarnessState, Transition, TransitionPrecondition,
};

use super::types::{HarnessError, can_transition};

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
            state_version: next_version,
            external_ref: fact.external_ref.clone(),
            result: fact.result.clone(),
            updated_at: fact.occurred_at,
        }),
        event: Some(event),
        job: fact.job.clone(),
        wake: fact.wake.clone(),
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
