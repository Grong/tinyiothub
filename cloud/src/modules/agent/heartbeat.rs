// Heartbeat module — per-workspace AI autonomous inspection
//
// Contains:
//   - HEARTBEAT.md task parsing (read/write/build/parse)
//   - heartbeat_loop — per-workspace async loop driving periodic AI inspection
//   - build_prompt — assembles prompt from tasks + recent actions + wake signals

use std::sync::Arc;

use dashmap::DashMap;

use super::agent::AgentPool;

/// A single heartbeat task
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTask {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Self-healing: an action the AI auto-executed during heartbeat
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AutoExecutedAction {
    pub tool: String,
    pub device_id: String,
    pub summary: String,
}

/// Self-healing: a proposal the AI wants human approval for
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingProposal {
    pub level: String,
    pub tool_name: String,
    pub device_id: String,
    pub device_name: String,
    pub tool_params: serde_json::Value,
    pub summary: String,
    pub reason: String,
    pub risk: String,
}

/// Parsed self-healing report from the AI's heartbeat response
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealingReport {
    pub auto_executed: Vec<AutoExecutedAction>,
    pub pending_proposals: Vec<PendingProposal>,
}

/// Read tasks from HEARTBEAT.md
pub async fn read_heartbeat_tasks(
    workspace_dir: &std::path::Path,
) -> anyhow::Result<Vec<HeartbeatTask>> {
    let path = workspace_dir.join("HEARTBEAT.md");
    if !path.exists() {
        return Ok(get_default_tasks());
    }
    let content = tokio::fs::read_to_string(&path).await?;
    Ok(parse_heartbeat_md(&content))
}

/// Write tasks to HEARTBEAT.md
///
/// NOTE: File-based writes have a known TOCTOU race — concurrent edits from
/// multiple admins may overwrite each other. Long-term fix (P2): migrate
/// tasks to a DB table with optimistic locking.
pub async fn write_heartbeat_tasks(
    workspace_dir: &std::path::Path,
    tasks: &[HeartbeatTask],
) -> anyhow::Result<()> {
    let content = build_heartbeat_md(tasks);
    tokio::fs::write(workspace_dir.join("HEARTBEAT.md"), content).await?;
    Ok(())
}

pub(crate) fn get_default_tasks() -> Vec<HeartbeatTask> {
    vec![
        HeartbeatTask {
            priority: "high".into(),
            text: "检查离线设备并尝试自动重连".into(),
            paused: false,
        },
        HeartbeatTask {
            priority: "medium".into(),
            text: "扫描未处理的高优先级告警".into(),
            paused: false,
        },
        HeartbeatTask {
            priority: "medium".into(),
            text: "生成设备状态日报摘要".into(),
            paused: false,
        },
        HeartbeatTask {
            priority: "low".into(),
            text: "检查系统磁盘和内存使用率".into(),
            paused: true,
        },
    ]
}

fn parse_heartbeat_md(content: &str) -> Vec<HeartbeatTask> {
    let mut tasks = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('-') || line.starts_with("#") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("- [") {
            let rest = rest.trim_end_matches(']').trim();
            let (priority_part, text) = rest.split_once(']').unwrap_or((rest, ""));
            let text = text.trim();
            let (priority, paused) = if priority_part.contains("|paused") {
                let p = priority_part.split('|').next().unwrap_or("low");
                (p.to_string(), true)
            } else {
                (priority_part.to_string(), false)
            };
            if !text.is_empty() {
                tasks.push(HeartbeatTask { priority, text: text.to_string(), paused });
            }
        } else if let Some(text) = line.strip_prefix("- ") {
            let text = text.trim();
            if !text.is_empty() {
                tasks.push(HeartbeatTask {
                    priority: "low".into(),
                    text: text.to_string(),
                    paused: false,
                });
            }
        }
    }
    tasks
}

pub(crate) fn build_heartbeat_md(tasks: &[HeartbeatTask]) -> String {
    let mut s = "# Periodic Tasks\n".to_string();
    for task in tasks {
        let flag =
            if task.paused { format!("{}|paused", task.priority) } else { task.priority.clone() };
        s.push_str(&format!("- [{}] {}\n", flag, task.text));
    }
    s
}

// ── Per-workspace heartbeat loop (v0.5) ──

use super::{
    action_repo::{ActionType, AgentAction, AgentActionRepository, EventType},
    heartbeat_manager::{HeartbeatConfig, WakeSignal},
};

