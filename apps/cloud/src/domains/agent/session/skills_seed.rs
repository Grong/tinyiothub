//! 内置技能播种：`apps/cloud/templates/skills/` 是唯一真源。
//!
//! 两级目录：全局 `data/skills/`（内置技能住所，启动时按模板覆盖写）与工作区
//! `<workspace>/skills/`（同名文件覆盖全局，属于历史副本或用户改动）。用户自建技能
//! 用别的文件名，本模块只按模板文件名处理，绝不触碰其它文件。
//!
//! 工作区副本的覆盖策略由 frontmatter `builtin:` 修订号决定：
//! - 无标记且与模板相同 → 不动；
//! - 无标记但内容不同 → 旧版内置副本，备份成 `<name>.md.bak` 后用模板覆盖；
//! - 标记修订号 ≥ 当前 → 不动（例如按 heartbeat_tasks 生成的巡检正文）；
//! - 标记修订号 < 当前 → 静默覆盖。
//!
//! 写入走同目录临时文件 + rename，避免半截文件被后续调用当成有效技能。

use std::path::Path;

use tinyiothub_skills::SkillDefinition;
use tinyiothub_storage::Db;

use crate::domains::agent::prompt::templates::{
    BUILTIN_SKILL_REV, INSPECTION_SKILL_MD, RETIRED_SKILLS, SKILL_TEMPLATES, VIDEO_INSPECTION_SKILL_MD,
};

const TODO_LIFECYCLE: &str = "\n## 计划状态\n\n\
- 加载本技能后，将 Todo 中本技能的占位项展开为下面的执行步骤；每项是具体动作。若 Todo 中还没有本技能项，先添加。当前步骤为 `in_progress`，其余步骤为 `pending`。\n\
- 每完成一个步骤，立即再次调用 `todo_write`：将该步骤标记为 `completed`，并将下一步骤标记为 `in_progress`。不要等到最终回复才更新。\n\
- 输出最终总结前再次调用 `todo_write`：完成项标记为 completed；确实被阻塞、无法完成的项保留 pending/in_progress，在最终回复说明阻塞原因，不得伪装完成。\n\
- 没有异常或提前结束时也要更新已执行项，不得让计划停留在初始状态。\n";

/// 读取 frontmatter 里的 `builtin` 修订号；无标记（用户自建或旧版副本）返回 `None`。
fn builtin_rev(content: &str) -> Option<u32> {
    let (frontmatter, _) = SkillDefinition::parse_frontmatter(content);
    frontmatter?.get("builtin")?.as_str()?.trim_matches('"').parse().ok()
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "skill path has no file name",
        ));
    };
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

/// 归档已下线技能，避免旧副本继续出现在技能索引里。
fn retire_skills(dir: &Path) -> anyhow::Result<usize> {
    let mut retired = 0;
    for name in RETIRED_SKILLS {
        let path = dir.join(format!("{name}.md"));
        if !path.exists() {
            continue;
        }
        let archived = dir.join(format!("{name}.md.retired"));
        std::fs::rename(&path, &archived)?;
        tracing::info!(skill = %name, path = %archived.display(), "retired builtin skill archived");
        retired += 1;
    }
    Ok(retired)
}

/// 把产品技能模板同步到全局技能目录。返回发生变化（写入或归档）的文件数。
pub fn ensure_global_skills_at(dir: &Path) -> anyhow::Result<usize> {
    std::fs::create_dir_all(dir)?;
    let mut changed = retire_skills(dir)?;
    for (name, body) in SKILL_TEMPLATES {
        let path = dir.join(format!("{name}.md"));
        if std::fs::read_to_string(&path).ok().as_deref() == Some(*body) {
            continue;
        }
        write_atomic(&path, body)?;
        changed += 1;
    }
    Ok(changed)
}

pub fn ensure_global_skills() -> anyhow::Result<usize> {
    ensure_global_skills_at(&crate::domains::agent::prompt::paths::global_skills_dir())
}

