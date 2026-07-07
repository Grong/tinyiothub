//! Plan stage — build execution steps from tasks or chat messages.
//!
//! Maps heartbeat task text to step templates, injects plan into system prompt
//! as an ASCII box for the LLM to follow.

use crate::heartbeat::types::HeartbeatTask;
use crate::harness::types::{FailureAction, PlanStep};

/// Build a plan from heartbeat tasks. Each task maps to one or more steps
/// based on keyword matching against task text.
pub fn build_plan(tasks: &[HeartbeatTask]) -> Vec<PlanStep> {
    let mut steps: Vec<PlanStep> = Vec::new();
    let mut step_num = 0u32;

    for task in tasks {
        if task.paused {
            continue;
        }

        let text = task.text.to_lowercase();

        if text.contains("alarm") || text.contains("告警") || text.contains("报警") {
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: format!("Investigate alarm: {}", task.text),
                required: true,
                max_retries: 2,
                tool_hints: vec!["query_devices".into(), "query_alarms".into(), "analyze_context".into()],
                on_failure: FailureAction::Escalate {
                    message: "Alarm investigation failed after retries".into(),
                },
            });
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: "Decide action: escalate or resolve".into(),
                required: true,
                max_retries: 1,
                tool_hints: vec!["resolve_alarm".into(), "send_notification".into()],
                on_failure: FailureAction::Escalate {
                    message: "Action decision failed".into(),
                },
            });
        } else if text.contains("device") || text.contains("设备") || text.contains("check") {
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: format!("Check device status: {}", task.text),
                required: true,
                max_retries: 1,
                tool_hints: vec!["query_devices".into(), "check_status".into()],
                on_failure: FailureAction::SkipAndContinue,
            });
        } else if text.contains("daily") || text.contains("summary") || text.contains("报告") || text.contains("summary") {
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: "Query recent metrics".into(),
                required: false,
                max_retries: 1,
                tool_hints: vec!["query_metrics".into(), "query_devices".into()],
                on_failure: FailureAction::SkipAndContinue,
            });
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: "Compile summary report".into(),
                required: true,
                max_retries: 1,
                tool_hints: vec!["send_notification".into()],
                on_failure: FailureAction::SkipAndContinue,
            });
        } else {
            // Generic fallback: gather info → report
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: format!("Gather information for: {}", task.text),
                required: false,
                max_retries: 1,
                tool_hints: vec!["query_devices".into(), "query_metrics".into()],
                on_failure: FailureAction::SkipAndContinue,
            });
            step_num += 1;
            steps.push(PlanStep {
                id: step_num.to_string(),
                title: "Report findings".into(),
                required: true,
                max_retries: 1,
                tool_hints: vec!["send_notification".into()],
                on_failure: FailureAction::SkipAndContinue,
            });
        }
    }

    steps
}

/// Build a single-step plan for chat — the harness observes the LLM's
/// tool calls in real-time without pre-defining steps.
pub fn build_chat_plan(message: &str) -> Vec<PlanStep> {
    vec![PlanStep {
        id: "chat".to_string(),
        title: format!("Respond to: {}", truncate(message, 80)),
        required: true,
        max_retries: 0,
        tool_hints: vec![],
        on_failure: FailureAction::SkipAndContinue,
    }]
}

