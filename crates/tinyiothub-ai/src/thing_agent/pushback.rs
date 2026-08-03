//! Run 完成后的报告回推链 + X2 失败人工清单（T13）+ X5 策略放宽 hint（T17）。
//!
//! 回推链（[`deliver`]）：
//! 1. 用户指令且带 session_key → `push_chat_message`（assistant 消息：结果摘要 + 动作清单 +
//!    verified 徽标）；
//! 2. 无会话 → admin 最近活跃会话；无活跃会话 → `notify_alert`（O28 收窄）；
//! 3. `outcome ∈ {Failed, Rejected, BudgetExceeded}` → 附加 `notify_alert`； Failed 时 payload 携带
//!    X2 人工清单（[`build_handoff_checklist`]，由 actions[] 轨迹合成——LLM 失败时 actions
//!    为空，清单同样成立）；
//! 4. Critical 事件连续 3 次因策略被拒 → 拒绝告警 payload 附加 `policy_relax_hint` （X5
//!    hint-only）。

use super::report::AgentRunsRepository;
use super::traits::ThingAgentHost;
use super::types::{ActionResult, Outcome, Priority, RunReport, TriggerSource, WakeSignal};

/// 连续 N 次策略拒绝才触发 X5 hint。
const POLICY_DENIAL_STREAK: usize = 3;
/// 查询最近 N 条同 dedup_key run 即够用。
const POLICY_DENIAL_LOOKBACK: u32 = POLICY_DENIAL_STREAK as u32;

/// Run 完成后把报告投递出去。host 调用失败只记录日志，不向上传播
/// （回推失败不应弄丢已落库的 run）。
pub async fn deliver(
    report: &RunReport,
    signal: &WakeSignal,
    runs_repo: &dyn AgentRunsRepository,
    host: &dyn ThingAgentHost,
) {
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

    // ③ 失败类 outcome 附加告警；Failed 携带 X2 清单。
    // ④ Critical 事件连续策略被拒时附加 X5 policy_relax_hint。
    if matches!(
        report.outcome,
        Outcome::Failed | Outcome::Rejected | Outcome::BudgetExceeded
    ) {
        let reason = format!("run_{}", report.outcome.as_str());
        let mut payload = alert_payload(report, &reason);
        if report.outcome == Outcome::Failed {
            payload["checklist"] = serde_json::Value::String(build_handoff_checklist(report));
        }
        if report.outcome == Outcome::Rejected
            && let Some(hint) = policy_relax_hint(report, signal, runs_repo).await
        {
            match serde_json::to_value(&hint) {
                Ok(v) => payload["policy_relax_hint"] = v,
                Err(e) => {
                    tracing::warn!(error = %e, run_id = %report.run_id, "policy_relax_hint serialization failed");
                }
            }
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

/// X5 放宽 hint 负载。`suggested` 固定为 `"add_to_allowed"`，供 UI 预填。
#[derive(Debug, Clone, serde::Serialize)]
struct PolicyRelaxHint<'a> {
    workspace_id: &'a str,
    action_name: &'a str,
    suggested: &'a str,
}

/// Critical 事件、当前 run 因策略被拒、且同 dedup_key 最近连续 3 条 run
/// 都因策略被拒时，返回 hint；否则 None。
async fn policy_relax_hint<'a>(
    report: &'a RunReport,
    signal: &'a WakeSignal,
    runs_repo: &dyn AgentRunsRepository,
) -> Option<PolicyRelaxHint<'a>> {
    // 仅 Critical 事件触发；用户指令/定时器/普通事件无此升级。
    if signal.priority != Priority::Critical {
        return None;
    }
    let key = signal.dedup_key.as_deref()?;
    // 当前 run 自身必须是一条策略拒绝，否则不构成"连续"的一部分。
    let (action_name, _) = policy_deny_info(report)?;

    let recent = match runs_repo
        .recent_runs_by_dedup_key(&report.workspace_id, key, POLICY_DENIAL_LOOKBACK)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                error = %e,
                run_id = %report.run_id,
                dedup_key = %key,
                "recent_runs_by_dedup_key failed — skip policy_relax_hint"
            );
            return None;
        }
    };

    if !last_n_are_consecutive_policy_denials(&recent, POLICY_DENIAL_STREAK) {
        return None;
    }

    Some(PolicyRelaxHint {
        workspace_id: &report.workspace_id,
        action_name,
        suggested: "add_to_allowed",
    })
}

