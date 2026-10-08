//! Agent runs 领域值类型：自治 run 报告（自 db/agent_runs.rs 归位，Task 1）。
//!
//! RunReport/Outcome/ActionRecord 为 report JSON 列的序列化格式（共享写入契约）；
//! 全部 SQL 留在 db，经 Db 门面委托暴露。

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Acted,
    NoActionNeeded,
    Failed,
    BudgetExceeded,
    Rejected,
}

impl Outcome {
    /// DB/metric 字符串（snake_case，与 serde 表示一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Acted => "acted",
            Outcome::NoActionNeeded => "no_action_needed",
            Outcome::Failed => "failed",
            Outcome::BudgetExceeded => "budget_exceeded",
            Outcome::Rejected => "rejected",
        }
    }

    /// 从 DB 字符串解析；未知值 None（调用方 fail-closed）。
    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "acted" => Outcome::Acted,
            "no_action_needed" => Outcome::NoActionNeeded,
            "failed" => Outcome::Failed,
            "budget_exceeded" => Outcome::BudgetExceeded,
            "rejected" => Outcome::Rejected,
            _ => return None,
        })
    }
}

/// Why a run ended the way it did — 工单 briefing 的 failure_kind 数据源
///（工单设计 T1：timeout/llm 在 Outcome 层折叠为 Failed，必须结构化细分，
/// 禁止下游从 trigger/summary 字符串解析）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// 预算截断（工具调用数/时长）。
    Budget,
    /// 全部动作被策略拒绝。
    Policy,
    /// LLM turn 返回错误。
    Llm,
    /// turn 超时（未响应取消）。
    Timeout,
    /// 动作全部执行失败（非策略拒绝）。
    Tool,
    /// Agent 进程/LLM 供应商不可用，run 未执行即中止。
    AgentUnavailable,
}

impl EndReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            EndReason::Budget => "budget",
            EndReason::Policy => "policy",
            EndReason::Llm => "llm",
            EndReason::Timeout => "timeout",
            EndReason::Tool => "tool",
            EndReason::AgentUnavailable => "agent_unavailable",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "budget" => EndReason::Budget,
            "policy" => EndReason::Policy,
            "llm" => EndReason::Llm,
            "timeout" => EndReason::Timeout,
            "tool" => EndReason::Tool,
            "agent_unavailable" => EndReason::AgentUnavailable,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRecord {
    pub thing_id: String,
    pub action_name: String,
    pub params: serde_json::Value,
    pub result: ActionResult,
    pub verified: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionResult {
    Success(serde_json::Value),
    Failed(String),
    UnknownCancelled,
}

/// 一次工具调用轨迹（自 agent runner.rs 下放；output=None = 配对 ToolResult 未到）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolTraceEntry {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    /// `None` = no paired ToolResult (cancelled in flight / stream ended).
    pub output: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub workspace_id: String,
    pub trigger: String, // TriggerSource 的序列化
    pub outcome: Outcome,
    pub summary: String,
    pub actions: Vec<ActionRecord>,
    pub verified: bool,
    pub duration_ms: u64,
    pub tool_calls: u32,
    pub tokens: u64,
    /// 失败细分原因（Acted/NoActionNeeded 为 None）。serde(default) 向后兼容
    /// 已落库的旧 report JSON。
    #[serde(default)]
    pub end_reason: Option<EndReason>,
    /// 首个 invoke_action 的目标 thing；workspace 级 run（无动作）为 None。
    #[serde(default)]
    pub thing_id: Option<String>,
    /// 完整工具调用轨迹（到达序）。serde(default) 向后兼容已落库的旧 report JSON。
    #[serde(default)]
    pub trace: Vec<ToolTraceEntry>,
}

/// 记忆/历史段统一条目格式：`"[acted] 调低设定值成功"`。
pub fn format_summary(outcome: &str, summary: &str) -> String {
    format!("[{outcome}] {summary}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 向后兼容：已落库的旧 report JSON（无 trace/end_reason/thing_id 字段）
    /// 必须仍能反序列化，trace 默认为空。
    #[test]
    fn old_report_json_without_trace_deserializes() {
        let old = r#"{
            "run_id": "r1",
            "workspace_id": "ws1",
            "trigger": "timer:ws1",
            "outcome": "acted",
            "summary": "[acted] 调低设定值成功",
            "actions": [],
            "verified": true,
            "duration_ms": 1200,
            "tool_calls": 3,
            "tokens": 456
        }"#;
        let report: RunReport = serde_json::from_str(old).expect("旧 report JSON 必须可反序列化");
        assert!(report.trace.is_empty());
        assert_eq!(report.end_reason, None);
        assert_eq!(report.thing_id, None);
    }

    /// trace 字段的序列化往返：含未配对 output=None 的条目。
    #[test]
    fn trace_roundtrip() {
        let entry = ToolTraceEntry {
            id: "c1".into(),
            name: "read_property".into(),
            args: serde_json::json!({"p": "temperature"}),
            output: None,
        };
        let v = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(v["output"], serde_json::Value::Null);
        let back: ToolTraceEntry = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back.output, None);
    }
}
