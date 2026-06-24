// Heartbeat module — per-workspace AI autonomous inspection
//
// Contains:
//   - HEARTBEAT.md task parsing (read/write/build/parse)
//   - heartbeat_loop — per-workspace async loop driving periodic AI inspection
//   - build_prompt — assembles prompt from tasks + recent actions + wake signals

use std::sync::Arc;

use super::agent::AgentPool;

/// A single heartbeat task
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTask {
    pub priority: String,
    pub text: String,
    pub paused: bool,
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

use super::action_repo::{AgentAction, AgentActionRepository};
use super::heartbeat_manager::{HeartbeatConfig, WakeSignal};

/// Per-workspace heartbeat loop — drives periodic AI inspection for a single workspace
pub(crate) async fn heartbeat_loop(
    workspace_id: String,
    config: HeartbeatConfig,
    agent_pool: Arc<AgentPool>,
    action_repo: Arc<dyn AgentActionRepository>,
    heartbeat_file: std::path::PathBuf,
    mut wake_rx: tokio::sync::mpsc::Receiver<WakeSignal>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let interval_secs = (config.interval_minutes as u64).saturating_mul(60);
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    let mut shutdown_rx = shutdown_rx;
    let mut consecutive_failures: u32 = 0;
    let mut skip_remaining: u32 = 0;

    tracing::info!(%workspace_id, interval_minutes=config.interval_minutes, "Heartbeat loop started");

    loop {
        let wake_signals: Vec<WakeSignal> = tokio::select! {
            _ = interval.tick() => {
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

        let wake_signals = dedup_and_cap(wake_signals, 5);

        // Exponential backoff: after N failures, skip 2^(N-1) ticks (capped at 1hr).
        // Wake signals always bypass the backoff.
        if skip_remaining > 0 && wake_signals.is_empty() {
            skip_remaining -= 1;
            tracing::info!(%workspace_id, skip_remaining, "Heartbeat backoff — skipping tick");
            continue;
        }

        let workspace_dir = heartbeat_file.parent().unwrap_or(std::path::Path::new("."));
        let tasks = read_heartbeat_tasks(workspace_dir).await.unwrap_or_else(|e| {
            tracing::warn!(%workspace_id, "Failed to read HEARTBEAT.md: {}", e);
            get_default_tasks()
        });

        let recent_actions = action_repo
            .find_recent_by_workspace(&workspace_id, &["heartbeat", "alarm"], config.max_recent_actions as u32)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(%workspace_id, "Failed to query recent actions: {}", e);
                vec![]
            });

        let prompt = build_prompt(&workspace_id, &tasks, &recent_actions, &wake_signals);

        let task_count = tasks.len();
        tracing::info!(
            %workspace_id,
            task_count,
            backoff = consecutive_failures,
            "Heartbeat tick — {} tasks to inspect",
            task_count,
        );

        match agent_pool.run_single(&workspace_id, &prompt).await {
            Ok(response) => {
                consecutive_failures = 0;
                skip_remaining = 0;
                let content = serde_json::json!({
                    "taskCount": task_count,
                    "result": truncate(&response, 5000),
                })
                .to_string();
                let action = AgentAction::new(
                    workspace_id.clone(),
                    "default".to_string(),
                    None,
                    None,
                    "heartbeat".to_string(),
                    "summary".to_string(),
                    content,
                );
                if let Err(e) = action_repo.insert(&action).await {
                    tracing::error!(%workspace_id, "Failed to record heartbeat action: {}", e);
                }
                tracing::debug!(%workspace_id, "Heartbeat tick completed");
            }
            Err(e) => {
                consecutive_failures = (consecutive_failures + 1).min(10);
                skip_remaining = (1u32 << (consecutive_failures - 1))
                    .min(60 / config.interval_minutes.max(1));
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
                    "heartbeat".to_string(),
                    "error".to_string(),
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
    signals.dedup_by_key(|s| s.reason.clone());
    signals.truncate(cap);
    signals
}

fn build_prompt(
    workspace_id: &str,
    tasks: &[HeartbeatTask],
    recent_actions: &[AgentAction],
    wake_signals: &[WakeSignal],
) -> String {
    let mut p = format!(
        "你是一个 IoT 平台的 AI 运维助手，负责工作空间 `{}` 的自动巡检。\n\n",
        workspace_id
    );

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
                action.action_type,
                truncate(&action.content, 200),
            ));
        }
        p.push('\n');
    }

    p.push_str("请执行以上巡检任务，以简洁的结构化格式输出结果。");
    p
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
    use super::*;
    use super::super::heartbeat_manager::WakePriority;

    fn make_signal(reason: &str, priority: WakePriority) -> WakeSignal {
        WakeSignal {
            workspace_id: "ws-1".into(),
            reason: reason.into(),
            context: "test".into(),
            priority,
        }
    }

    fn make_action(workspace_id: &str, action_type: &str, content: &str) -> AgentAction {
        AgentAction::new(
            workspace_id.into(),
            "default".into(),
            None,
            None,
            "heartbeat".into(),
            action_type.into(),
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
    fn test_dedup_and_cap_dedup_by_reason() {
        let signals = vec![
            make_signal("alarm:1", WakePriority::Critical),
            make_signal("alarm:1", WakePriority::High),
        ];
        let result = dedup_and_cap(signals, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].reason, "alarm:1");
    }

    #[test]
    fn test_dedup_and_cap_truncate() {
        let signals: Vec<WakeSignal> = (0..10)
            .map(|i| make_signal(&format!("sig-{}", i), WakePriority::Normal))
            .collect();
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
                priority: "high".into(),
                text: "检查离线设备".into(),
                paused: false,
            },
            HeartbeatTask {
                priority: "low".into(),
                text: "生成报表".into(),
                paused: true,
            },
        ];
        let md = build_heartbeat_md(&tasks);
        let parsed = parse_heartbeat_md(&md);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].priority, "high");
        assert_eq!(parsed[0].text, "检查离线设备");
        assert_eq!(parsed[1].paused, true);
    }

    #[test]
    fn test_build_prompt_includes_tasks() {
        let tasks = vec![HeartbeatTask {
            priority: "high".into(),
            text: "检查离线设备".into(),
            paused: false,
        }];
        let prompt = build_prompt("ws-1", &tasks, &[], &[]);
        assert!(prompt.contains("巡检任务"));
        assert!(prompt.contains("检查离线设备"));
        assert!(prompt.contains("ws-1"));
    }

    #[test]
    fn test_build_prompt_includes_wake_signals() {
        let signals = vec![make_signal("alarm:test", WakePriority::Critical)];
        let prompt = build_prompt("ws-1", &[], &[], &signals);
        assert!(prompt.contains("实时事件"));
        assert!(prompt.contains("CRITICAL"));
        assert!(prompt.contains("alarm:test"));
    }

    #[test]
    fn test_build_prompt_includes_recent_actions() {
        let actions = vec![make_action("ws-1", "summary", "一切正常")];
        let prompt = build_prompt("ws-1", &[], &actions, &[]);
        assert!(prompt.contains("最近AI操作记录"));
    }

    #[test]
    fn test_build_prompt_skips_paused_tasks() {
        let tasks = vec![
            HeartbeatTask {
                priority: "high".into(),
                text: "活跃任务".into(),
                paused: false,
            },
            HeartbeatTask {
                priority: "low".into(),
                text: "暂停任务".into(),
                paused: true,
            },
        ];
        let prompt = build_prompt("ws-1", &tasks, &[], &[]);
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
}