/// 提取 RunReport 中首个因策略被拒绝的动作：返回 (action_name, reason)。
/// 仅识别 LLM 收到 `{"denied": true, "reason": ...}` 的结构化拒绝结果。
fn policy_deny_info(report: &RunReport) -> Option<(&str, &str)> {
    for action in &report.actions {
        if let ActionResult::Success(v) = &action.result
            && v.get("denied").and_then(|v| v.as_bool()) == Some(true)
        {
            let reason = v.get("reason").and_then(|v| v.as_str()).unwrap_or("unknown");
            if is_policy_deny_reason(reason) {
                return Some((action.action_name.as_str(), reason));
            }
        }
    }
    None
}

/// 判定拒绝 reason 是否属于"策略配置类"（放宽策略可解决）。
/// `hourly_fuse` 是暂时熔断，不触发 hint；`run_action_cap`/`autonomy_not_act`
/// 等也不通过 allowlist 解决，排除。
fn is_policy_deny_reason(reason: &str) -> bool {
    matches!(reason, "action_not_allowed" | "action_denied")
}

/// 检查 runs（新→旧）前 n 条是否全部 outcome=rejected 且因策略被拒。
fn last_n_are_consecutive_policy_denials(runs: &[RunReport], n: usize) -> bool {
    if runs.len() < n {
        return false;
    }
    runs.iter()
        .take(n)
        .all(|r| r.outcome == Outcome::Rejected && policy_deny_info(r).is_some())
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
    use crate::thing_agent::report::AgentRunsRepository;
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

    #[derive(Default)]
    struct StubRunsRepo {
        reports: Mutex<Vec<RunReport>>,
    }

    #[async_trait::async_trait]
    impl AgentRunsRepository for StubRunsRepo {
        async fn insert_run(
            &self,
            _report: &RunReport,
            _problem_key: Option<&str>,
            _dedup_key: Option<&str>,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn recent_summaries(&self, _workspace_id: &str, _limit: u32) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn history_by_dedup_key(
            &self,
            _workspace_id: &str,
            _key: &str,
            _limit: u32,
        ) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }

        async fn recent_runs_by_dedup_key(
            &self,
            _workspace_id: &str,
            _key: &str,
            limit: u32,
        ) -> anyhow::Result<Vec<RunReport>> {
            Ok(self
                .reports
                .lock()
                .unwrap()
                .iter()
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn ack_run(&self, _run_id: &str, _actor: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn last_problem_run(
            &self,
            _workspace_id: &str,
            _problem_key: &str,
            _since_hours: u32,
        ) -> anyhow::Result<Option<(Outcome, bool, bool)>> {
            Ok(None)
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

    fn denied_action(action_name: &str, reason: &str) -> ActionRecord {
        ActionRecord {
            thing_id: "t1".to_string(),
            action_name: action_name.to_string(),
            params: serde_json::json!({"speed": 3}),
            result: ActionResult::Success(serde_json::json!({
                "denied": true,
                "reason": reason,
            })),
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

    fn event_signal(priority: Priority) -> WakeSignal {
        WakeSignal {
            workspace_id: "ws_1".to_string(),
            priority,
            source: TriggerSource::ThingEvent {
                thing_id: "t1".to_string(),
                event_name: "temp_high".to_string(),
                event_id: 7,
                level: if priority == Priority::Critical { 5 } else { 3 },
                data: serde_json::json!({"value": 42}),
            },
            dedup_key: Some("thing:t1:event:temp_high".to_string()),
        }
    }

    fn rejected_run(run_id: &str, action_name: &str, reason: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: "ws_1".to_string(),
            trigger: "thing:t1:event:temp_high".to_string(),
            outcome: Outcome::Rejected,
            summary: "策略拒绝".to_string(),
            actions: vec![denied_action(action_name, reason)],
            verified: false,
            duration_ms: 100,
            tool_calls: 2,
            tokens: 500,
        }
    }

    #[tokio::test]
    async fn user_directive_with_session_pushes_assistant_message() {
        let host = StubHost::default();
        let runs = StubRunsRepo::default();
        let r = report(Outcome::Acted, true, vec![ok_action()]);
        deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &runs, &host).await;

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
        let runs = StubRunsRepo::default();
        let r = report(Outcome::Acted, true, vec![ok_action()]);
        deliver(&r, &event_signal(Priority::Normal), &runs, &host).await;

        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].0, "agent:ws_1:a/admin");
        drop(pushes);
        assert!(host.alerts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn no_active_session_falls_back_to_alert_without_panic() {
        let host = StubHost::default(); // admin_session = None
        let runs = StubRunsRepo::default();
        let r = report(Outcome::NoActionNeeded, true, vec![]);
        deliver(&r, &event_signal(Priority::Normal), &runs, &host).await;

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
        let runs = StubRunsRepo::default();
        let r = report(
            Outcome::Failed,
            false,
            vec![ok_action(), failed_action(), unknown_action()],
        );
        deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &runs, &host).await;

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
            let runs = StubRunsRepo::default();
            let r = report(outcome, false, vec![]);
            deliver(&r, &user_signal(Some("agent:ws_1:a/s1")), &runs, &host).await;

            let alerts = host.alerts.lock().unwrap();
            assert_eq!(alerts.len(), 1, "{outcome:?}");
            assert_eq!(alerts[0].1["reason"], reason);
            assert!(alerts[0].1.get("checklist").is_none(), "非 Failed 不带清单");
            assert!(
                alerts[0].1.get("policy_relax_hint").is_none(),
                "无策略拒绝信息时不带 hint"
            );
        }
    }

    #[tokio::test]
    async fn merged_signal_finds_nested_user_session() {
        let host = StubHost::default();
        let runs = StubRunsRepo::default();
        let merged = WakeSignal {
            workspace_id: "ws_1".to_string(),
            priority: Priority::Normal,
            source: TriggerSource::Merged {
                signals: vec![event_signal(Priority::Normal), user_signal(Some("agent:ws_1:a/s9"))],
            },
            dedup_key: Some("k".to_string()),
        };
        let r = report(Outcome::Acted, true, vec![]);
        deliver(&r, &merged, &runs, &host).await;

        let pushes = host.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        assert_eq!(pushes[0].0, "agent:ws_1:a/s9");
    }

    // X5: 3 次连续策略拒绝的 Critical 事件触发 policy_relax_hint。
    #[tokio::test]
    async fn three_consecutive_policy_denials_trigger_relax_hint() {
        let host = StubHost::default();
        let runs = StubRunsRepo {
            reports: Mutex::new(vec![
                rejected_run("run_3", "reboot", "action_not_allowed"),
                rejected_run("run_2", "reboot", "action_not_allowed"),
                // 当前 run（第 3 次）在 alert 之前已落库，所以查询结果第一条就是当前 run。
                rejected_run("run_1", "reboot", "action_not_allowed"),
            ]),
        };
        let r = rejected_run("run_3", "reboot", "action_not_allowed");
        deliver(&r, &event_signal(Priority::Critical), &runs, &host).await;

        let alerts = host.alerts.lock().unwrap();
        // 无会话 fallback + run_rejected 告警，共 2 条。
        assert_eq!(alerts.len(), 2);
        let rejected_alert = alerts
            .iter()
            .find(|(_, p)| p["reason"] == "run_rejected")
            .expect("run_rejected alert");
        let hint = rejected_alert.1["policy_relax_hint"].as_object().expect("hint object");
        assert_eq!(hint["workspace_id"], "ws_1");
        assert_eq!(hint["action_name"], "reboot");
        assert_eq!(hint["suggested"], "add_to_allowed");
    }

    #[tokio::test]
    async fn two_consecutive_denials_do_not_trigger_relax_hint() {
        let host = StubHost::default();
        let runs = StubRunsRepo {
            reports: Mutex::new(vec![
                rejected_run("run_2", "reboot", "action_not_allowed"),
                rejected_run("run_1", "reboot", "action_not_allowed"),
            ]),
        };
        let r = rejected_run("run_2", "reboot", "action_not_allowed");
        deliver(&r, &event_signal(Priority::Critical), &runs, &host).await;

        let alerts = host.alerts.lock().unwrap();
        let rejected_alert = alerts
            .iter()
            .find(|(_, p)| p["reason"] == "run_rejected")
            .expect("run_rejected alert");
        assert!(
            rejected_alert.1.get("policy_relax_hint").is_none(),
            "2 次拒绝不触发 hint"
        );
    }

    #[tokio::test]
    async fn non_critical_event_does_not_trigger_relax_hint() {
        let host = StubHost::default();
        let runs = StubRunsRepo {
            reports: Mutex::new(vec![
                rejected_run("run_3", "reboot", "action_not_allowed"),
                rejected_run("run_2", "reboot", "action_not_allowed"),
                rejected_run("run_1", "reboot", "action_not_allowed"),
            ]),
        };
        let r = rejected_run("run_3", "reboot", "action_not_allowed");
        deliver(&r, &event_signal(Priority::Normal), &runs, &host).await;

        let alerts = host.alerts.lock().unwrap();
        let rejected_alert = alerts
            .iter()
            .find(|(_, p)| p["reason"] == "run_rejected")
            .expect("run_rejected alert");
        assert!(
            rejected_alert.1.get("policy_relax_hint").is_none(),
            "非 Critical 不触发 hint"
        );
    }

    #[tokio::test]
    async fn hourly_fuse_reason_does_not_trigger_relax_hint() {
        let host = StubHost::default();
        let runs = StubRunsRepo {
            reports: Mutex::new(vec![
                rejected_run("run_3", "reboot", "action_not_allowed"),
                rejected_run("run_2", "reboot", "action_not_allowed"),
                rejected_run("run_1", "reboot", "hourly_fuse"),
            ]),
        };
        let r = rejected_run("run_3", "reboot", "action_not_allowed");
        deliver(&r, &event_signal(Priority::Critical), &runs, &host).await;

        let alerts = host.alerts.lock().unwrap();
        let rejected_alert = alerts
            .iter()
            .find(|(_, p)| p["reason"] == "run_rejected")
            .expect("run_rejected alert");
        assert!(
            rejected_alert.1.get("policy_relax_hint").is_none(),
            "hourly_fuse 不触发 hint"
        );
    }

    #[tokio::test]
    async fn mixed_streak_breaks_relax_hint() {
        let host = StubHost::default();
        let runs = StubRunsRepo {
            reports: Mutex::new(vec![
                rejected_run("run_3", "reboot", "action_not_allowed"),
                rejected_run("run_2", "reboot", "action_not_allowed"),
                RunReport {
                    outcome: Outcome::Acted,
                    ..rejected_run("run_1", "reboot", "action_not_allowed")
                },
            ]),
        };
        let r = rejected_run("run_3", "reboot", "action_not_allowed");
        deliver(&r, &event_signal(Priority::Critical), &runs, &host).await;

        let alerts = host.alerts.lock().unwrap();
        let rejected_alert = alerts
            .iter()
            .find(|(_, p)| p["reason"] == "run_rejected")
            .expect("run_rejected alert");
        assert!(
            rejected_alert.1.get("policy_relax_hint").is_none(),
            "非连续拒绝不触发 hint"
        );
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

    #[test]
    fn policy_deny_info_extracts_first_denied_action() {
        let r = RunReport {
            actions: vec![
                ok_action(),
                denied_action("reboot", "action_not_allowed"),
                denied_action("wipe", "action_denied"),
            ],
            ..report(Outcome::Rejected, false, vec![])
        };
        let (name, reason) = policy_deny_info(&r).expect("deny info");
        assert_eq!(name, "reboot");
        assert_eq!(reason, "action_not_allowed");
    }

    #[test]
    fn is_policy_deny_reason_filters_hourly_fuse() {
        assert!(is_policy_deny_reason("action_not_allowed"));
        assert!(is_policy_deny_reason("action_denied"));
        assert!(!is_policy_deny_reason("hourly_fuse"));
        assert!(!is_policy_deny_reason("run_action_cap"));
        assert!(!is_policy_deny_reason("autonomy_not_act"));
    }
}