/// Per-workspace heartbeat loop — drives periodic AI inspection for a single workspace
///
/// TODO(T11): Inject `Arc<SseConnectionManager>` to broadcast action events to the
///   AI 运维中心 page via SSE. Hook point: after each `action_repo.insert()` call.
/// TODO(T12): Inject `Arc<dyn MemoryStore>` to trigger `reflect_conversation_turn()`
///   after heartbeat actions complete, so AI learns from autonomous operations.
pub(crate) async fn heartbeat_loop(
    workspace_id: String,
    config: HeartbeatConfig,
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    heartbeat_file: std::path::PathBuf,
    mut wake_rx: tokio::sync::mpsc::Receiver<WakeSignal>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    last_ticks: Arc<DashMap<String, tokio::time::Instant>>,
) {
    let interval_secs = (config.interval_minutes as u64).saturating_mul(60);
    let interval_dur = tokio::time::Duration::from_secs(interval_secs);
    let mut shutdown_rx = shutdown_rx;
    let mut consecutive_failures: u32 = 0;

    tracing::info!(%workspace_id, interval_minutes=config.interval_minutes, "Heartbeat loop started");

    // Use sleep (one-shot) instead of interval (repeating) so that wake-signal
    // executions also reset the timer — preventing alarm storms from
    // triggering heartbeat more often than the configured interval.
    loop {
        let sleep = tokio::time::sleep(interval_dur);
        tokio::pin!(sleep);

        let wake_signals: Vec<WakeSignal> = tokio::select! {
            _ = &mut sleep => {
                drain_channel(&mut wake_rx)
            }
            signal = wake_rx.recv() => {
                let mut signals = vec![];
                if let Some(s) = signal {
                    signals.push(s);
                }
                signals.append(&mut drain_channel(&mut wake_rx));
                signals
            }
            _ = &mut shutdown_rx => {
                tracing::info!(%workspace_id, "Heartbeat loop received shutdown signal");
                break;
            }
        };

        let triggered_by_wake = !wake_signals.is_empty();
        let wake_signals = dedup_and_cap(wake_signals, 5);

        if triggered_by_wake {
            tracing::info!(
                %workspace_id,
                signal_count = wake_signals.len(),
                reasons = %wake_signals.iter().map(|s| s.reason.as_str()).collect::<Vec<_>>().join(", "),
                "Heartbeat tick — triggered by wake signal",
            );
        }

        // Rate-limit guard: if too soon since last tick, sleep for the remaining gap.
        // Uses sleep (not continue) to avoid cascading timer resets that can occur
        // when wake signals arrive right before the sleep would complete.
        // MUST be before any I/O (DB queries, file reads) to avoid wasted work.
        if let Some(entry) = last_ticks.get(&workspace_id) {
            let elapsed = entry.elapsed();
            if elapsed < interval_dur {
                let remaining = interval_dur - elapsed;
                tracing::info!(
                    %workspace_id,
                    remaining_secs = remaining.as_secs(),
                    "Heartbeat tick — rate-limited, sleeping for remaining gap",
                );
                tokio::time::sleep(remaining).await;
            }
        }
        last_ticks.insert(workspace_id.clone(), tokio::time::Instant::now());

        let workspace_dir = heartbeat_file.parent().unwrap_or(std::path::Path::new("."));
        let tasks = read_heartbeat_tasks(workspace_dir).await.unwrap_or_else(|e| {
            tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
            get_default_tasks()
        });

        let recent_actions = action_repo
            .find_recent_by_workspace(
                &workspace_id,
                &[EventType::Heartbeat, EventType::Alarm],
                config.max_recent_actions as u32,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(%workspace_id, "Failed to query recent actions: {}", e);
                vec![]
            });

        // Query approved proposals from previous ticks
        let approved_proposals = action_repo
            .find_recent_by_workspace(&workspace_id, &[EventType::Heartbeat], 20)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(%workspace_id, "Failed to query proposals: {}", e);
                vec![]
            })
            .into_iter()
            .filter(|a| {
                a.action_type == ActionType::Proposal
                    && serde_json::from_str::<serde_json::Value>(&a.content)
                        .map(|c| c.get("status").and_then(|s| s.as_str()) == Some("approved"))
                        .unwrap_or(false)
            })
            .collect::<Vec<_>>();

        let prompt = build_prompt(
            &workspace_id,
            &tasks,
            &recent_actions,
            &wake_signals,
            &approved_proposals,
        );

        let task_count = tasks.len();
        tracing::info!(
            %workspace_id,
            task_count,
            backoff = consecutive_failures,
            "Heartbeat tick — {} tasks to inspect",
            task_count,
        );

        // Exponential backoff: delay before LLM call when failures accumulate.
        // Formula: min(2^failures * 30s, 15min). Caps at 10 failures.
        if consecutive_failures > 0 {
            let backoff_secs = (30u64.saturating_mul(2u64.saturating_pow(consecutive_failures - 1)))
                .min(900);
            tracing::warn!(
                %workspace_id,
                consecutive_failures,
                backoff_secs,
                "Heartbeat backoff due to prior failures",
            );
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
        }

        match agent_pool.run_streaming(&workspace_id, &prompt).await {
            Ok(run_result) => {
                consecutive_failures = 0;

                let response = run_result.final_text;

                // Parse self-healing report from AI response
                let healing_report = parse_healing_report(&response);

                // Record auto-executed actions
                if let Some(ref report) = healing_report {
                    for action in &report.auto_executed {
                        let content = serde_json::json!({
                            "type": "auto_executed",
                            "tool": action.tool,
                            "deviceId": action.device_id,
                            "summary": action.summary,
                        })
                        .to_string();
                        let record = AgentAction::new(
                            workspace_id.clone(),
                            "default".to_string(),
                            None,
                            Some(action.device_id.clone()),
                            EventType::Heartbeat,
                            ActionType::AutoExecuted,
                            content,
                        );
                        if let Err(e) = action_repo.insert(&record).await {
                            tracing::error!(%workspace_id, "Failed to record auto action: {}", e);
                        }
                    }

                    // Save pending proposals
                    for proposal in &report.pending_proposals {
                        let proposal_id = uuid::Uuid::new_v4().to_string();
                        let content = serde_json::json!({
                            "type": "proposal",
                            "proposalId": proposal_id,
                            "status": "pending",
                            "level": proposal.level,
                            "toolName": proposal.tool_name,
                            "deviceId": proposal.device_id,
                            "deviceName": proposal.device_name,
                            "toolParams": proposal.tool_params,
                            "summary": proposal.summary,
                            "reason": proposal.reason,
                            "risk": proposal.risk,
                        })
                        .to_string();
                        let record = AgentAction::new(
                            workspace_id.clone(),
                            "default".to_string(),
                            None,
                            Some(proposal.device_id.clone()),
                            EventType::Heartbeat,
                            ActionType::Proposal,
                            content,
                        );
                        if let Err(e) = action_repo.insert(&record).await {
                            tracing::error!(%workspace_id, "Failed to save proposal: {}", e);
                        }
                    }

                    // Mark approved proposals as executed if the AI reported them.
                    // Match by (device_id, tool_name) composite key instead of
                    // fragile substring comparison on summary fields.
                    for approved in &approved_proposals {
                        if let Ok(parsed) =
                            serde_json::from_str::<serde_json::Value>(&approved.content)
                        {
                            let proposal_device = parsed
                                .get("deviceId")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let proposal_tool = parsed
                                .get("toolName")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let was_executed = healing_report
                                .as_ref()
                                .map(|r| {
                                    r.auto_executed.iter().any(|a| {
                                        a.device_id == proposal_device && a.tool == proposal_tool
                                    })
                                })
                                .unwrap_or(false);
                            if was_executed {
                                if let Ok(mut content) =
                                    serde_json::from_str::<serde_json::Value>(&approved.content)
                                {
                                    content["status"] =
                                        serde_json::Value::String("executed".into());
                                    let _ = action_repo
                                        .update_content(&approved.id, &content.to_string())
                                        .await;
                                }
                            }
                        }
                    }
                }

                // Summary record (always saved for backward compatibility)
                let summary_content = serde_json::json!({
                    "taskCount": task_count,
                    "autoExecutedCount": healing_report.as_ref().map(|r| r.auto_executed.len()).unwrap_or(0),
                    "proposalCount": healing_report.as_ref().map(|r| r.pending_proposals.len()).unwrap_or(0),
                    "result": truncate(&response, 5000),
                })
                .to_string();
                let action = AgentAction::new(
                    workspace_id.clone(),
                    "default".to_string(),
                    None,
                    None,
                    EventType::Heartbeat,
                    ActionType::Summary,
                    summary_content,
                );
                if let Err(e) = action_repo.insert(&action).await {
                    tracing::error!(%workspace_id, "Failed to record heartbeat action: {}", e);
                }
                tracing::debug!(%workspace_id, "Heartbeat tick completed");
            }
            Err(e) => {
                consecutive_failures = (consecutive_failures + 1).min(10);
                let content = serde_json::json!({
                    "taskCount": task_count,
                    "error": truncate(&e.to_string(), 5000),
                })
                .to_string();
                let action = AgentAction::new(
                    workspace_id.clone(),
                    "default".to_string(),
                    None,
                    None,
                    EventType::Heartbeat,
                    ActionType::Error,
                    content,
                );
                if let Err(e2) = action_repo.insert(&action).await {
                    tracing::error!(%workspace_id, "Failed to record heartbeat error: {}", e2);
                }
                tracing::warn!(%workspace_id, failures=consecutive_failures, "Heartbeat LLM error: {}", e);
            }
        }
    }

    tracing::info!(%workspace_id, "Heartbeat loop exited");
}

