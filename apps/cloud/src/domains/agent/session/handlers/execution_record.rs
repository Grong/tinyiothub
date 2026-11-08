//! RunReport→ExecutionRecord 纯函数适配层（C6：HeartbeatResult 的读侧兼容视角）。
//!
//! get_logs 主查询切换到 agent_runs（Task 10）后，每行心跳 run 经
//! [`run_row_to_execution_record`] 映射为前端契约形态（
//! web/src/ui/views/ai-ops.ts 的 `ExecutionRecord`，即 [`HeartbeatLogEntry`]）。
//! 历史无 trace 行：trace=[] → 前端显示「无过程记录」不炸。

use super::workspace_heartbeat::HeartbeatLogEntry;

/// 读侧兼容视角的类型别名——前端契约形态即现有 get_logs 响应条目，
/// 字段形状不得破（ai-ops 在消费）。
pub type ExecutionRecord = HeartbeatLogEntry;

/// agent_runs 行 → ExecutionRecord。
///
/// `run_id`/`trigger` 是签名锚定的适配契约（提案/auto_executed 明细由 handler
/// 按 tick_id=run_id 联结后填充进返回条目）；本函数只映射 run 行自身字段：
///
/// | ExecutionRecord | 来源 |
/// |---|---|
/// | timestamp | created_at |
/// | status | outcome ∈ {failed, budget_exceeded, rejected} → "error"（巡检未完成，对应旧模型 error 锚行）；{acted, no_action_needed} 及其余 → "success"（修复轮 1 裁决：词表全映射，不截断） |
/// | errorMessage / result | status=error → errorMessage=消息；否则 result=消息 |
/// | taskCount | report.task_count（Option<u32>，None/缺 → 0；修复轮 1 起由 manager 装配点/T9 迁移写入） |
/// | trace | report.trace（缺字段/解析失败 → 空数组） |
/// | autoExecuted / pendingProposals | 此处空，handler 联结明细后填充 |
///
/// 消息提取：统一内核心跳 run 的 summary 列是 LLM 自愈报告原文（可能带
/// ```json 围栏），抽其中 summary/error 字段；历史迁移行（hbm_ 前缀）的
/// summary 已是人读文本，非 JSON 时原样透出。
pub fn run_row_to_execution_record(
    run_id: &str,
    trigger: &str,
    outcome: &str,
    summary: &str,
    report_json: &serde_json::Value,
    created_at: &str,
) -> ExecutionRecord {
    // 签名锚定 C6 契约；当前映射不消费 run_id/trigger（明细联结在 handler 侧）。
    let _ = (run_id, trigger);
    let message = extract_message(summary);
    let status = match outcome {
        // 修复轮 1 裁决：这三个 outcome 都=巡检未完成，对应旧模型 error 锚行；
        // 截断到 success 会让前端绿点误显「完成」。
        "failed" | "budget_exceeded" | "rejected" => "error",
        _ => "success",
    };
    HeartbeatLogEntry {
        timestamp: created_at.to_string(),
        task_count: report_json.get("task_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        status: status.to_string(),
        error_message: if status == "error" { message.clone() } else { None },
        result: if status == "success" { message } else { None },
        auto_executed: Vec::new(),
        pending_proposals: Vec::new(),
        trace: report_json
            .get("trace")
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default(),
    }
}

/// 从 run summary 列提取人读消息：自愈报告 JSON（可带围栏/前后散文）→
/// summary 或 error 字段；非 JSON（历史行人读文本）原样；空串 → None。
fn extract_message(summary: &str) -> Option<String> {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(v) = parse_embedded_json(trimmed) {
        if let Some(s) = v.get("summary").and_then(|s| s.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
        if let Some(s) = v.get("error").and_then(|s| s.as_str()).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    Some(summary.to_string())
}

/// 解析可能嵌在散文/围栏里的 JSON 对象：先整串试，失败再取首个 '{' 到
/// 末个 '}' 的切片。与 report.rs extract_json 同思路，此处只需 Value。
fn parse_embedded_json(text: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三态之一——有 trace：统一内核 run（summary=带围栏自愈报告）映射为
    /// success 条目，trace 完整带出，summary 字段被抽取为 result。
    #[test]
    fn maps_run_with_trace_and_fenced_report_summary() {
        let report = serde_json::json!({
            "run_id": "run_1",
            "trace": [{"id": "c1", "name": "read_property", "args": {"p": "temp"}, "output": "26.5"}],
        });
        let summary = "巡检结论：\n```json\n{\"summary\": \"温度偏高已处理\", \"proposals\": []}\n```";

        let rec = run_row_to_execution_record(
            "run_1",
            "heartbeat:ws_1",
            "acted",
            summary,
            &report,
            "2026-10-13 01:00:00",
        );

        assert_eq!(rec.timestamp, "2026-10-13 01:00:00");
        assert_eq!(rec.status, "success");
        assert_eq!(rec.result.as_deref(), Some("温度偏高已处理"));
        assert!(rec.error_message.is_none());
        assert_eq!(rec.trace.len(), 1);
        assert_eq!(rec.trace[0].name, "read_property");
        assert_eq!(rec.trace[0].output.as_deref(), Some("26.5"));
        // 明细由 handler 联结填充，适配器输出为空
        assert!(rec.auto_executed.is_empty() && rec.pending_proposals.is_empty());
    }

    /// 三态之二——无 trace：历史迁移行（hbm_）summary 已是人读文本、report
    /// 缺 trace 字段 → 原样透出 + trace=[]（前端「无过程记录」不炸）。
    #[test]
    fn maps_history_row_without_trace_to_empty_trace() {
        let report = serde_json::json!({"run_id": "hbm_t1", "outcome": "no_action_needed"});

        let rec = run_row_to_execution_record(
            "hbm_t1",
            "heartbeat:ws_1",
            "no_action_needed",
            "checked 2 things",
            &report,
            "2026-07-20 10:00:00",
        );

        assert_eq!(rec.status, "success");
        assert_eq!(rec.result.as_deref(), Some("checked 2 things"));
        assert!(rec.trace.is_empty(), "历史无 trace 行 → 空数组");
    }

    /// 三态之三——error：outcome=failed → status=error + errorMessage，
    /// result 为空；自愈报告 error 字段被抽取。
    #[test]
    fn maps_failed_run_to_error_status() {
        let report = serde_json::json!({"trace": []});

        let rec = run_row_to_execution_record(
            "run_e",
            "heartbeat:ws_1",
            "failed",
            "{\"error\": \"LLM 超时\", \"summary\": \"\"}",
            &report,
            "t",
        );

        assert_eq!(rec.status, "error");
        assert_eq!(rec.error_message.as_deref(), Some("LLM 超时"));
        assert!(rec.result.is_none());
    }

    /// 边界：空 summary → 消息 None（不伪造）；非 JSON 散文含花括号但不合法
    /// → 原样透出。
    #[test]
    fn extract_message_edges() {
        assert_eq!(extract_message(""), None);
        assert_eq!(extract_message("   "), None);
        assert_eq!(
            extract_message("checked {temp} ok").as_deref(),
            Some("checked {temp} ok")
        );
        // 无围栏纯 JSON 也可抽
        assert_eq!(
            extract_message("{\"summary\": \"一切正常\"}").as_deref(),
            Some("一切正常")
        );
    }

    /// 修复轮 1 裁决：status 词表全映射——failed/budget_exceeded/rejected
    /// 都=巡检未完成 → error（截断到 success 会让前端绿点误显「完成」）；
    /// acted/no_action_needed → success。
    #[test]
    fn status_maps_full_outcome_vocabulary() {
        let report = serde_json::json!({});
        for outcome in ["failed", "budget_exceeded", "rejected"] {
            let rec = run_row_to_execution_record("r", "heartbeat:ws", outcome, "出问题", &report, "t");
            assert_eq!(rec.status, "error", "{outcome} 必须映 error");
            assert_eq!(
                rec.error_message.as_deref(),
                Some("出问题"),
                "{outcome} 消息进 errorMessage"
            );
            assert!(rec.result.is_none());
        }
        for outcome in ["acted", "no_action_needed"] {
            let rec = run_row_to_execution_record("r", "heartbeat:ws", outcome, "正常", &report, "t");
            assert_eq!(rec.status, "success", "{outcome} 必须映 success");
            assert_eq!(rec.result.as_deref(), Some("正常"));
            assert!(rec.error_message.is_none());
        }
    }

    /// 修复轮 1：taskCount 透出——report.task_count（Some）→ taskCount；
    /// None/缺字段 → 0（与现状显示一致）。
    #[test]
    fn task_count_passes_through_from_report() {
        let with = serde_json::json!({"task_count": 3});
        let rec = run_row_to_execution_record("r", "heartbeat:ws", "acted", "s", &with, "t");
        assert_eq!(rec.task_count, 3);

        // serde(default) 后兼容：旧 report JSON 无 task_count 键
        let without = serde_json::json!({"trace": []});
        let rec = run_row_to_execution_record("r", "heartbeat:ws", "acted", "s", &without, "t");
        assert_eq!(rec.task_count, 0);

        // 迁移行 content 缺 taskCount → json_extract NULL → null → 0
        let null = serde_json::json!({"task_count": null});
        let rec = run_row_to_execution_record("r", "heartbeat:ws", "acted", "s", &null, "t");
        assert_eq!(rec.task_count, 0);
    }
}
