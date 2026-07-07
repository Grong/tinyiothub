//! Verify stage — cross-check LLM self-report against actual tool execution.
//!
//! Compares what the LLM claims it did (StepResult) with what actually happened
//! (ToolCallRecord list). Detects inconsistencies (lies), incomplete execution,
//! and runs the quality gate to produce a TickVerdict.

use crate::harness::types::{LieCounter, StepResult, StepStatus, StepVerdict, TickVerdict};

/// Cross-check a step's self-reported result against the actual tool calls made.
///
/// Detection rules:
/// - All tool calls failed but LLM reports Done → Lying
/// - Required step produced no tool calls and no output → Incomplete
/// - Otherwise → Consistent
pub fn cross_check(step: &StepResult) -> StepVerdict {
    // All tool calls failed but step claims Done
    if step.status == StepStatus::Done
        && !step.tool_calls.is_empty()
        && step.tool_calls.iter().all(|tc| !tc.success)
    {
        return StepVerdict::Lying {
            reason: format!(
                "Step '{}' reported Done but all {} tool calls failed",
                step.step_id,
                step.tool_calls.len()
            ),
        };
    }

    // Write-class tool had readback verification that failed
    if let Some(tc) = step.tool_calls.iter().find(|tc| tc.readback_verified == Some(false)) {
        return StepVerdict::Lying {
            reason: format!(
                "Step '{}' tool '{}' failed readback verification: {}",
                step.step_id,
                tc.name,
                tc.readback_detail.as_deref().unwrap_or("no detail")
            ),
        };
    }

    // Step is Done but produced no output and no tool calls
    if step.status == StepStatus::Done && step.tool_calls.is_empty() && step.output.is_empty() {
        return StepVerdict::Incomplete {
            reason: format!(
                "Step '{}' completed with no output or tool calls",
                step.step_id
            ),
        };
    }

    // Step failed
    if matches!(step.status, StepStatus::Failed { .. }) {
        return StepVerdict::Incomplete {
            reason: format!("Step '{}' failed", step.step_id),
        };
    }

    StepVerdict::Consistent
}

/// Quality gate — aggregate step verdicts into a tick-level verdict.
///
/// - Any lie → Fail
/// - Any incomplete → Partial (escalated step IDs)
/// - All consistent → Pass
pub fn quality_gate(verdicts: &[StepVerdict], step_ids: &[String]) -> TickVerdict {
    let has_lies = verdicts.iter().any(|v| matches!(v, StepVerdict::Lying { .. }));
    if has_lies {
        let reasons: Vec<String> = verdicts
            .iter()
            .filter_map(|v| match v {
                StepVerdict::Lying { reason } => Some(reason.clone()),
                _ => None,
            })
            .collect();
        return TickVerdict::Fail {
            reason: format!("Lie(s) detected: {}", reasons.join("; ")),
        };
    }

    let has_incomplete = verdicts
        .iter()
        .any(|v| matches!(v, StepVerdict::Incomplete { .. }));
    if has_incomplete {
        let escalated: Vec<String> = verdicts
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                if matches!(v, StepVerdict::Incomplete { .. }) {
                    step_ids.get(i).cloned()
                } else {
                    None
                }
            })
            .collect();
        return TickVerdict::Partial { escalated };
    }

    TickVerdict::Pass
}

/// Run full verification on a set of step results. Returns the tick verdict
/// and whether any lies were detected (for LieCounter tracking).
pub fn verify_steps(steps: &[StepResult]) -> (TickVerdict, bool) {
    let verdicts: Vec<StepVerdict> = steps.iter().map(cross_check).collect();
    let step_ids: Vec<String> = steps.iter().map(|s| s.step_id.clone()).collect();

    let lie_detected = verdicts.iter().any(|v| matches!(v, StepVerdict::Lying { .. }));
    let tick_verdict = quality_gate(&verdicts, &step_ids);

    (tick_verdict, lie_detected)
}