/// Inject the execution plan into the system prompt as an ASCII box.
/// The plan helps the LLM understand the structured execution flow.
pub fn inject_plan_prompt(steps: &[PlanStep], base_prompt: &str) -> String {
    if steps.is_empty() {
        return base_prompt.to_string();
    }

    let mut prompt = base_prompt.to_string();
    prompt.push_str("\n\n## Execution Plan\n\n");
    prompt.push_str("Follow these steps in order. After each step, report what you did and what you found.\n\n");

    // ASCII box header
    let box_width = 64;
    prompt.push_str(&format!("+{}+\n", "-".repeat(box_width)));

    for step in steps {
        let required = if step.required { "[REQUIRED]" } else { "[OPTIONAL]" };
        let action = match &step.on_failure {
            FailureAction::Retry { max } => format!("on failure: retry up to {}x", max),
            FailureAction::SkipAndContinue => "on failure: skip".to_string(),
            FailureAction::Escalate { message } => format!("on failure: ESCALATE — {}", message),
        };

        // Step header
        prompt.push_str(&format!(
            "| Step {}: {} {} |\n",
            step.id,
            pad_right(&step.title, 40),
            required
        ));

        // Hints
        if !step.tool_hints.is_empty() {
            prompt.push_str(&format!(
                "|   Hints: {} |\n",
                pad_right(&step.tool_hints.join(", "), 52)
            ));
        }

        // Failure action
        prompt.push_str(&format!("|   {} |\n", pad_right(&action, 56)));

        // Separator between steps
        prompt.push_str(&format!("+{}+\n", "-".repeat(box_width)));
    }

    prompt.push('\n');
    prompt
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn pad_right(s: &str, width: usize) -> String {
    let visible = s.chars().count();
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(text: &str) -> HeartbeatTask {
        HeartbeatTask {
            id: 1,
            workspace_id: "ws_1".into(),
            priority: "high".into(),
            text: text.into(),
            paused: false,
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_alarm_task_produces_investigation_steps() {
        let tasks = vec![make_task("High temperature alarm on device d1")];
        let steps = build_plan(&tasks);
        assert!(!steps.is_empty());
        assert!(steps.iter().any(|s| s.title.contains("Investigate")));
        assert!(steps.iter().any(|s| s.title.contains("Decide")));
    }

    #[test]
    fn test_device_check_produces_single_step() {
        let tasks = vec![make_task("Check device connectivity")];
        let steps = build_plan(&tasks);
        assert_eq!(steps.len(), 1);
        assert!(steps[0].title.contains("Check"));
    }

    #[test]
    fn test_daily_summary_produces_two_steps() {
        let tasks = vec![make_task("Daily summary report")];
        let steps = build_plan(&tasks);
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn test_generic_task_produces_fallback_steps() {
        let tasks = vec![make_task("Something completely new")];
        let steps = build_plan(&tasks);
        assert_eq!(steps.len(), 2);
        assert!(steps[0].title.contains("Gather"));
        assert!(steps[1].title.contains("Report"));
    }

    #[test]
    fn test_paused_tasks_are_skipped() {
        let mut task = make_task("Alarm!");
        task.paused = true;
        let steps = build_plan(&[task]);
        assert!(steps.is_empty());
    }

    #[test]
    fn test_empty_tasks() {
        let steps = build_plan(&[]);
        assert!(steps.is_empty());
    }

    #[test]
    fn test_chat_plan_single_step() {
        let steps = build_chat_plan("What's the temperature of device d1?");
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].id, "chat");
    }

    #[test]
    fn test_inject_plan_prompt_includes_required_and_optional() {
        let steps = vec![
            PlanStep {
                id: "1".into(),
                title: "Investigate".into(),
                required: true,
                max_retries: 2,
                tool_hints: vec!["query".into()],
                on_failure: FailureAction::Escalate {
                    message: "critical".into(),
                },
            },
            PlanStep {
                id: "2".into(),
                title: "Report".into(),
                required: false,
                max_retries: 0,
                tool_hints: vec![],
                on_failure: FailureAction::SkipAndContinue,
            },
        ];
        let prompt = inject_plan_prompt(&steps, "You are an AI assistant.");
        assert!(prompt.contains("[REQUIRED]"));
        assert!(prompt.contains("[OPTIONAL]"));
        assert!(prompt.contains("ESCALATE"));
        assert!(prompt.contains("Hints:"));
        assert!(prompt.contains("Step 1"));
        assert!(prompt.contains("Step 2"));
    }

    #[test]
    fn test_inject_plan_prompt_empty_steps() {
        let prompt = inject_plan_prompt(&[], "Base prompt");
        assert_eq!(prompt, "Base prompt");
    }
}
