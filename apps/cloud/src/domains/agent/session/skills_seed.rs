//! 把巡检职责从 heartbeat_tasks 迁移为 workspace 技能正文。
//!
//! 只在技能文件不存在（或内容为空）时写入；已存在的用户编辑永不覆盖。
//! 写入走同目录临时文件 + rename，避免半截文件被后续调用当成有效技能。

use std::path::Path;

use tinyiothub_storage::Db;

/// 技能模板（含 frontmatter）。编译期嵌入，默认正文与模板不再漂移。
const INSPECTION_TEMPLATE: &str = include_str!("../../../../templates/skills/inspection.md");

const TODO_LIFECYCLE: &str = "\n## 计划状态\n\n\
- 开始执行前调用 `todo_write` 创建本轮计划：当前步骤为 `in_progress`，其余步骤为 `pending`。\n\
- 每完成一个步骤，立即再次调用 `todo_write`：将该步骤标记为 `completed`，并将下一步骤标记为 `in_progress`。不要等到最终回复才更新。\n\
- 输出最终总结前再次调用 `todo_write`：完成项标记为 completed；确实被阻塞、无法完成的项保留 pending/in_progress，在最终回复说明阻塞原因，不得伪装完成。\n\
- 没有异常或提前结束时也要更新已执行项，不得让计划停留在初始状态。\n";

pub async fn ensure_inspection_skill_at(workspace_dir: &Path, db: &Db, workspace_id: &str) -> anyhow::Result<bool> {
    let skills_dir = workspace_dir.join("skills");
    let path = skills_dir.join("inspection.md");
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(existing) if !existing.trim().is_empty() => return Ok(false),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "inspection skill exists but is unreadable; leaving it untouched");
                return Ok(false);
            }
        }
    }
    std::fs::create_dir_all(&skills_dir)?;
    let tasks = match db.list_heartbeat_tasks(workspace_id).await {
        Ok(tasks) => tasks,
        Err(error) => {
            tracing::warn!(
                %workspace_id,
                %error,
                "read heartbeat tasks for inspection skill failed; using default body"
            );
            Vec::new()
        }
    };
    let content = if tasks.iter().any(|task| !task.paused) {
        let mut body = String::from("# 巡检\n\n## 执行步骤\n\n");
        for (index, task) in tasks.iter().filter(|task| !task.paused).enumerate() {
            body.push_str(&format!("{}. {}\n", index + 1, task.text));
        }
        body.push_str(TODO_LIFECYCLE);
        body.push_str("\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n");
        format!("---\nname: inspection\nversion: \"1\"\ntrigger: /inspection\ninject: index\n---\n\n{body}")
    } else {
        INSPECTION_TEMPLATE.to_string()
    };
    let tmp = skills_dir.join(".inspection.md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
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

    #[test]
    fn inspection_template_defines_todo_lifecycle() {
        assert!(INSPECTION_TEMPLATE.contains("todo_write"));
        assert!(INSPECTION_TEMPLATE.contains("每完成一个步骤"));
        assert!(INSPECTION_TEMPLATE.contains("不得伪装完成"));
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
        assert!(body.contains("每完成一个步骤"));
    }

    #[tokio::test]
    async fn unreadable_existing_skill_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let db = test_db().await;
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let path = skills_dir.join("inspection.md");
        let bytes = [0xff, 0xfe];
        std::fs::write(&path, bytes).unwrap();

        assert!(
            !ensure_inspection_skill_at(dir.path(), &db, "ws-unreadable")
                .await
                .unwrap(),
            "an unreadable existing file must not be overwritten"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[tokio::test]
    async fn repairs_empty_inspection_skill_file() {
        let dir = tempfile::tempdir().unwrap();
        let db = test_db().await;
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(skills_dir.join("inspection.md"), "").unwrap();

        assert!(
            ensure_inspection_skill_at(dir.path(), &db, "ws-empty").await.unwrap(),
            "an empty existing file must not satisfy the idempotency guard"
        );
        let body = std::fs::read_to_string(skills_dir.join("inspection.md")).unwrap();
        assert!(body.contains("name: inspection"));
        assert!(!body.trim().is_empty());
    }
}
