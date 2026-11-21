//! Tool-call execution guard.
//!
//! This is the first piece of the harness control plane: it observes every
//! model-issued call before dispatch and stops runaway retries or unbounded
//! turns. It deliberately does not execute tools, approve effects, or own
//! capability policy. Those remain in the tool/composition layers.

use std::collections::HashMap;

use serde_json::Value;

const DEFAULT_MAX_TOOL_CALLS: usize = 128;
const REPEAT_THRESHOLDS: [usize; 3] = [3, 5, 8];

/// The guard's decision for one tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    /// Execute the call unchanged.
    Run,
    /// Do not execute; return the message to the model as tool feedback.
    Skip(String),
    /// Stop the turn with an explicit reason.
    Stop(String),
}

/// Coarse failure classes used to decide whether a retry can ever help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolFailureKind {
    /// The tool or device does not expose the requested capability.
    Unsupported,
    /// A referenced id/record does not exist.
    NotFound,
    /// The arguments or their types are invalid.
    InvalidArgs,
    /// Policy or approval rejected the call.
    Permission,
    /// The call was cancelled by a deadline.
    Timeout,
    /// Network/provider failure that may recover.
    Transient,
    /// Unclassified execution failure.
    Unknown,
}

/// Classify a tool error message without depending on a specific tool's
/// wording. This is intentionally conservative: only explicit capability
/// failures stop the retry loop.
pub fn classify_failure(message: &str) -> ToolFailureKind {
    let text = message.to_ascii_lowercase();
    if text.contains("no declaration")
        || text.contains("not declared")
        || text.contains("not supported")
        || text.contains("unsupported")
        || text.contains("unknown tool")
        || text.contains("command '") && text.contains("not found")
        || text.contains("property '") && text.contains("not found")
    {
        ToolFailureKind::Unsupported
    } else if text.contains("not found") || text.contains("未找到") || text.contains("不存在") {
        ToolFailureKind::NotFound
    } else if text.contains("permission")
        || text.contains("denied")
        || text.contains("forbidden")
        || text.contains("not allowed")
        || text.contains("approval")
    {
        ToolFailureKind::Permission
    } else if text.contains("timeout") || text.contains("timed out") {
        ToolFailureKind::Timeout
    } else if text.contains("invalid")
        || text.contains("required")
        || text.contains("validation")
        || text.contains("参数")
    {
        ToolFailureKind::InvalidArgs
    } else if text.contains("network")
        || text.contains("connect")
        || text.contains("rate limit")
        || text.contains("429")
        || text.contains("503")
    {
        ToolFailureKind::Transient
    } else {
        ToolFailureKind::Unknown
    }
}

/// Per-turn execution policy. Thresholds follow DSH's repeat-reminder tiers:
/// 3 and 5 are advisory skips, 8 is a hard stop.
#[derive(Debug, Clone)]
pub struct ToolGuardPolicy {
    pub max_tool_calls: usize,
    pub repeat_thresholds: [usize; 3],
}

impl Default for ToolGuardPolicy {
    fn default() -> Self {
        Self {
            max_tool_calls: DEFAULT_MAX_TOOL_CALLS,
            repeat_thresholds: REPEAT_THRESHOLDS,
        }
    }
}

/// Mutable per-turn guard state.
#[derive(Debug)]
pub struct ToolGuard {
    policy: ToolGuardPolicy,
    total_calls: usize,
    repeats: HashMap<String, usize>,
    failures: HashMap<ToolFailureKind, usize>,
}

impl Default for ToolGuard {
    fn default() -> Self {
        Self::new(ToolGuardPolicy::default())
    }
}

impl ToolGuard {
    pub fn new(policy: ToolGuardPolicy) -> Self {
        Self {
            policy,
            total_calls: 0,
            repeats: HashMap::new(),
            failures: HashMap::new(),
        }
    }

    /// Check a call before dispatch. Callers must invoke this exactly once for
    /// every accepted model call.
    pub fn before(&mut self, name: &str, raw_args: &str) -> GuardDecision {
        if self.total_calls >= self.policy.max_tool_calls {
            return GuardDecision::Stop(format!(
                "工具调用预算已用尽（{} 次），本轮停止。请总结已获得的证据并说明未完成项。",
                self.policy.max_tool_calls
            ));
        }
        self.total_calls += 1;

        let family = tool_family(name);
        let key = format!("{family}\u{0}{}", normalized_args(family, raw_args));
        let count = self.repeats.entry(key).or_insert(0);
        *count += 1;
        let count = *count;

        let [soft, medium, hard] = self.policy.repeat_thresholds;
        if count == hard {
            return GuardDecision::Stop(format!(
                "同一工具语义已重复调用 {count} 次，仍没有进展；本轮停止。不要再尝试同义工具名，说明阻塞原因。"
            ));
        }
        if count == medium {
            return GuardDecision::Skip(format!(
                "同一工具语义已重复调用 {count} 次。检查上一次结果；如果结果没有变化，停止换工具名重试，改为说明阻塞或调整计划。"
            ));
        }
        if count == soft {
            return GuardDecision::Skip(format!(
                "检测到同一工具语义重复调用 {count} 次。先分析上一次结果，再决定是否继续；不要用同义工具名绕过失败。"
            ));
        }
        if count > soft && count < hard {
            return GuardDecision::Skip(format!(
                "同一工具语义仍在重复（第 {count} 次），上一次调用的结果没有带来新进展。停止重试并说明阻塞。"
            ));
        }
        GuardDecision::Run
    }