fn drain_channel(rx: &mut tokio::sync::mpsc::Receiver<WakeSignal>) -> Vec<WakeSignal> {
    let mut signals = vec![];
    while let Ok(s) = rx.try_recv() {
        signals.push(s);
    }
    signals
}

fn dedup_and_cap(mut signals: Vec<WakeSignal>, cap: usize) -> Vec<WakeSignal> {
    // Sort by priority (Critical > High > Normal) so high-priority signals survive truncation
    signals.sort_by_key(|s| std::cmp::Reverse(s.priority));
    // Dedup by (device_id, alarm_type) — signals without dedup keys are never deduped
    let mut seen = std::collections::HashSet::new();
    signals.retain(|s| {
        let key = (s.device_id.clone(), s.alarm_type.clone());
        // Signals without both dedup keys: always keep (can't dedup)
        if key.0.is_none() || key.1.is_none() {
            return true;
        }
        seen.insert(key)
    });
    signals.truncate(cap);
    signals
}

fn build_prompt(
    workspace_id: &str,
    tasks: &[HeartbeatTask],
    recent_actions: &[AgentAction],
    wake_signals: &[WakeSignal],
    approved_proposals: &[AgentAction],
) -> String {
    let mut p = format!(
        "你是一个 IoT 平台的 AI 运维助手，负责工作空间 `{}` 的自动巡检。\n\n\
         你有以下工具可以直接调用：\n\
         - search_devices: 搜索设备\n\
         - get_device: 获取设备详情\n\
         - read_properties: 读取设备属性\n\
         - write_properties: 修改设备属性值\n\
         - send_command: 向设备发送命令（如重启、重连、配置切换）\n\
         - alarm_list / alarm_acknowledge: 查询和确认告警\n\n",
        workspace_id
    );

    // ── L0-L3 severity guidance ──
    p.push_str("## 自愈操作分级\n\n");
    p.push_str("- L0 (自动执行): 纯查询操作（搜索设备、读取属性、查询告警）\n");
    p.push_str("- L1 (自动执行): 低风险写操作（确认告警、向离线设备发送重连命令、调整非关键属性如上报间隔）。每设备每次巡检不超过 3 个写操作。\n");
    p.push_str("- L2 (需审批): 中等风险操作（修改设备阈值参数、批量写入属性、启停设备任务）—— 不直接执行，在 pending_proposals 中提出建议。\n");
    p.push_str(
        "- L3 (已禁用): 高风险操作（delete_device、delete_schedule）已被系统禁用，不要提议。\n\n",
    );

    // ── Safety constraints ──
    p.push_str("## 安全约束\n\n");
    p.push_str("- write_properties 前必须先用 read_properties 确认当前值\n");
    p.push_str("- 不要修改标记为只读的属性\n");
    p.push_str("- 不要向正在执行关键任务的设备发送可能中断运行的命令\n");
    p.push_str("- L1 操作直接调用工具执行并记录到 auto_executed；L2 操作不执行，写入 pending_proposals\n\n");

    // ── Approved proposals from previous ticks ──
    if !approved_proposals.is_empty() {
        p.push_str("## 已批准的待执行操作\n\n");
        for proposal in approved_proposals {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&proposal.content) {
                let summary = parsed.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let tool = parsed.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
                let device = parsed.get("device_name").and_then(|v| v.as_str()).unwrap_or("");
                let params = parsed.get("tool_params").map(|v| v.to_string()).unwrap_or_default();
                p.push_str(&format!(
                    "- [已批准] 用 {} 对设备 {} 执行: {} (参数: {})\n",
                    tool,
                    device,
                    summary,
                    truncate(&params, 200),
                ));
            }
        }
        p.push_str("\n以上操作已经人工批准，请直接调用工具执行（作为 L1），并在 auto_executed 中记录。\n\n");
    }

    let active_tasks: Vec<&HeartbeatTask> = tasks.iter().filter(|t| !t.paused).collect();
    if !active_tasks.is_empty() {
        p.push_str("## 巡检任务\n\n");
        for task in &active_tasks {
            p.push_str(&format!("- [{}] {}\n", task.priority, task.text));
        }
        p.push('\n');
    }

    if !wake_signals.is_empty() {
        p.push_str("## 实时事件\n\n");
        for sig in wake_signals {
            p.push_str(&format!(
                "- [{}] {}: {}\n",
                sig.priority_label(),
                sig.reason,
                truncate(&sig.context, 500),
            ));
        }
        p.push('\n');
    }

    if !recent_actions.is_empty() {
        p.push_str("## 最近AI操作记录\n\n");
        for action in recent_actions.iter().take(5) {
            p.push_str(&format!(
                "- [{}] {}: {}\n",
                &action.created_at,
                action.action_type.as_str(),
                truncate(&action.content, 200),
            ));
        }
        p.push('\n');
    }

    // ── Output format requirement ──
    p.push_str("## 输出格式\n\n");
    p.push_str("巡检结束后，必须以 JSON 代码块输出执行摘要：\n\n");
    p.push_str("```json\n");
    p.push_str("{\n");
    p.push_str("  \"auto_executed\": [\n");
    p.push_str("    {\"tool\": \"send_command\", \"device_id\": \"dev-001\", \"summary\": \"已重连离线设备\"}\n");
    p.push_str("  ],\n");
    p.push_str("  \"pending_proposals\": [\n");
    p.push_str("    {\"level\": \"L2\", \"tool_name\": \"write_properties\", \"device_id\": \"dev-002\", \"device_name\": \"温度传感器A\", \"tool_params\": {\"device_id\": \"dev-002\", \"properties\": {\"report_interval\": 60}}, \"summary\": \"调整上报间隔为60秒\", \"reason\": \"当前间隔过短导致带宽浪费\", \"risk\": \"低\"}\n");
    p.push_str("  ]\n");
    p.push_str("}\n");
    p.push_str("```\n");
    p.push_str("没有对应内容时写空数组 []。");
    p
}

