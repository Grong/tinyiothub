// Heartbeat module — per-workspace HEARTBEAT.md task parsing utilities.
//
// The heartbeat_loop and HeartbeatManager have been replaced by
// tinyiothub_ai::heartbeat (HeartbeatRunner + heartbeat_loop).

use serde::{Deserialize, Serialize};

/// A single heartbeat task
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