    /// Observe a failed result. Capability failures are stopped after three
    /// distinct attempts; retryable/transient failures are left to the model's
    /// normal retry path.
    pub fn after(&mut self, message: &str) -> Option<GuardDecision> {
        let kind = classify_failure(message);
        if !matches!(kind, ToolFailureKind::Unsupported | ToolFailureKind::NotFound) {
            return None;
        }
        let count = self.failures.entry(kind).or_insert(0);
        *count += 1;
        if *count < 3 {
            return None;
        }
        Some(GuardDecision::Stop(format!(
            "连续 {count} 次遇到能力不存在或不支持（{kind:?}）。不要再用同义工具名重试；本轮停止并说明阻塞。"
        )))
    }

    pub fn total_calls(&self) -> usize {
        self.total_calls
    }
}

/// Collapse semantically equivalent tool names into one guard family.
fn tool_family(name: &str) -> &str {
    match name {
        "send_command" | "invoke_action" | "call_device" => "device.action",
        "read_device" | "read_property" | "read_properties" | "get_thing" | "get_thing_profile" => "device.read",
        "list_devices" | "list_things" | "search_things" => "device.list",
        "alarm_list" | "list_alarms" => "alarm.list",
        "alarm_acknowledge" | "alarm_resolve" => "alarm.action",
        "todo_write" | "update_todo" => "todo.write",
        _ => name,
    }
}

/// Normalize arguments for semantic aliases so `send_command(reconnect, dev)`,
/// `invoke_action(reconnect, dev)` and `call_device(reconnect, dev)` share one
/// repeat key.
fn normalized_args(family: &str, raw_args: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(raw_args) else {
        return raw_args.trim().to_string();
    };
    let Some(record) = value.as_object() else {
        return value.to_string();
    };
    let string = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|key| record.get(*key).and_then(Value::as_str))
            .unwrap_or_default()
            .to_string()
    };
    match family {
        "device.action" => format!(
            "{}|{}",
            string(&["thingId", "thing_id", "deviceId", "device_id"]),
            string(&[
                "action",
                "actionName",
                "action_name",
                "commandName",
                "command_name",
                "command"
            ])
        ),
        "device.read" => format!(
            "{}|{}",
            string(&["thingId", "thing_id", "deviceId", "device_id"]),
            string(&["propertyName", "property_name", "property", "properties"])
        ),
        "alarm.action" => string(&["id", "alarmId", "alarm_id"]),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_aliases_share_a_repeat_key() {
        let mut guard = ToolGuard::default();
        let first = r#"{"thingId":"dev-1","commandName":"reconnect"}"#;
        let second = r#"{"thingId":"dev-1","actionName":"reconnect"}"#;
        let third = r#"{"thingId":"dev-1","action":"reconnect"}"#;

        assert_eq!(guard.before("send_command", first), GuardDecision::Run);
        assert_eq!(guard.before("invoke_action", second), GuardDecision::Run);
        assert!(matches!(guard.before("call_device", third), GuardDecision::Skip(_)));
    }

    #[test]
    fn different_devices_are_not_a_repeat() {
        let mut guard = ToolGuard::default();
        for (name, args) in [
            ("send_command", r#"{"thingId":"dev-1","commandName":"reconnect"}"#),
            ("send_command", r#"{"thingId":"dev-2","commandName":"reconnect"}"#),
            ("send_command", r#"{"thingId":"dev-3","commandName":"reconnect"}"#),
        ] {
            assert_eq!(guard.before(name, args), GuardDecision::Run);
        }
    }

    #[test]
    fn hard_threshold_stops_the_turn() {
        let mut guard = ToolGuard::default();
        let args = r#"{"thingId":"dev-1","action":"restart"}"#;
        for _ in 0..7 {
            let _ = guard.before("call_device", args);
        }
        assert!(matches!(guard.before("call_device", args), GuardDecision::Stop(_)));
    }

    #[test]
    fn classifies_capability_and_transient_failures() {
        assert_eq!(
            classify_failure("Command 'reconnect' not found on thing"),
            ToolFailureKind::Unsupported
        );
        assert_eq!(
            classify_failure("no declaration for thing 'device-cam-02'"),
            ToolFailureKind::Unsupported
        );
        assert_eq!(classify_failure("报警未找到: alarm-005"), ToolFailureKind::NotFound);
        assert_eq!(classify_failure("HTTP 503: unavailable"), ToolFailureKind::Transient);
        assert_eq!(classify_failure("permission denied"), ToolFailureKind::Permission);
    }

    #[test]
    fn unsupported_failures_stop_after_three_attempts() {
        let mut guard = ToolGuard::default();
        assert!(guard.after("no declaration for thing 'dev-1'").is_none());
        assert!(guard.after("no declaration for thing 'dev-2'").is_none());
        assert!(matches!(
            guard.after("no declaration for thing 'dev-3'"),
            Some(GuardDecision::Stop(_))
        ));
    }

    #[test]
    fn transient_failures_do_not_trip_the_capability_stop() {
        let mut guard = ToolGuard::default();
        for _ in 0..10 {
            assert!(guard.after("HTTP 503: unavailable").is_none());
        }
    }

    #[test]
    fn tool_budget_stops_the_turn() {
        let mut guard = ToolGuard::new(ToolGuardPolicy {
            max_tool_calls: 1,
            repeat_thresholds: REPEAT_THRESHOLDS,
        });
        assert_eq!(
            guard.before("read_device", r#"{"thingId":"dev-1"}"#),
            GuardDecision::Run
        );
        assert!(matches!(
            guard.before("read_device", r#"{"thingId":"dev-2"}"#),
            GuardDecision::Stop(_)
        ));
    }
}