/// 刷新工作区内已存在的内置技能副本。内置技能的住所是全局目录，因此这里只处理
/// 工作区里已有的同名文件（旧版副本/用户改动），不新建；用户自建技能永不触碰。
pub fn ensure_builtin_skills_at(dir: &Path) -> anyhow::Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut changed = retire_skills(dir)?;
    for (name, body) in SKILL_TEMPLATES {
        let path = dir.join(format!("{name}.md"));
        let existing = match std::fs::read_to_string(&path) {
            Ok(existing) => existing,
            Err(_) => continue,
        };
        if existing.trim().is_empty() {
            write_atomic(&path, body)?;
            changed += 1;
            continue;
        }
        if existing == *body {
            continue;
        }
        match builtin_rev(&existing) {
            Some(rev) if rev >= BUILTIN_SKILL_REV => continue,
            Some(_) => {
                write_atomic(&path, body)?;
                changed += 1;
            }
            None => {
                let backup = path.with_file_name(format!("{name}.md.bak"));
                if !backup.exists() {
                    std::fs::copy(&path, &backup)?;
                }
                write_atomic(&path, body)?;
                changed += 1;
                tracing::info!(
                    skill = %name,
                    backup = %backup.display(),
                    "legacy builtin skill copy refreshed from template"
                );
            }
        }
    }
    Ok(changed)
}

pub fn ensure_builtin_skills(workspace_id: &str) -> anyhow::Result<usize> {
    let dir = crate::domains::agent::prompt::paths::workspace_skills_dir(workspace_id);
    ensure_builtin_skills_at(&dir)
}

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
        let mut body = String::from("# 设备巡检\n\n## 执行步骤\n\n");
        for (index, task) in tasks.iter().filter(|task| !task.paused).enumerate() {
            body.push_str(&format!("{}. {}\n", index + 1, task.text));
        }
        body.push_str(TODO_LIFECYCLE);
        body.push_str("\n## 注意事项\n\n- 涉及写设备必须先获得审批。\n");
        format!(
            "---\nname: inspection\ntitle: 设备巡检\nversion: \"2\"\ntrigger: /inspection\ninject: index\nbuiltin: \"{BUILTIN_SKILL_REV}\"\n---\n\n{body}"
        )
    } else {
        INSPECTION_SKILL_MD.to_string()
    };
    let tmp = skills_dir.join(".inspection.md.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(true)
}

pub async fn ensure_video_inspection_skill_at(workspace_dir: &Path) -> anyhow::Result<bool> {
    let skills_dir = workspace_dir.join("skills");
    let path = skills_dir.join("video-inspection.md");
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(existing) if !existing.trim().is_empty() => return Ok(false),
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "video-inspection skill exists but is unreadable; leaving it untouched");
                return Ok(false);
            }
        }
    }
    std::fs::create_dir_all(&skills_dir)?;
    let tmp = skills_dir.join(".video-inspection.md.tmp");
    std::fs::write(&tmp, VIDEO_INSPECTION_SKILL_MD)?;
    std::fs::rename(&tmp, &path)?;
    Ok(true)
}

