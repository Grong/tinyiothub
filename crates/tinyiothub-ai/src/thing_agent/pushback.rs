//! Run 完成后的报告回推链 + X2 失败人工清单（T13）。
//!
//! 回推链（[`deliver`]）：
//! 1. 用户指令且带 session_key → `push_chat_message`（assistant 消息：结果摘要 + 动作清单 +
//!    verified 徽标）；
//! 2. 无会话 → admin 最近活跃会话；无活跃会话 → `notify_alert`（O28 收窄）；
//! 3. `outcome ∈ {Failed, Rejected, BudgetExceeded}` → 附加 `notify_alert`； Failed 时 payload 携带
//!    X2 人工清单（[`build_handoff_checklist`]，由 actions[] 轨迹合成——LLM 失败时 actions
//!    为空，清单同样成立）。

use super::traits::ThingAgentHost;
use super::types::{ActionResult, Outcome, RunReport, TriggerSource, WakeSignal};

/// Run 完成后把报告投递出去。host 调用失败只记录日志，不向上传播
/// （回推失败不应弄丢已落库的 run）。
pub async fn deliver(report: &RunReport, signal: &WakeSignal, host: &dyn ThingAgentHost) {
    let content = format_report_message(report);

    // ① 用户指令会话；② admin 最近活跃会话；都无 → 告警（O28）。
    let pushed = if let Some(key) = user_session_key(signal) {
        push(report, host, key, &content).await
    } else {
        match host.recent_active_admin_session(&report.workspace_id).await {
            Ok(Some(key)) => push(report, host, &key, &content).await,
            Ok(None) => false,
            Err(e) => {
                tracing::warn!(error = %e, run_id = %report.run_id, "admin session lookup failed");
                false
            }
        }
    };

    if !pushed {
        let payload = alert_payload(report, "no_active_session");
        if let Err(e) = host.notify_alert(&report.workspace_id, payload).await {
            tracing::warn!(error = %e, run_id = %report.run_id, "no-session alert failed");
        }
    }

    // ③ 失败类 outcome 附加告警；Failed 携带 X2 人工清单。
    if matches!(
        report.outcome,
        Outcome::Failed | Outcome::Rejected | Outcome::BudgetExceeded
    ) {
        let reason = format!("run_{}", report.outcome.as_str());
        let mut payload = alert_payload(report, &reason);
        if report.outcome == Outcome::Failed {
            payload["checklist"] = serde_json::Value::String(build_handoff_checklist(report));
        }
        if let Err(e) = host.notify_alert(&report.workspace_id, payload).await {
            tracing::warn!(error = %e, run_id = %report.run_id, "failure alert failed");
        }
    }
}

async fn push(report: &RunReport, host: &dyn ThingAgentHost, session_key: &str, content: &str) -> bool {
    match host.push_chat_message(session_key, content, &report.run_id).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, run_id = %report.run_id, %session_key, "push_chat_message failed");
            false
        }
    }
}

fn alert_payload(report: &RunReport, reason: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "thing_agent_run_alert",
        "reason": reason,
        "run_id": report.run_id,
        "workspace_id": report.workspace_id,
        "outcome": report.outcome.as_str(),
        "summary": report.summary,
    })
}

/// assistant 消息：结果摘要 + 动作清单 + verified 徽标；Failed 时附 X2 人工清单。
pub fn format_report_message(report: &RunReport) -> String {
    let badge = if report.verified {
        "✓ 已验证"
    } else {
        "⚠ 未验证"
    };
    let mut msg = format!("[{}] {} · {badge}", report.outcome.as_str(), report.summary);
    if !report.actions.is_empty() {
        msg.push_str("\n动作清单：");
        for a in &report.actions {
            let verified_mark = if a.verified { " ✓" } else { "" };
            msg.push_str(&format!(
                "\n- {} · {}{} → {}{verified_mark}",
                a.thing_id,
                a.action_name,
                params_suffix(&a.params),
                result_text(&a.result)
            ));
        }
    }
    if report.outcome == Outcome::Failed {
        msg.push_str("\n\n");
        msg.push_str(&build_handoff_checklist(report));
    }
    msg
}

