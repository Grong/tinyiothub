// Heartbeat module — per-workspace HEARTBEAT.md task parsing utilities.
//
// The heartbeat_loop and HeartbeatManager have been replaced by
// crate::loop_::heartbeat (HeartbeatRunner + heartbeat_loop).

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
pub async fn read_heartbeat_tasks(workspace_dir: &std::path::Path) -> anyhow::Result<Vec<HeartbeatTask>> {
    let path = workspace_dir.join("HEARTBEAT.md");
    if !path.exists() {
        return Ok(get_default_tasks());
    }
    let content = tokio::fs::read_to_string(&path).await?;
    Ok(parse_heartbeat_md(&content))
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
                tasks.push(HeartbeatTask {
                    priority,
                    text: text.to_string(),
                    paused,
                });
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

#[cfg(test)]
pub(crate) fn build_heartbeat_md(tasks: &[HeartbeatTask]) -> String {
    let mut s = "# Periodic Tasks\n".to_string();
    for task in tasks {
        let flag = if task.paused {
            format!("{}|paused", task.priority)
        } else {
            task.priority.clone()
        };
        s.push_str(&format!("- [{}] {}\n", flag, task.text));
    }
    s
}

/// One-time migration: import HEARTBEAT.md tasks into the DB table.
///
/// The DB is the single source of truth for heartbeat tasks; the file is a
/// legacy source. Runs only when the table is empty for this workspace and
/// the file exists. On success the file is renamed to HEARTBEAT.md.migrated
/// so it never re-seeds. Returns true when a migration happened.
pub async fn migrate_file_tasks_to_db(
    repo: &dyn crate::loop_::heartbeat::repo::HeartbeatTaskRepository,
    workspace_id: &str,
    workspace_dir: &std::path::Path,
) -> anyhow::Result<bool> {
    let existing = repo
        .list_by_workspace(workspace_id)
        .await
        .map_err(|e| anyhow::anyhow!("list tasks: {e}"))?;
    if !existing.is_empty() {
        return Ok(false);
    }
    let path = workspace_dir.join("HEARTBEAT.md");
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(false);
    }
    let tasks = read_heartbeat_tasks(workspace_dir).await?;
    if tasks.is_empty() {
        return Ok(false);
    }
    let new_tasks: Vec<crate::loop_::heartbeat::types::NewHeartbeatTask> = tasks
        .into_iter()
        .map(|t| crate::loop_::heartbeat::types::NewHeartbeatTask {
            priority: t.priority,
            text: t.text,
            paused: t.paused,
        })
        .collect();
    repo.replace_all(workspace_id, &new_tasks)
        .await
        .map_err(|e| anyhow::anyhow!("replace tasks: {e}"))?;
    tokio::fs::rename(&path, workspace_dir.join("HEARTBEAT.md.migrated")).await?;
    tracing::info!(
        workspace_id,
        count = new_tasks.len(),
        "Migrated HEARTBEAT.md tasks to DB"
    );
    Ok(true)
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
        assert!(parsed[1].paused);
    }

    async fn migration_test_repo() -> crate::host::heartbeat_repo::SqliteHeartbeatTaskRepository {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("in-memory sqlite");
        for stmt in
            include_str!("../../../../crates/db/migrations/20260629000001_create_heartbeat_tasks.sql").split(';')
        {
            let stmt = stmt.trim();
            if !stmt.is_empty() {
                sqlx::query(stmt).execute(&pool).await.expect("apply migration");
            }
        }
        crate::host::heartbeat_repo::SqliteHeartbeatTaskRepository::new(pool)
    }

    #[tokio::test]
    async fn test_migrate_file_tasks_to_db_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("HEARTBEAT.md"),
            "- [high] 检查设备\n- [low|paused] 日报",
        )
        .unwrap();
        let repo = migration_test_repo().await;

        let migrated = migrate_file_tasks_to_db(&repo, "ws_1", dir.path()).await.unwrap();
        assert!(migrated);

        use crate::loop_::heartbeat::repo::HeartbeatTaskRepository;
        let tasks = repo.list_by_workspace("ws_1").await.unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|t| t.text == "检查设备" && t.priority == "high" && !t.paused)
        );
        assert!(tasks.iter().any(|t| t.text == "日报" && t.paused));
        assert!(!dir.path().join("HEARTBEAT.md").exists());
        assert!(dir.path().join("HEARTBEAT.md.migrated").exists());

        // Second run: file gone + table non-empty → no-op, no duplicates
        let again = migrate_file_tasks_to_db(&repo, "ws_1", dir.path()).await.unwrap();
        assert!(!again);
        assert_eq!(repo.list_by_workspace("ws_1").await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_migrate_without_file_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let repo = migration_test_repo().await;
        let migrated = migrate_file_tasks_to_db(&repo, "ws_1", dir.path()).await.unwrap();
        assert!(!migrated);
    }
}