pub async fn ensure_video_inspection_skill(workspace_id: &str) -> anyhow::Result<bool> {
    let dir = crate::domains::agent::prompt::paths::workspace_dir(workspace_id);
    ensure_video_inspection_skill_at(&dir).await
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
    fn video_inspection_template_defines_snapshot_contract() {
        let parsed = tinyiothub_skills::loader::parse_skill("video-inspection", VIDEO_INSPECTION_SKILL_MD)
            .expect("video inspection template must parse");
        assert_eq!(parsed.name, "video-inspection");
        assert_eq!(parsed.title, "视频巡检");
        assert_eq!(parsed.trigger.as_deref(), Some("/video-inspection"));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("name: video-inspection"));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("call_device"));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("action=\"snapshot\""));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("消防通道"));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("静态截图无法可靠判断"));
        assert!(VIDEO_INSPECTION_SKILL_MD.contains("todo_write"));
        assert!(
            VIDEO_INSPECTION_SKILL_MD.contains("不要用 `read_device`"),
            "skill must explicitly forbid the read_device snapshot path"
        );
    }

    #[test]
    fn inspection_template_defines_todo_lifecycle() {
        assert!(INSPECTION_SKILL_MD.contains("todo_write"));
        assert!(INSPECTION_SKILL_MD.contains("每完成一个步骤"));
        assert!(INSPECTION_SKILL_MD.contains("不得伪装完成"));
    }

    /// 节能技能的闭环契约：系统级分析、数据缺口、群控分档、回读验证与核算口径。
    /// 这些段落是修订号 3 的核心变化，删掉会退回“一次性机会清单”。
    #[test]
    fn energy_saving_template_defines_system_loop() {
        let body = SKILL_TEMPLATES
            .iter()
            .find(|(name, _)| *name == "energy-saving")
            .expect("energy-saving template missing")
            .1;
        let parsed =
            tinyiothub_skills::loader::parse_skill("energy-saving", body).expect("energy-saving template must parse");
        assert_eq!(parsed.title, "节能分析");
        assert_eq!(parsed.trigger.as_deref(), Some("/energy-saving"));
        assert!(body.contains("能耗系统"), "analysis unit must be a system");
        assert!(body.contains("数据缺口"), "gaps must be reported");
        assert!(body.contains("禁止输出负荷预测"), "no forecast without history");
        assert!(body.contains("群控分档"), "multi-unit systems need a band table");
        assert!(body.contains("未生效"), "writes must be verified by read-back");
        assert!(body.contains("核算口径"), "savings must follow a stated method");
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
        assert_eq!(builtin_rev(&body), Some(BUILTIN_SKILL_REV));
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
        assert_eq!(
            ensure_builtin_skills_at(&skills_dir).unwrap(),
            0,
            "unreadable copies are left untouched"
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

    #[test]
    fn global_sync_writes_every_template_and_archives_retired() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(
            skills_dir.join("heartbeat-monitor.md"),
            "---\nname: heartbeat-monitor\n---\n",
        )
        .unwrap();

        assert_eq!(ensure_global_skills_at(&skills_dir).unwrap(), SKILL_TEMPLATES.len() + 1);
        for (name, body) in SKILL_TEMPLATES {
            assert_eq!(
                std::fs::read_to_string(skills_dir.join(format!("{name}.md"))).unwrap(),
                *body
            );
        }
        assert!(!skills_dir.join("heartbeat-monitor.md").exists());
        assert!(skills_dir.join("heartbeat-monitor.md.retired").exists());
        assert_eq!(ensure_global_skills_at(&skills_dir).unwrap(), 0);
    }

    #[test]
    fn workspace_sync_refreshes_legacy_copy_with_backup_and_keeps_user_skills() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(
            skills_dir.join("troubleshooting.md"),
            "---\nname: troubleshooting\n---\n\n旧正文\n",
        )
        .unwrap();
        std::fs::write(skills_dir.join("my-own-skill.md"), "user content").unwrap();

        assert_eq!(ensure_builtin_skills_at(&skills_dir).unwrap(), 1);
        let refreshed = std::fs::read_to_string(skills_dir.join("troubleshooting.md")).unwrap();
        assert_eq!(
            refreshed,
            crate::domains::agent::prompt::templates::SKILL_TEMPLATES
                .iter()
                .find(|(name, _)| *name == "troubleshooting")
                .map(|(_, body)| *body)
                .unwrap()
        );
        assert!(skills_dir.join("troubleshooting.md.bak").exists());
        assert_eq!(
            std::fs::read_to_string(skills_dir.join("my-own-skill.md")).unwrap(),
            "user content"
        );
        assert_eq!(ensure_builtin_skills_at(&skills_dir).unwrap(), 0);
    }

    #[test]
    fn workspace_sync_respects_current_and_newer_builtin_rev() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let custom = format!(
            "---\nname: inspection\ntitle: 设备巡检\nbuiltin: \"{BUILTIN_SKILL_REV}\"\n---\n\n按任务生成的正文\n"
        );
        std::fs::write(skills_dir.join("inspection.md"), &custom).unwrap();

        assert_eq!(ensure_builtin_skills_at(&skills_dir).unwrap(), 0);
        assert_eq!(
            std::fs::read_to_string(skills_dir.join("inspection.md")).unwrap(),
            custom
        );
        assert!(!skills_dir.join("inspection.md.bak").exists());
        assert!(
            std::fs::read_to_string(skills_dir.join("troubleshooting.md")).is_err(),
            "missing builtin copies stay absent: the global dir is their home"
        );
    }

    fn backticked_tokens(body: &str) -> Vec<&str> {
        let mut tokens = Vec::new();
        let mut rest = body;
        while let Some(start) = rest.find('`') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('`') else { break };
            tokens.push(&after[..end]);
            rest = &after[end + 1..];
        }
        tokens
    }

    /// 工具形状的 token：全小写 + 至少一个下划线（`read_device`），排除驼峰字段名。
    fn is_tool_shaped(token: &str) -> bool {
        token.contains('_')
            && !token.starts_with('_')
            && !token.ends_with('_')
            && token.chars().all(|c| c.is_ascii_lowercase() || c == '_')
    }

    #[test]
    fn skill_templates_only_name_model_visible_tools() {
        use crate::domains::agent::tools::catalog::{PLATFORM_PLANE_TOOLS, TOOL_CATALOG};
        use std::collections::HashSet;

        let canonical: HashSet<&str> = TOOL_CATALOG.iter().map(|entry| entry.name).collect();
        let aliases: HashSet<&str> = TOOL_CATALOG
            .iter()
            .flat_map(|entry| entry.aliases.iter().copied())
            .collect();
        let non_tools: HashSet<&str> = ["file_path", "in_progress"].into_iter().collect();

        for (name, body) in SKILL_TEMPLATES {
            for token in backticked_tokens(body) {
                if !is_tool_shaped(token) {
                    continue;
                }
                assert!(
                    !aliases.contains(token),
                    "{name} 引用了隐藏别名 `{token}`；模型只看得到规范名，请改用 canonical 名"
                );
                assert!(
                    canonical.contains(token) || PLATFORM_PLANE_TOOLS.contains(&token) || non_tools.contains(token),
                    "{name} 引用了不存在的工具 `{token}`；新增平台工具请登记到 PLATFORM_PLANE_TOOLS"
                );
            }
        }
    }

    #[test]
    fn skill_templates_declare_current_builtin_rev_and_unique_titles() {
        let expected = BUILTIN_SKILL_REV.to_string();
        let mut parsed = Vec::new();
        for (name, body) in SKILL_TEMPLATES {
            let (frontmatter, _) = SkillDefinition::parse_frontmatter(body);
            let frontmatter = frontmatter.unwrap_or_else(|| panic!("{name} 缺少 frontmatter"));
            let rev = frontmatter
                .get("builtin")
                .and_then(|value| value.as_str())
                .map(|value| value.trim_matches('"').to_string());
            assert_eq!(
                rev.as_deref(),
                Some(expected.as_str()),
                "{name} 必须声明 builtin: {expected}"
            );
            assert!(!RETIRED_SKILLS.contains(name), "{name} 已下线，不能继续留在模板表里");
            parsed.push(tinyiothub_skills::loader::parse_skill(name, body).expect("skill template must parse"));
        }

        for outer in &parsed {
            for inner in &parsed {
                if outer.name == inner.name {
                    continue;
                }
                assert!(
                    !inner.title.contains(&outer.title),
                    "标题冲突：`{}` 的标题 {:?} 是 `{}` 的标题 {:?} 的子串；agenda 按标题兜底匹配会双命中",
                    outer.name,
                    outer.title,
                    inner.name,
                    inner.title
                );
            }
        }
    }
}