/// X2 失败人工清单：已执行/尝试了什么、卡在哪、建议人工步骤。
/// 含 `UnknownCancelled` 动作时明示"该动作结果未知，请人工核实设备状态"。
pub fn build_handoff_checklist(report: &RunReport) -> String {
    let mut out = String::from("人工处理清单：\n【已执行/尝试】");

    if report.actions.is_empty() {
        // LLM 失败：未产出任何动作，清单仍由（空）轨迹合成。
        out.push_str("\n- 无动作记录（未执行任何设备动作，失败发生在 LLM 调用/规划阶段）");
    } else {
        for (i, a) in report.actions.iter().enumerate() {
            out.push_str(&format!(
                "\n{}. {} · {}{} → {}",
                i + 1,
                a.thing_id,
                a.action_name,
                params_suffix(&a.params),
                result_text(&a.result)
            ));
            if matches!(a.result, ActionResult::UnknownCancelled) {
                out.push_str("（该动作结果未知，请人工核实设备状态）");
            }
        }
    }

    out.push_str("\n【卡点】");
    match report
        .actions
        .iter()
        .find(|a| !matches!(a.result, ActionResult::Success(_)))
    {
        Some(a) => out.push_str(&format!(
            "{} · {} → {}",
            a.thing_id,
            a.action_name,
            result_text(&a.result)
        )),
        None if report.actions.is_empty() => out.push_str("LLM 调用/规划阶段，未执行任何动作"),
        None => out.push_str("已记录动作均成功，失败发生在动作之外（见结果摘要）"),
    }

    out.push_str("\n【建议人工步骤】");
    let mut suggestions = 0u32;
    for a in &report.actions {
        match &a.result {
            ActionResult::Failed(err) => {
                out.push_str(&format!(
                    "\n- 核实设备 {} 状态，人工重试 {}（失败原因：{err}）",
                    a.thing_id, a.action_name
                ));
                suggestions += 1;
            }
            ActionResult::UnknownCancelled => {
                out.push_str(&format!(
                    "\n- 人工核实设备 {} 的 {} 实际执行结果",
                    a.thing_id, a.action_name
                ));
                suggestions += 1;
            }
            ActionResult::Success(_) => {}
        }
    }
    if suggestions == 0 {
        out.push_str("\n- 检查 LLM 配置/额度后重试，或手动处理该问题");
    }
    out.push_str("\n- 处理完成后在 run 记录上人工确认（ack）");
    out
}

fn params_suffix(params: &serde_json::Value) -> String {
    match params {
        serde_json::Value::Null => String::new(),
        v => format!("({})", serde_json::to_string(v).unwrap_or_default()),
    }
}

fn result_text(result: &ActionResult) -> String {
    match result {
        ActionResult::Success(_) => "成功".to_string(),
        ActionResult::Failed(err) => format!("失败：{err}"),
        ActionResult::UnknownCancelled => "结果未知（已取消）".to_string(),
    }
}