/// Check whether the agent should be degraded after recording lies.
/// Returns true if degradation threshold has been reached.
pub fn check_degradation(counter: &mut LieCounter, lie_detected: bool) -> bool {
    counter.record(lie_detected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::types::{StepResult, StepStatus, ToolCallRecord};

    fn make_step(id: &str, status: StepStatus, tool_calls: Vec<ToolCallRecord>, output: &str) -> StepResult {
        StepResult {
            step_id: id.into(),
            status,
            output: output.into(),
            tool_calls,
            retries: 0,
            duration_ms: 100,
        }
    }

    fn make_tool_call(name: &str, success: bool) -> ToolCallRecord {
        ToolCallRecord {
            name: name.into(),
            args: serde_json::json!({}),
            success,
            output: if success { "OK".into() } else { "Error: timeout".into() },
            proposed: false,
            readback_verified: None,
            readback_detail: None,
        }
    }

    #[test]
    fn test_cross_check_consistent() {
        let step = make_step(
            "1",
            StepStatus::Done,
            vec![make_tool_call("query", true)],
            "Device is online",
        );
        assert_eq!(cross_check(&step), StepVerdict::Consistent);
    }

    #[test]
    fn test_cross_check_lying_all_failed() {
        let step = make_step(
            "1",
            StepStatus::Done,
            vec![make_tool_call("cmd", false)],
            "All good!",
        );
        assert!(matches!(cross_check(&step), StepVerdict::Lying { .. }));
    }

    #[test]
    fn test_cross_check_lying_readback_failed() {
        let step = make_step(
            "1",
            StepStatus::Done,
            vec![ToolCallRecord {
                name: "write_config".into(),
                args: serde_json::json!({}),
                success: true,
                output: "OK".into(),
                proposed: false,
                readback_verified: Some(false),
                readback_detail: Some("Value mismatch: expected 100, got 0".into()),
            }],
            "Config updated successfully",
        );
        assert!(matches!(cross_check(&step), StepVerdict::Lying { .. }));
    }

    #[test]
    fn test_cross_check_incomplete_no_output() {
        let step = make_step("1", StepStatus::Done, vec![], "");
        assert!(matches!(cross_check(&step), StepVerdict::Incomplete { .. }));
    }

    #[test]
    fn test_cross_check_incomplete_failed_status() {
        let step = make_step(
            "1",
            StepStatus::Failed {
                reason: "timeout".into(),
            },
            vec![],
            "",
        );
        assert!(matches!(cross_check(&step), StepVerdict::Incomplete { .. }));
    }

    #[test]
    fn test_quality_gate_pass() {
        let verdicts = vec![StepVerdict::Consistent, StepVerdict::Consistent];
        let ids: Vec<String> = vec!["1".into(), "2".into()];
        assert_eq!(quality_gate(&verdicts, &ids), TickVerdict::Pass);
    }

    #[test]
    fn test_quality_gate_fail_on_lie() {
        let verdicts = vec![
            StepVerdict::Consistent,
            StepVerdict::Lying {
                reason: "faked success".into(),
            },
        ];
        let ids: Vec<String> = vec!["1".into(), "2".into()];
        assert!(matches!(quality_gate(&verdicts, &ids), TickVerdict::Fail { .. }));
    }

    #[test]
    fn test_quality_gate_partial() {
        let verdicts = vec![
            StepVerdict::Consistent,
            StepVerdict::Incomplete {
                reason: "timeout".into(),
            },
        ];
        let ids: Vec<String> = vec!["1".into(), "2".into()];
        match quality_gate(&verdicts, &ids) {
            TickVerdict::Partial { escalated } => {
                assert_eq!(escalated, vec!["2"]);
            }
            other => panic!("Expected Partial, got {:?}", other),
        }
    }

    #[test]
    fn test_verify_steps_all_consistent() {
        let steps = vec![
            make_step("1", StepStatus::Done, vec![make_tool_call("q", true)], "OK"),
            make_step("2", StepStatus::Done, vec![], "Done"),
        ];
        let (verdict, lie) = verify_steps(&steps);
        assert_eq!(verdict, TickVerdict::Pass);
        assert!(!lie);
    }

    #[test]
    fn test_lie_counter_resets_on_clean_tick() {
        let mut counter = LieCounter::new(3);
        // Two lies...
        assert!(!counter.record(true));
        assert!(!counter.record(true));
        assert_eq!(counter.consecutive_ticks, 2);
        // Clean tick resets
        assert!(!counter.record(false));
        assert_eq!(counter.consecutive_ticks, 0);
    }

    #[test]
    fn test_lie_counter_degradation() {
        let mut counter = LieCounter::new(3);
        assert!(!counter.record(true));
        assert!(!counter.record(true));
        // Third consecutive lie triggers degradation
        assert!(counter.record(true));
        assert!(counter.is_degraded());
    }

    #[test]
    fn test_lie_counter_reset() {
        let mut counter = LieCounter::new(2);
        counter.record(true);
        counter.reset();
        assert_eq!(counter.consecutive_ticks, 0);
        assert!(!counter.is_degraded());
    }
}