/// Extract a HealingReport from the AI's text response.
/// Looks for a ```json ... ``` code block, then falls back to scanning for the JSON object.
fn parse_healing_report(response: &str) -> Option<HealingReport> {
    // Try extracting from ```json code block first
    if let Some(json_str) = extract_json_block(response) {
        if let Ok(report) = serde_json::from_str::<HealingReport>(&json_str) {
            return Some(report);
        }
    }
    // Fallback: scan for { "auto_executed": ... } pattern
    if let Some(start) = response.find(r#""auto_executed""#) {
        // Backtrack to find the opening brace
        let before = &response[..start];
        if let Some(brace) = before.rfind('{') {
            let slice = &response[brace..];
            if let Some(end) = slice.find("}\n") {
                let json_str = &slice[..=end];
                if let Ok(report) = serde_json::from_str::<HealingReport>(json_str) {
                    return Some(report);
                }
            }
        }
    }
    None
}

fn extract_json_block(text: &str) -> Option<String> {
    let start = text.find("```json")?;
    let after_fence = &text[start + 7..];
    let end = after_fence.find("```")?;
    Some(after_fence[..end].trim().to_string())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max_len);
        format!("{}…(+{} 字省略)", &s[..end], s.len() - end)
    }
}

#[cfg(test)]
mod tests {
    use super::{super::heartbeat_manager::WakePriority, *};