/// 信号链上第一个用户指令会话（Merged 递归）。
fn user_session_key(signal: &WakeSignal) -> Option<&str> {
    match &signal.source {
        TriggerSource::UserDirective {
            session_key: Some(key), ..
        } => Some(key),
        TriggerSource::Merged { signals } => signals.iter().find_map(user_session_key),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thing_agent::traits::ThingEventSignal;
    use crate::thing_agent::types::{ActionRecord, Priority};
    use std::sync::Mutex;

    #[derive(Default)]
    struct StubHost {
        pushes: Mutex<Vec<(String, String, String)>>,
        alerts: Mutex<Vec<(String, serde_json::Value)>>,
        admin_session: Option<String>,
    }

    #[async_trait::async_trait]
    impl ThingAgentHost for StubHost {
        fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<ThingEventSignal> {
            let (tx, _) = tokio::sync::broadcast::channel(1);
            tx.subscribe()
        }

        async fn replay_events_since(&self, _cursor: i64, _min_level: i32) -> anyhow::Result<Vec<ThingEventSignal>> {
            Ok(vec![])
        }

        async fn push_chat_message(&self, session_key: &str, content: &str, run_id: &str) -> anyhow::Result<()> {
            self.pushes
                .lock()
                .unwrap()
                .push((session_key.to_string(), content.to_string(), run_id.to_string()));
            Ok(())
        }

        async fn notify_alert(&self, workspace_id: &str, payload: serde_json::Value) -> anyhow::Result<()> {
            self.alerts.lock().unwrap().push((workspace_id.to_string(), payload));
            Ok(())
        }

        async fn recent_active_admin_session(&self, _workspace_id: &str) -> anyhow::Result<Option<String>> {
            Ok(self.admin_session.clone())
        }
    }

    fn report(outcome: Outcome, verified: bool, actions: Vec<ActionRecord>) -> RunReport {
        RunReport {
            run_id: "run_1".to_string(),
            workspace_id: "ws_1".to_string(),
            trigger: "user".to_string(),
            outcome,
            summary: "调低设定值".to_string(),
            actions,
            verified,
            duration_ms: 100,
            tool_calls: 2,
            tokens: 500,
        }
    }

    fn ok_action() -> ActionRecord {
        ActionRecord {
            thing_id: "t1".to_string(),
            action_name: "set_fan".to_string(),
            params: serde_json::json!({"speed": 3}),
            result: ActionResult::Success(serde_json::json!({"ok": true})),
            verified: true,
        }
    }

    fn failed_action() -> ActionRecord {
        ActionRecord {
            thing_id: "t2".to_string(),
            action_name: "reboot".to_string(),
            params: serde_json::json!({}),
            result: ActionResult::Failed("timeout".to_string()),
            verified: false,
        }
    }

    fn unknown_action() -> ActionRecord {
        ActionRecord {
            thing_id: "t3".to_string(),
            action_name: "poll".to_string(),
            params: serde_json::Value::Null,
            result: ActionResult::UnknownCancelled,
            verified: false,
        }
    }

    fn user_signal(session_key: Option<&str>) -> WakeSignal {
        WakeSignal {
            workspace_id: "ws_1".to_string(),
            priority: Priority::Normal,
            source: TriggerSource::UserDirective {
                user_id: "u1".to_string(),
                text: "调低设定值".to_string(),
                session_key: session_key.map(str::to_string),
                source: None,
            },
            dedup_key: None,
        }
    }

    fn event_signal() -> WakeSignal {
        WakeSignal {
            workspace_id: "ws_1".to_string(),
            priority: Priority::High,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id: 7,
                level: 3,
                data: serde_json::json!({"value": 42}),
            },
            dedup_key: Some("thing:t1:event:temp_high".to_string()),
        }
    }

    #[tokio::test]
    async fn user_directive_with_session_pushes_assistant_message() {
        let host = StubHost::default();
        let r = report(Outcome::Acted, true, vec![ok_action()]);
        deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &host).await;

        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].0, "agent:ws_1:a/s1");
        assert_eq!(pushes[0].2, "run_1");
        assert!(pushes[0].1.contains("调低设定值"), "结果摘要: {}", pushes[0].1);
        assert!(pushes[0].1.contains("set_fan"), "动作清单: {}", pushes[0].1);
        assert!(pushes[0].1.contains("已验证"), "verified 徽标: {}", pushes[0].1);
        drop(pushes);
        assert!(host.alerts.lock().unwrap().is_empty(), "成功路径不告警");
    }

    #[tokio::test]
    async fn event_signal_without_session_uses_admin_session() {
        let host = StubHost {
            admin_session: Some("agent:ws_1:a/admin".to_string()),
            ..Default::default()
        };
        let r = report(Outcome::Acted, true, vec![ok_action()]);
        deliver(&r, &event_signal(), &host).await;

        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].0, "agent:ws_1:a/admin");
        drop(pushes);
        assert!(host.alerts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_active_session_falls_back_to_alert_without_panic() {
        let host = StubHost::default(); // admin_session = None
        let r = report(Outcome::NoActionNeeded, true, vec![]);
        deliver(&r, &event_signal(), &host).await;

        assert!(host.pushes.lock().unwrap().is_empty());
        let alerts = host.alerts.lock().unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].0, "ws_1");
        assert_eq!(alerts[0].1["reason"], "no_active_session");
        assert_eq!(alerts[0].1["run_id"], "run_1");
    }

    #[tokio::test]
    async fn failed_outcome_sends_additional_alert_with_checklist() {
        let host = StubHost::default();
        let r = report(
            Outcome::Failed,
            false,
            vec![ok_action(), failed_action(), unknown_action()],
        );
        deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &host).await;

        // 会话路径仍然回推
        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert!(pushes[0].1.contains("未验证"), "未验证徽标: {}", pushes[0].1);
        drop(pushes);

        // 附加失败告警，携带 X2 清单
        let alerts = host.alerts.lock().unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].1["reason"], "run_failed");
        let checklist = alerts[0].1["checklist"].as_str().expect("checklist");
        assert!(checklist.contains("set_fan"), "已执行/尝试: {checklist}");
        assert!(checklist.contains("reboot"), "卡点: {checklist}");
        assert!(
            checklist.contains("结果未知，请人工核实设备状态"),
            "UnknownCancelled 明示: {checklist}"
        );
    }

    #[tokio::test]
    async fn rejected_and_budget_exceeded_alert_without_checklist() {
        for (outcome, reason) in [
            (Outcome::Rejected, "run_rejected"),
            (Outcome::BudgetExceeded, "run_budget_exceeded"),
        ] {
            let host = StubHost::default();
            let r = report(outcome, false, vec![]);
            deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &host).await;

            let alerts = host.alerts.lock().unwrap();
            assert_eq!(alerts.len(), 1, "{outcome:?}");
            assert_eq!(alerts[0].1["reason"], reason);
            assert!(alerts[0].1.get("checklist").is_none(), "非 Failed 不带清单");
        }
    }

    #[tokio::test]
    async fn merged_signal_finds_nested_user_session() {
        let host = StubHost::default();
        let merged = WakeSignal {
            workspace_id: "ws_1".to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![event_signal(), user_signal(Some("agent:ws_1:a/s9"))],
            },
            dedup_key: Some("k".to_string()),
        };
        let r = report(Outcome::Acted, true, vec![]);
        deliver(&r, &merged, &host).await;

        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].0, "agent:ws_1:a/s9");
    }

    #[test]
    fn checklist_covers_executed_stuck_and_suggested_steps() {
        let r = report(Outcome::Failed, false, vec![ok_action(), failed_action()]);
        let checklist = build_handoff_checklist(&r);

        assert!(checklist.contains("set_fan"), "已执行/尝试了什么: {checklist}");
        assert!(checklist.contains("成功"), "成功动作结果: {checklist}");
        assert!(checklist.contains("reboot"), "卡点动作: {checklist}");
        assert!(checklist.contains("timeout"), "卡点原因: {checklist}");
        assert!(checklist.contains("建议"), "建议人工步骤: {checklist}");
    }

    #[test]
    fn checklist_marks_unknown_cancelled_explicitly() {
        let r = report(Outcome::Failed, false, vec![unknown_action()]);
        let checklist = build_handoff_checklist(&r);
        assert!(checklist.contains("poll"));
        assert!(checklist.contains("结果未知，请人工核实设备状态"), "明示: {checklist}");
    }

    #[test]
    fn checklist_handles_llm_failure_without_actions() {
        let r = report(Outcome::Failed, false, vec![]);
        let checklist = build_handoff_checklist(&r);
        assert!(
            checklist.contains("未执行") || checklist.contains("无动作"),
            "空轨迹说明: {checklist}"
        );
        assert!(checklist.contains("建议"), "仍给出人工建议: {checklist}");
    }
}
