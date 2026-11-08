//! 把巡检职责从 heartbeat_tasks 迁移为 workspace 技能正文。
//!
//! 只在技能文件不存在时写入；已存在的用户编辑永不覆盖。

use std::path::Path;

use tinyiothub_storage::Db;

const DEFAULT_BODY: &str = "# 巡检\n\n## 执行步骤\n\n1. 检查离线设备并尝试自动重连。\n2. 扫描未处理的高优先级告警。\n3. 生成设备状态摘要（聚焦异常与离线设备）。\n\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n";

pub async fn ensure_inspection_skill_at(workspace_dir: &Path, db: &Db, workspace_id: &str) -> anyhow::Result<bool> {
    let skills_dir = workspace_dir.join("skills");
    let path = skills_dir.join("inspection.md");
    if path.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(&skills_dir)?;
    let tasks = db.list_heartbeat_tasks(workspace_id).await.unwrap_or_default();
    let body = if tasks.iter().any(|task| !task.paused) {
        let mut body = String::from("# 巡检\n\n## 执行步骤\n\n");
        for (index, task) in tasks.iter().filter(|task| !task.paused).enumerate() {
            body.push_str(&format!("{}. {}\n", index + 1, task.text));
        }
        body.push_str("\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n");
        body
    } else {
        DEFAULT_BODY.to_string()
    };
    let content = format!("---\nname: inspection\nversion: \"1\"\ntrigger: /inspection\ninject: index\n---\n\n{body}");
    std::fs::write(&path, content)?;
    Ok(true)
}

pub async fn ensure_inspection_skill(db: &Db, workspace_id: &str) -> anyhow::Result<bool> {
    let dir = crate::domains::agent::prompt::paths::workspace_dir(workspace_id);
    ensure_inspection_skill_at(&dir, db, workspace_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        Db::new(pool)
    }

    #[tokio::test]
    async fn seeds_inspection_skill_from_tasks_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = test_db().await;
        db.replace_heartbeat_tasks(
            "ws-skill",
            &[tinyiothub_core::heartbeat::NewHeartbeatTask {
                priority: "high".into(),
                text: "检查离线设备".into(),
                paused: false,
            }],
        )
        .await
        .unwrap();

        assert!(ensure_inspection_skill_at(dir.path(), &db, "ws-skill").await.unwrap());
        assert!(!ensure_inspection_skill_at(dir.path(), &db, "ws-skill").await.unwrap());
        let body = std::fs::read_to_string(dir.path().join("skills/inspection.md")).unwrap();
        assert!(body.contains("检查离线设备"));
        assert!(body.contains("name: inspection"));
    }
}
