//! Execute stage — run a single PlanStep through the agent with trust gating.
//!
//! PreToolUse: checks TrustEngine on every tool call observed in the stream.
//! PostToolUse: readback verification for write-class tools via follow-up query.
//! Retries: on PostToolUse mismatch, retries up to step.max_retries.

use std::sync::Arc;
use std::time::Instant;

use tracing::{debug, info, warn};

use crate::agent::pool::AgentPoolLike;
use crate::harness::types::{
    PlanStep, PreToolDecision, StepResult, StepStatus, StreamEvent, ToolCallRecord,
};
use crate::policy::{PolicyCategory, PolicyEngine};
use crate::tool::trust::{TrustConfig, classify_tool_safety, evaluate_tool_trust, ToolSafety};

/// Execute a single plan step through the agent.
///
/// 1. Build a step-specific prompt
/// 2. Send via `send_message_streamed()` for real-time tool observation
/// 3. Apply PreToolUse trust checks on observed tool calls
/// 4. PostToolUse: readback verification for write-class tools
/// 5. Retry up to `max_retries` on readback mismatch
pub async fn execute_step(
    step: &PlanStep,
    agent_pool: &Arc<dyn AgentPoolLike>,
    trust_config: &TrustConfig,
    policy_engine: &Arc<dyn PolicyEngine>,
    workspace_id: &str,
) -> StepResult {
    let start = Instant::now();
    let mut last_error: Option<String> = None;

    for attempt in 0..=step.max_retries {
        if attempt > 0 {
            debug!(
                workspace_id,
                step_id = %step.id,
                attempt,
                "Retrying step after failure"
            );
        }

        match execute_step_once(step, agent_pool, trust_config, policy_engine, workspace_id).await {
            Ok(mut result) => {
                result.retries = attempt;
                result.duration_ms = start.elapsed().as_millis() as u64;

                // PostToolUse: readback verification for write tools
                if let Some(failure) = verify_write_tools(&result, agent_pool, workspace_id).await {
                    if attempt < step.max_retries {
                        warn!(
                            workspace_id,
                            step_id = %step.id,
                            attempt,
                            reason = %failure,
                            "PostToolUse verification failed, will retry"
                        );
                        last_error = Some(failure);
                        continue;
                    }
                    // Exhausted retries — record as failed
                    return StepResult {
                        step_id: step.id.clone(),
                        status: StepStatus::Failed {
                            reason: format!(
                                "PostToolUse verification failed after {} retries: {}",
                                attempt, failure
                            ),
                        },
                        output: result.output,
                        tool_calls: result.tool_calls,
                        retries: attempt,
                        duration_ms: start.elapsed().as_millis() as u64,
                    };
                }

                return result;
            }
            Err(e) => {
                last_error = Some(e.to_string());
                if attempt < step.max_retries {
                    warn!(
                        workspace_id,
                        step_id = %step.id,
                        attempt,
                        error = %e,
                        "Step execution failed, will retry"
                    );
                    continue;
                }
            }
        }
    }

    StepResult {
        step_id: step.id.clone(),
        status: StepStatus::Failed {
            reason: last_error.unwrap_or_else(|| "Unknown error".into()),
        },
        output: String::new(),
        tool_calls: vec![],
        retries: step.max_retries,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Single attempt at executing a step (no retry).
async fn execute_step_once(
    step: &PlanStep,
    agent_pool: &Arc<dyn AgentPoolLike>,
    trust_config: &TrustConfig,
    policy_engine: &Arc<dyn PolicyEngine>,
    workspace_id: &str,
) -> anyhow::Result<StepResult> {
    let start = Instant::now();
    let mut output = String::new();
    let mut tool_calls: Vec<ToolCallRecord> = Vec::new();

    let prompt = build_step_prompt(step);

    let mut rx = agent_pool.send_message_streamed(workspace_id, &prompt).await?;

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Chunk { delta } | StreamEvent::Thinking { delta } => {
                output.push_str(&delta);
            }

            StreamEvent::ToolCall { id: _id, name, args } => {
                // PreToolUse: check trust before recording
                let trust_decision = evaluate_tool_trust(trust_config, &name);
                let policy_decision = policy_engine
                    .evaluate(workspace_id, PolicyCategory::ToolExecution, &name)
                    .await;

                let pre_tool = PreToolDecision::from(trust_decision);

                match pre_tool {
                    PreToolDecision::Allow => {
                        // Tool will auto-execute (AutonomyLevel::Full).
                        // Record is pending — result comes in ToolResult event.
                        debug!(
                            workspace_id,
                            tool = %name,
                            "PreToolUse: Allow"
                        );
                    }
                    PreToolDecision::Block { reason } => {
                        warn!(
                            workspace_id,
                            tool = %name,
                            reason = %reason,
                            "PreToolUse: Block — tool call observed but should have been blocked"
                        );
                        tool_calls.push(ToolCallRecord {
                            name: name.clone(),
                            args,
                            success: false,
                            output: reason,
                            proposed: false,
                            readback_verified: None,
                            readback_detail: None,
                        });
                    }
                    PreToolDecision::Propose { reason } => {
                        info!(
                            workspace_id,
                            tool = %name,
                            reason = %reason,
                            "PreToolUse: Propose — recording as proposed call"
                        );
                        tool_calls.push(ToolCallRecord {
                            name: name.clone(),
                            args,
                            success: false,
                            output: format!("PROPOSAL: {}", reason),
                            proposed: true,
                            readback_verified: None,
                            readback_detail: None,
                        });
                    }
                }

                // Check policy for block
                if let crate::policy::PolicyDecision::Block { reason } = policy_decision {
                    warn!(
                        workspace_id,
                        tool = %name,
                        reason = %reason,
                        "Tool blocked by policy engine"
                    );
                    // Override with policy block
                    if let Some(last) = tool_calls.last_mut()
                        && last.name == name
                    {
                        last.success = false;
                        last.output = format!("POLICY_BLOCK: {}", reason);
                        last.proposed = false;
                    }
                }
            }

            StreamEvent::ToolResult {
                id: _id,
                name,
                output: tool_output,
                success,
            } => {
                // Update existing pending record or create new one
                if let Some(pending) = tool_calls
                    .iter_mut()
                    .rev()
                    .find(|tc| tc.name == name && tc.success && tc.output.is_empty())
                {
                    pending.success = success;
                    pending.output = tool_output;
                } else if let Some(last) = tool_calls.last_mut() {
                    if last.name == name {
                        last.success = success;
                        last.output = tool_output;
                    }
                } else {
                    tool_calls.push(ToolCallRecord {
                        name,
                        args: serde_json::Value::Null,
                        success,
                        output: tool_output,
                        proposed: false,
                        readback_verified: None,
                        readback_detail: None,
                    });
                }
            }

            StreamEvent::Final { text } => {
                if !text.is_empty() {
                    output = text;
                }
            }

            StreamEvent::Error { message } => {
                warn!(workspace_id, step_id = %step.id, error = %message, "Stream error during step execution");
            }
        }
    }

    let status = StepStatus::Done;

    Ok(StepResult {
        step_id: step.id.clone(),
        status,
        output,
        tool_calls,
        retries: 0,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// PostToolUse: verify write-class tools via readback query.
///
/// Returns Some(reason) if verification failed, None if OK or not needed.
async fn verify_write_tools(
    result: &StepResult,
    agent_pool: &Arc<dyn AgentPoolLike>,
    workspace_id: &str,
) -> Option<String> {
    for tc in &result.tool_calls {
        // Only verify write-class tools that reported success
        if !tc.success || tc.proposed {
            continue;
        }

        let safety = classify_tool_safety(&tc.name);
        if !matches!(safety, ToolSafety::Write | ToolSafety::Destructive) {
            continue;
        }

        // Build a readback verification prompt
        let verify_prompt = format!(
            "You just called the tool '{}'. Verify that the operation actually took effect. \
             Check the current state and confirm whether the change was applied correctly. \
             Respond with ONLY 'VERIFIED: <yes/no>' followed by a brief explanation.",
            tc.name
        );

        match agent_pool.send_message(workspace_id, &verify_prompt).await {
            Ok(response) => {
                let verified = response.to_lowercase().contains("verified: yes");
                if !verified {
                    return Some(format!(
                        "Readback for '{}' failed: {}",
                        tc.name,
                        response.chars().take(200).collect::<String>()
                    ));
                }
            }
            Err(e) => {
                return Some(format!("Readback for '{}' unavailable: {}", tc.name, e));
            }
        }
    }

    None
}

/// Build a focused prompt for a single plan step.
fn build_step_prompt(step: &PlanStep) -> String {
    let mut prompt = format!(
        "## Step {}: {}\n\nExecute this step now. ",
        step.id, step.title
    );

    if !step.tool_hints.is_empty() {
        prompt.push_str(&format!(
            "Suggested tools: {}. ",
            step.tool_hints.join(", ")
        ));
    }

    prompt.push_str("Report what you did and what you found.\n");

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_step_prompt_includes_hints() {
        let step = PlanStep {
            id: "1".into(),
            title: "Investigate alarm".into(),
            required: true,
            max_retries: 1,
            tool_hints: vec!["query_alarms".into(), "query_devices".into()],
            on_failure: crate::harness::types::FailureAction::Escalate {
                message: "critical".into(),
            },
        };
        let prompt = build_step_prompt(&step);
        assert!(prompt.contains("Step 1"));
        assert!(prompt.contains("Investigate alarm"));
        assert!(prompt.contains("query_alarms"));
        assert!(prompt.contains("query_devices"));
    }

    #[test]
    fn test_build_step_prompt_no_hints() {
        let step = PlanStep {
            id: "2".into(),
            title: "Report".into(),
            required: false,
            max_retries: 0,
            tool_hints: vec![],
            on_failure: crate::harness::types::FailureAction::SkipAndContinue,
        };
        let prompt = build_step_prompt(&step);
        assert!(prompt.contains("Step 2"));
        assert!(prompt.contains("Report"));
        assert!(!prompt.contains("Suggested tools"));
    }
}