    fn make_signal(reason: &str, priority: WakePriority) -> WakeSignal {
        WakeSignal {
            workspace_id: "ws-1".into(),
            reason: reason.into(),
            context: "test".into(),
            priority,
            device_id: None,
            alarm_type: None,
            rule_id: None,
        }
    }

    fn make_signal_dedup(
        reason: &str,
        priority: WakePriority,
        device_id: &str,
        alarm_type: &str,
    ) -> WakeSignal {
        WakeSignal {
            workspace_id: "ws-1".into(),
            reason: reason.into(),
            context: "test".into(),
            priority,
            device_id: Some(device_id.into()),
            alarm_type: Some(alarm_type.into()),
            rule_id: Some("rule-1".into()),
        }
    }

    fn make_action(workspace_id: &str, action_type: ActionType, content: &str) -> AgentAction {
        AgentAction::new(
            workspace_id.into(),
            "default".into(),
            None,
            None,
            EventType::Heartbeat,
            action_type,
            content.into(),
        )
    }

    #[test]
    fn test_dedup_and_cap_priority_sort() {
        // Critical (2) should survive truncation over High (1) and Normal (0)
        let signals = vec![
            make_signal("normal", WakePriority::Normal),
            make_signal("critical", WakePriority::Critical),
            make_signal("high", WakePriority::High),
        ];
        let result = dedup_and_cap(signals, 2);
        assert_eq!(result.len(), 2);
        // First should be Critical (highest priority due to Reverse sort)
        assert!(matches!(result[0].priority, WakePriority::Critical));
        assert!(matches!(result[1].priority, WakePriority::High));
    }

    #[test]
    fn test_dedup_and_cap_dedup_by_device_and_type() {
        // Same device + same alarm_type → dedup, keep highest priority (Critical)
        let signals = vec![
            make_signal_dedup("alarm:1", WakePriority::High, "dev-01", "DeviceOffline"),
            make_signal_dedup("alarm:2", WakePriority::Critical, "dev-01", "DeviceOffline"),
        ];
        let result = dedup_and_cap(signals, 5);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].priority, WakePriority::Critical));
    }

    #[test]
    fn test_dedup_and_cap_different_devices_kept() {
        // Different devices → both kept
        let signals = vec![
            make_signal_dedup("alarm:1", WakePriority::High, "dev-01", "DeviceOffline"),
            make_signal_dedup("alarm:2", WakePriority::High, "dev-02", "DeviceOffline"),
        ];
        let result = dedup_and_cap(signals, 5);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_and_cap_no_dedup_key_always_kept() {
        // Signals without dedup keys are never deduped
        let signals = vec![
            make_signal("alarm:1", WakePriority::Critical),
            make_signal("alarm:1", WakePriority::High),
        ];
        let result = dedup_and_cap(signals, 5);
        // Without dedup keys, both are kept (they can't be deduped)
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_and_cap_truncate() {
        let signals: Vec<WakeSignal> =
            (0..10).map(|i| make_signal(&format!("sig-{}", i), WakePriority::Normal)).collect();
        let result = dedup_and_cap(signals, 3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_dedup_and_cap_empty() {
        let result = dedup_and_cap(vec![], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_heartbeat_md_basic() {
        let content = "- [high] 检查离线设备\n- [medium] 扫描告警";
        let tasks = parse_heartbeat_md(content);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].priority, "high");
        assert_eq!(tasks[0].text, "检查离线设备");
        assert!(!tasks[0].paused);
        assert_eq!(tasks[1].priority, "medium");
        assert_eq!(tasks[1].text, "扫描告警");
    }

    #[test]
    fn test_parse_heartbeat_md_paused() {
        let content = "- [high|paused] 检查离线设备";
        let tasks = parse_heartbeat_md(content);
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].paused);
        assert_eq!(tasks[0].priority, "high");
    }

    #[test]
    fn test_parse_heartbeat_md_simple() {
        let content = "- 做一个简单的任务";
        let tasks = parse_heartbeat_md(content);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].priority, "low");
        assert!(!tasks[0].paused);
    }

    #[test]
    fn test_parse_heartbeat_md_skips_headers() {
        let content = "# 标题\n- [high] 一个任务";
        let tasks = parse_heartbeat_md(content);
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_parse_heartbeat_md_empty() {
        let tasks = parse_heartbeat_md("");
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_get_default_tasks() {
        let tasks = get_default_tasks();
        assert!(!tasks.is_empty());
        assert!(tasks.iter().any(|t| !t.paused));
        assert!(tasks.iter().any(|t| t.paused));
    }

    #[test]
    fn test_build_heartbeat_md_roundtrip() {
        let tasks = vec![
            HeartbeatTask {
                priority: "high".into(), text: "检查离线设备".into(), paused: false
            },
            HeartbeatTask { priority: "low".into(), text: "生成报表".into(), paused: true },
        ];
        let md = build_heartbeat_md(&tasks);
        let parsed = parse_heartbeat_md(&md);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].priority, "high");
        assert_eq!(parsed[0].text, "检查离线设备");
        assert!(parsed[1].paused);
    }

    #[test]
    fn test_build_prompt_includes_tasks() {
        let tasks = vec![HeartbeatTask {
            priority: "high".into(),
            text: "检查离线设备".into(),
            paused: false,
        }];
        let prompt = build_prompt("ws-1", &tasks, &[], &[], &[]);
        assert!(prompt.contains("巡检任务"));
        assert!(prompt.contains("检查离线设备"));
        assert!(prompt.contains("ws-1"));
    }

    #[test]
    fn test_build_prompt_includes_wake_signals() {
        let signals = vec![make_signal("alarm:test", WakePriority::Critical)];
        let prompt = build_prompt("ws-1", &[], &[], &signals, &[]);
        assert!(prompt.contains("实时事件"));
        assert!(prompt.contains("CRITICAL"));
        assert!(prompt.contains("alarm:test"));
    }

    #[test]
    fn test_build_prompt_includes_recent_actions() {
        let actions = vec![make_action("ws-1", ActionType::Summary, "一切正常")];
        let prompt = build_prompt("ws-1", &[], &actions, &[], &[]);
        assert!(prompt.contains("最近AI操作记录"));
    }

    #[test]
    fn test_build_prompt_skips_paused_tasks() {
        let tasks = vec![
            HeartbeatTask { priority: "high".into(), text: "活跃任务".into(), paused: false },
            HeartbeatTask { priority: "low".into(), text: "暂停任务".into(), paused: true },
        ];
        let prompt = build_prompt("ws-1", &tasks, &[], &[], &[]);
        assert!(prompt.contains("活跃任务"));
        assert!(!prompt.contains("暂停任务"));
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let input = "这是一个很长的字符串需要截断测试这是额外内容";
        let result = truncate(input, 15);
        assert!(result.contains("…"));
        assert!(result.len() < input.len(), "truncated result should be shorter than input");
    }

    #[test]
    fn test_parse_healing_report_valid_json_block() {
        let response = r#"巡检完成。

```json
{
  "auto_executed": [
    {"tool": "send_command", "device_id": "dev-001", "summary": "已重连离线设备"}
  ],
  "pending_proposals": []
}
```"#;
        let report = parse_healing_report(response);
        assert!(report.is_some());
        let report = report.unwrap();
        assert_eq!(report.auto_executed.len(), 1);
        assert_eq!(report.auto_executed[0].tool, "send_command");
        assert_eq!(report.auto_executed[0].device_id, "dev-001");
        assert!(report.pending_proposals.is_empty());
    }

    #[test]
    fn test_parse_healing_report_with_proposal() {
        let response = r#"```json
{
  "auto_executed": [],
  "pending_proposals": [
    {
      "level": "L2",
      "tool_name": "write_properties",
      "device_id": "dev-002",
      "device_name": "温度传感器A",
      "tool_params": {"device_id": "dev-002", "properties": {"report_interval": 60}},
      "summary": "调整上报间隔",
      "reason": "带宽浪费",
      "risk": "低"
    }
  ]
}
```"#;
        let report = parse_healing_report(response).unwrap();
        assert_eq!(report.pending_proposals.len(), 1);
        assert_eq!(report.pending_proposals[0].level, "L2");
        assert_eq!(report.pending_proposals[0].tool_name, "write_properties");
    }

    #[test]
    fn test_parse_healing_report_no_json() {
        let response = "巡检完成，一切正常。";
        assert!(parse_healing_report(response).is_none());
    }

    #[test]
    fn test_parse_healing_report_malformed_json() {
        let response = r#"```json
{ "auto_executed": [this is not valid json } ```
```"#;
        assert!(parse_healing_report(response).is_none());
    }

    #[test]
    fn test_extract_json_block() {
        let text = r#"some text
```json
{"key": "value"}
```
more text"#;
        let json = extract_json_block(text);
        assert_eq!(json.unwrap(), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_block_not_found() {
        assert!(extract_json_block("no code block here").is_none());
    }
}
