//! Agent runs 持久化：自治 run 报告（P-集中化 E6b，自 agent crate 迁入）。
//!
//! 值类型归位 core（Task 1），本模块经 glob re-export 组织 db 内部路径；
//! 全部 SQL 留在本文件，经 `Db` 门面委托暴露（Task 9）。

use sqlx::SqlitePool;

use crate::database::Db;
use crate::error::Result;

// 领域值类型住 core（tinyiothub_core::agent_runs）；此处 re-export 仅为 db
// 内部模块组织，非跨 crate 摆渡层。
pub use tinyiothub_core::agent_runs::*;

// ──────────────────────────────────────────────
// 持久化函数（pool 参数）+ Db 门面委托
// ──────────────────────────────────────────────

/// trigger 串前缀作为 trigger_type（"thing:t1:event:x" → "thing"）；无 ':'
/// 时用整串。
fn trigger_type_of(trigger: &str) -> &str {
    trigger.split(':').next().unwrap_or(trigger)
}

async fn insert_run(
    pool: &SqlitePool,
    report: &RunReport,
    problem_key: Option<&str>,
    dedup_key: Option<&str>,
) -> Result<()> {
    // RunReport 结构体无 action_count 字段，但 T4 count_actions_last_hour
    // 依赖 json_extract(report,'$.action_count') —— 落库时显式补写。
    let mut report_json = serde_json::to_value(report)?;
    report_json["action_count"] = serde_json::json!(report.actions.len());

    let outcome = report.outcome.as_str();
    sqlx::query(
        "INSERT INTO agent_runs
                 (id, workspace_id, trigger_type, trigger_context, outcome, summary,
                  report, verified, tool_calls, tokens, duration_ms, problem_key, dedup_key)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&report.run_id)
    .bind(&report.workspace_id)
    .bind(trigger_type_of(&report.trigger))
    .bind(&report.trigger)
    .bind(outcome)
    .bind(&report.summary)
    .bind(report_json.to_string())
    .bind(report.verified)
    .bind(i64::from(report.tool_calls))
    .bind(report.tokens as i64)
    .bind(report.duration_ms as i64)
    .bind(problem_key)
    .bind(dedup_key)
    .execute(pool)
    .await?;

    // X4 指标：结构化日志 metric 字段（router.rs 先例，一次一条）。
    tracing::info!(
        metric = "agent_run_completed",
        workspace_id = %report.workspace_id,
        run_id = %report.run_id,
        outcome = outcome,
        duration_ms = report.duration_ms,
        "Agent run persisted"
    );
    tracing::info!(
        metric = "agent_tokens_daily",
        workspace_id = %report.workspace_id,
        tokens = report.tokens,
        "Agent run tokens"
    );
    Ok(())
}

async fn recent_summaries(pool: &SqlitePool, workspace_id: &str, limit: u32) -> Result<Vec<String>> {
    // rowid 决胜：created_at 秒级精度，同秒批量插入仍保持插入序。
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT outcome, summary FROM agent_runs
             WHERE workspace_id = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
    )
    .bind(workspace_id)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(o, s)| format_summary(&o, &s)).collect())
}

async fn history_by_dedup_key(pool: &SqlitePool, workspace_id: &str, key: &str, limit: u32) -> Result<Vec<String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT outcome, summary FROM agent_runs
             WHERE workspace_id = ? AND dedup_key = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
    )
    .bind(workspace_id)
    .bind(key)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(o, s)| format_summary(&o, &s)).collect())
}

async fn recent_runs_by_dedup_key(
    pool: &SqlitePool,
    workspace_id: &str,
    key: &str,
    limit: u32,
) -> Result<Vec<RunReport>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT report FROM agent_runs
             WHERE workspace_id = ? AND dedup_key = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
    )
    .bind(workspace_id)
    .bind(key)
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|(json,)| serde_json::from_str::<RunReport>(&json).map_err(Into::into))
        .collect()
}

async fn ack_run(pool: &SqlitePool, run_id: &str, actor: &str) -> Result<bool> {
    // 幂等：仅首认生效（acked_at IS NULL），重复确认/不存在 rows_affected = 0。
    let result = sqlx::query(
        "UPDATE agent_runs SET acked_at = datetime('now'), acked_by = ?
             WHERE id = ? AND acked_at IS NULL",
    )
    .bind(actor)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn last_problem_run(
    pool: &SqlitePool,
    workspace_id: &str,
    problem_key: &str,
    since_hours: u32,
) -> Result<Option<(Outcome, bool, bool)>> {
    let row: Option<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT outcome, verified, acked_at FROM agent_runs
             WHERE workspace_id = ? AND problem_key = ?
               AND created_at > datetime('now', ?)
             ORDER BY created_at DESC, rowid DESC
             LIMIT 1",
    )
    .bind(workspace_id)
    .bind(problem_key)
    .bind(format!("-{since_hours} hours"))
    .fetch_optional(pool)
    .await?;
    // 未知 outcome 字符串 fail-closed 到 Failed（X6 dedup 保守方向）。
    Ok(row.map(|(o, verified, acked_at)| {
        (
            Outcome::from_db(&o).unwrap_or(Outcome::Failed),
            verified,
            acked_at.is_some(),
        )
    }))
}

async fn count_problem_runs(pool: &SqlitePool, workspace_id: &str, problem_key: &str, since_hours: u32) -> Result<u32> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM agent_runs
             WHERE workspace_id = ? AND problem_key = ?
               AND created_at > datetime('now', ?)",
    )
    .bind(workspace_id)
    .bind(problem_key)
    .bind(format!("-{since_hours} hours"))
    .fetch_one(pool)
    .await?;
    Ok(n as u32)
}

/// 僵尸 run reconcile（Task 9 启动顺序第 3 步）：status='running' 但
/// 调用方（启动时的内存 RunRegistry）不认领的行 → 'interrupted'。
/// 进程刚启动时无在飞 run，`known_active` 为防御性排除集（预热窗口
/// 已有完成报告的 run_id）。逐行条件更新（status 仍为 'running' 才
/// 生效），启动期执行一次，行数有界。返回标记行数。
async fn interrupt_zombie_running_runs(pool: &SqlitePool, known_active: &[String]) -> Result<u64> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM agent_runs WHERE status = 'running'")
        .fetch_all(pool)
        .await?;
    let mut marked = 0u64;
    for (id,) in rows {
        if known_active.iter().any(|k| k == &id) {
            continue;
        }
        let result = sqlx::query("UPDATE agent_runs SET status = 'interrupted' WHERE id = ? AND status = 'running'")
            .bind(&id)
            .execute(pool)
            .await?;
        marked += result.rows_affected();
    }
    Ok(marked)
}

// ──────────────────────────────────────────────
// Db 门面委托
// ──────────────────────────────────────────────

impl Db {
    /// 落库一条自治 run 报告（report JSON 补写 action_count；X4 指标日志）。
    pub async fn insert_agent_run(
        &self,
        report: &RunReport,
        problem_key: Option<&str>,
        dedup_key: Option<&str>,
    ) -> Result<()> {
        insert_run(self.pool(), report, problem_key, dedup_key).await
    }

    /// 工作区最近 run 的 "[outcome] summary" 串（最新在前，limit 截断）。
    pub async fn recent_agent_run_summaries(&self, workspace_id: &str, limit: u32) -> Result<Vec<String>> {
        recent_summaries(self.pool(), workspace_id, limit).await
    }

    /// 按 dedup_key 过滤的最近 run 摘要串（最新在前，limit 截断）。
    pub async fn agent_run_history_by_dedup_key(
        &self,
        workspace_id: &str,
        key: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        history_by_dedup_key(self.pool(), workspace_id, key, limit).await
    }

    /// 按 dedup_key 过滤的最近 RunReport（JSON 解析回结构体，最新在前）。
    pub async fn recent_agent_runs_by_dedup_key(
        &self,
        workspace_id: &str,
        key: &str,
        limit: u32,
    ) -> Result<Vec<RunReport>> {
        recent_runs_by_dedup_key(self.pool(), workspace_id, key, limit).await
    }

    /// 幂等确认 run（仅首认生效；返回是否本次写入）。
    pub async fn ack_agent_run(&self, run_id: &str, actor: &str) -> Result<bool> {
        ack_run(self.pool(), run_id, actor).await
    }

    /// 窗口内某 problem_key 的最新 run（outcome, verified, acked）；未知
    /// outcome fail-closed 到 Failed。
    pub async fn last_problem_agent_run(
        &self,
        workspace_id: &str,
        problem_key: &str,
        since_hours: u32,
    ) -> Result<Option<(Outcome, bool, bool)>> {
        last_problem_run(self.pool(), workspace_id, problem_key, since_hours).await
    }

    /// 窗口内某 problem_key 的 run 计数。
    pub async fn count_problem_agent_runs(
        &self,
        workspace_id: &str,
        problem_key: &str,
        since_hours: u32,
    ) -> Result<u32> {
        count_problem_runs(self.pool(), workspace_id, problem_key, since_hours).await
    }

    /// 僵尸 run reconcile：status='running' 且不在 `known_active` 认领集
    /// 的行标记 'interrupted'，返回标记行数。
    pub async fn interrupt_zombie_running_agent_runs(&self, known_active: &[String]) -> Result<u64> {
        interrupt_zombie_running_runs(self.pool(), known_active).await
    }
}

#[cfg(test)]
mod tests {
    use crate::agent_runs::{ActionRecord, ActionResult};

    use super::*;

    pub async fn test_pool() -> SqlitePool {
        crate::test_helpers::test_pool().await
    }

    fn sample_report(run_id: &str, workspace_id: &str, summary: &str) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            workspace_id: workspace_id.to_string(),
            trigger: "thing:t1:event:temp_high".to_string(),
            outcome: Outcome::Acted,
            summary: summary.to_string(),
            actions: vec![
                ActionRecord {
                    thing_id: "t1".to_string(),
                    action_name: "set_fan".to_string(),
                    params: serde_json::json!({"speed": 3}),
                    result: ActionResult::Success(serde_json::json!({"ok": true})),
                    verified: true,
                },
                ActionRecord {
                    thing_id: "t2".to_string(),
                    action_name: "reboot".to_string(),
                    params: serde_json::Value::Null,
                    result: ActionResult::Failed("timeout".to_string()),
                    verified: false,
                },
            ],
            verified: true,
            duration_ms: 1234,
            tool_calls: 5,
            tokens: 6789,
        }
    }

    /// Raw insert with explicit created_at / problem_key / dedup_key for
    /// window and view tests.
    pub async fn insert_raw(
        pool: &SqlitePool,
        id: &str,
        workspace_id: &str,
        outcome: &str,
        problem_key: Option<&str>,
        dedup_key: Option<&str>,
        tokens: i64,
        age_modifier: &str,
    ) {
        sqlx::query(
            "INSERT INTO agent_runs
                 (id, workspace_id, trigger_type, outcome, summary, report, verified,
                  tokens, problem_key, dedup_key, created_at)
             VALUES (?, ?, 'timer', ?, ?, '{}', 0, ?, ?, ?, datetime('now', ?))",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(outcome)
        .bind(format!("summary of {id}"))
        .bind(tokens)
        .bind(problem_key)
        .bind(dedup_key)
        .bind(age_modifier)
        .execute(pool)
        .await
        .expect("raw insert agent_run");
    }

    #[tokio::test]
    pub async fn insert_run_persists_row_with_action_count_in_report_json() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        let report = sample_report("run_1", "ws_1", "调低设定值成功");
        db.insert_agent_run(&report, Some("temp_high:t1"), Some("thing:t1:event:temp_high"))
            .await
            .expect("insert_run");

        let row: (
            String,
            String,
            String,
            String,
            bool,
            i64,
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT trigger_type, trigger_context, outcome, summary, verified,
                        tool_calls, tokens, duration_ms, problem_key, dedup_key
                 FROM agent_runs WHERE id = 'run_1'",
        )
        .fetch_one(&pool)
        .await
        .expect("row");
        assert_eq!(row.0, "thing");
        assert_eq!(row.1, "thing:t1:event:temp_high");
        assert_eq!(row.2, "acted");
        assert_eq!(row.3, "调低设定值成功");
        assert!(row.4);
        assert_eq!(row.5, 5);
        assert_eq!(row.6, 6789);
        assert_eq!(row.7, 1234);
        assert_eq!(row.8.as_deref(), Some("temp_high:t1"));
        assert_eq!(row.9.as_deref(), Some("thing:t1:event:temp_high"));

        // T4 count_actions_last_hour 依赖 json_extract(report,'$.action_count')。
        let (action_count,): (i64,) =
            sqlx::query_as("SELECT json_extract(report, '$.action_count') FROM agent_runs WHERE id = 'run_1'")
                .fetch_one(&pool)
                .await
                .expect("action_count");
        assert_eq!(action_count, 2);

        // report JSON 列保留完整 RunReport（round-trip）。
        let (report_json,): (String,) = sqlx::query_as("SELECT report FROM agent_runs WHERE id = 'run_1'")
            .fetch_one(&pool)
            .await
            .expect("report json");
        let back: RunReport = serde_json::from_str(&report_json).expect("report json parses as RunReport");
        assert_eq!(back.run_id, "run_1");
        assert_eq!(back.actions.len(), 2);
        assert_eq!(back.outcome, Outcome::Acted);
    }

    #[tokio::test]
    pub async fn recent_summaries_returns_latest_first_capped_and_formatted() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        for i in 1..=7 {
            let mut report = sample_report(&format!("run_{i}"), "ws_1", &format!("摘要{i}"));
            if i == 3 {
                report.outcome = Outcome::Failed;
            }
            db.insert_agent_run(&report, None, None).await.expect("insert");
        }
        // 其他工作区不混入
        db.insert_agent_run(&sample_report("run_other", "ws_2", "别的"), None, None)
            .await
            .expect("insert other ws");

        let summaries = db.recent_agent_run_summaries("ws_1", 5).await.expect("recent");
        assert_eq!(summaries.len(), 5);
        // 最新在前（run_7 → run_3），含 outcome 前缀
        assert_eq!(summaries[0], "[acted] 摘要7");
        assert_eq!(summaries[4], "[failed] 摘要3");
        assert!(!summaries.iter().any(|s| s.contains("摘要1") || s.contains("摘要2")));
    }

    #[tokio::test]
    pub async fn history_by_dedup_key_filters_and_caps() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        for i in 1..=4 {
            db.insert_agent_run(
                &sample_report(&format!("k1_{i}"), "ws_1", &format!("同类{i}")),
                None,
                Some("key1"),
            )
            .await
            .expect("insert");
        }
        db.insert_agent_run(&sample_report("k2_1", "ws_1", "另一类"), None, Some("key2"))
            .await
            .expect("insert");
        db.insert_agent_run(&sample_report("k1_other_ws", "ws_2", "他区"), None, Some("key1"))
            .await
            .expect("insert");

        let history = db.agent_run_history_by_dedup_key("ws_1", "key1", 3).await.expect("history");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0], "[acted] 同类4");
        assert_eq!(history[2], "[acted] 同类2");
        assert!(!history.iter().any(|s| s.contains("另一类") || s.contains("他区")));

        assert!(
            db.agent_run_history_by_dedup_key("ws_1", "missing", 3)
                .await
                .expect("empty")
                .is_empty()
        );
    }

    #[tokio::test]
    pub async fn recent_runs_by_dedup_key_returns_parsed_reports_newest_first() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        db.insert_agent_run(&sample_report("acted", "ws_1", "已处理"), None, Some("key1"))
            .await
            .expect("insert acted");
        let mut denied = sample_report("denied", "ws_1", "策略拒绝");
        denied.outcome = Outcome::Rejected;
        denied.actions = vec![ActionRecord {
            thing_id: "t1".to_string(),
            action_name: "reboot".to_string(),
            params: serde_json::Value::Null,
            result: ActionResult::Success(serde_json::json!({"denied": true, "reason": "action_not_allowed"})),
            verified: false,
        }];
        db.insert_agent_run(&denied, None, Some("key1"))
            .await
            .expect("insert denied");
        db.insert_agent_run(&sample_report("other_key", "ws_1", "其他 key"), None, Some("key2"))
            .await
            .expect("insert other key");
        db.insert_agent_run(&sample_report("other_ws", "ws_2", "他区"), None, Some("key1"))
            .await
            .expect("insert other ws");

        let runs = db.recent_agent_runs_by_dedup_key("ws_1", "key1", 3).await.expect("recent");
        assert_eq!(runs.len(), 2, "只返回 key1 且只返回 ws_1");
        assert_eq!(runs[0].run_id, "denied");
        assert_eq!(runs[0].outcome, Outcome::Rejected);
        assert_eq!(runs[0].actions[0].action_name, "reboot");
        assert_eq!(runs[1].run_id, "acted");
        assert_eq!(runs[1].outcome, Outcome::Acted);

        assert!(
            db.recent_agent_runs_by_dedup_key("ws_1", "missing", 3)
                .await
                .expect("empty")
                .is_empty()
        );
    }

    #[tokio::test]
    pub async fn ack_run_is_idempotent() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());
        db.insert_agent_run(&sample_report("run_1", "ws_1", "s"), None, None)
            .await
            .expect("insert");

        assert!(db.ack_agent_run("run_1", "user_1").await.expect("first ack"));
        // 重复确认：false，且 acked_by 不被覆盖
        assert!(!db.ack_agent_run("run_1", "user_2").await.expect("second ack"));
        // 不存在的 run：false
        assert!(!db.ack_agent_run("run_missing", "user_1").await.expect("missing ack"));

        let (acked_by,): (String,) = sqlx::query_as("SELECT acked_by FROM agent_runs WHERE id = 'run_1'")
            .fetch_one(&pool)
            .await
            .expect("acked_by");
        assert_eq!(acked_by, "user_1");
    }

    #[tokio::test]
    pub async fn last_problem_run_respects_window_and_returns_flags() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        // 窗口外（-7h）：不得命中
        insert_raw(&pool, "old", "ws_1", "failed", Some("p1"), None, 0, "-7 hours").await;
        assert!(db.last_problem_agent_run("ws_1", "p1", 6).await.expect("query").is_none());

        // 窗口内两条：返回最新（-1h）的 (outcome, verified, acked)
        insert_raw(
            &pool,
            "older_in_window",
            "ws_1",
            "acted",
            Some("p1"),
            None,
            0,
            "-5 hours",
        )
        .await;
        insert_raw(
            &pool,
            "newest",
            "ws_1",
            "budget_exceeded",
            Some("p1"),
            None,
            0,
            "-1 hours",
        )
        .await;
        sqlx::query(
            "UPDATE agent_runs SET verified = 1, acked_at = datetime('now'), acked_by = 'u1' WHERE id = 'newest'",
        )
        .execute(&pool)
        .await
        .expect("ack newest");

        let (outcome, verified, acked) = db
            .last_problem_agent_run("ws_1", "p1", 6)
            .await
            .expect("query")
            .expect("found");
        assert_eq!(outcome, Outcome::BudgetExceeded);
        assert!(verified);
        assert!(acked);

        // 6h 边界：-5h59m 在内，-6h1m 在外
        insert_raw(&pool, "edge_in", "ws_1", "failed", Some("p2"), None, 0, "-359 minutes").await;
        insert_raw(&pool, "edge_out", "ws_1", "acted", Some("p2"), None, 0, "-361 minutes").await;
        let (outcome, ..) = db
            .last_problem_agent_run("ws_1", "p2", 6)
            .await
            .expect("query")
            .expect("edge row in window");
        assert_eq!(outcome, Outcome::Failed);

        // 其他 problem_key / 工作区互不影响
        assert!(db.last_problem_agent_run("ws_2", "p1", 6).await.expect("query").is_none());

        // 未知 outcome 字符串 fail-closed 到 Failed（T18 dedup 保守方向）
        insert_raw(&pool, "legacy", "ws_1", "success", Some("p3"), None, 0, "-1 hours").await;
        let (outcome, ..) = db
            .last_problem_agent_run("ws_1", "p3", 6)
            .await
            .expect("query")
            .expect("legacy row");
        assert_eq!(outcome, Outcome::Failed);
    }

    #[tokio::test]
    pub async fn count_problem_runs_respects_window_key_and_workspace() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        insert_raw(&pool, "in_1", "ws_1", "acted", Some("p1"), None, 0, "-1 hours").await;
        insert_raw(&pool, "in_2", "ws_1", "acted", Some("p1"), None, 0, "-5 hours").await;
        // 窗口外（-7h）不计入；6h 边界：-359min 在内，-361min 在外
        insert_raw(&pool, "out", "ws_1", "acted", Some("p1"), None, 0, "-7 hours").await;
        insert_raw(&pool, "edge_in", "ws_1", "acted", Some("p1"), None, 0, "-359 minutes").await;
        insert_raw(&pool, "edge_out", "ws_1", "acted", Some("p1"), None, 0, "-361 minutes").await;
        // 其他 problem_key / 工作区不计入
        insert_raw(&pool, "other_key", "ws_1", "acted", Some("p2"), None, 0, "-1 hours").await;
        insert_raw(&pool, "other_ws", "ws_2", "acted", Some("p1"), None, 0, "-1 hours").await;

        assert_eq!(db.count_problem_agent_runs("ws_1", "p1", 6).await.expect("count"), 3);
        assert_eq!(db.count_problem_agent_runs("ws_1", "p1", 8).await.expect("count"), 5);
        assert_eq!(db.count_problem_agent_runs("ws_1", "p2", 6).await.expect("count"), 1);
        assert_eq!(db.count_problem_agent_runs("ws_2", "p1", 6).await.expect("count"), 1);
        assert_eq!(db.count_problem_agent_runs("ws_1", "missing", 6).await.expect("count"), 0);
    }

    #[tokio::test]
    pub async fn interrupt_zombie_running_runs_marks_orphans_only() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        insert_raw(&pool, "ghost", "ws_1", "acted", None, None, 0, "-1 hours").await;
        insert_raw(&pool, "owned", "ws_1", "acted", None, None, 0, "-1 hours").await;
        insert_raw(&pool, "done", "ws_1", "acted", None, None, 0, "-1 hours").await;
        sqlx::query("UPDATE agent_runs SET status = 'running' WHERE id IN ('ghost', 'owned')")
            .execute(&pool)
            .await
            .expect("set running");

        let marked = db
            .interrupt_zombie_running_agent_runs(&["owned".to_string()])
            .await
            .expect("reconcile");
        assert_eq!(marked, 1);

        let status_of = |id: &str| {
            let pool = pool.clone();
            let id = id.to_string();
            async move {
                let (s,): (String,) = sqlx::query_as("SELECT status FROM agent_runs WHERE id = ?")
                    .bind(&id)
                    .fetch_one(&pool)
                    .await
                    .expect("status");
                s
            }
        };
        assert_eq!(status_of("ghost").await, "interrupted");
        assert_eq!(status_of("owned").await, "running", "registry 认领的 run 不判僵尸");
        assert_eq!(status_of("done").await, "completed", "completed 行不动");

        // 幂等：同一认领集再次 reconcile 无行可标。
        let marked = db
            .interrupt_zombie_running_agent_runs(&["owned".to_string()])
            .await
            .expect("reconcile again");
        assert_eq!(marked, 0);
        // 认领集为空时，残留的 running 行全部判僵尸。
        let marked = db.interrupt_zombie_running_agent_runs(&[]).await.expect("reconcile empty known");
        assert_eq!(marked, 1);
        assert_eq!(status_of("owned").await, "interrupted");
    }

    #[tokio::test]
    pub async fn agent_daily_cost_view_aggregates_by_workspace_and_day() {
        let pool = test_pool().await;
        let db = Db::new(pool.clone());

        let mut r1 = sample_report("run_1", "ws_1", "s1");
        r1.tokens = 100;
        r1.duration_ms = 10;
        let mut r2 = sample_report("run_2", "ws_1", "s2");
        r2.tokens = 200;
        r2.duration_ms = 20;
        db.insert_agent_run(&r1, None, None).await.expect("insert r1");
        db.insert_agent_run(&r2, None, None).await.expect("insert r2");
        // 昨天的 run 不进今天的聚合
        insert_raw(&pool, "yesterday", "ws_1", "acted", None, None, 999, "-1 days").await;

        let (runs, tokens, duration_ms): (i64, i64, i64) = sqlx::query_as(
            "SELECT runs, tokens, duration_ms FROM agent_daily_cost
             WHERE workspace_id = 'ws_1' AND day = date('now')",
        )
        .fetch_one(&pool)
        .await
        .expect("view row");
        assert_eq!(runs, 2);
        assert_eq!(tokens, 300);
        assert_eq!(duration_ms, 30);
    }
    #[test]
    fn run_report_json_round_trip() {
        let report = RunReport {
            run_id: "run_01".to_string(),
            workspace_id: "ws_01".to_string(),
            trigger: "thing:t1:event:temp_high".to_string(),
            outcome: Outcome::Acted,
            summary: "cooled down".to_string(),
            actions: vec![
                ActionRecord {
                    thing_id: "t1".to_string(),
                    action_name: "set_fan".to_string(),
                    params: serde_json::json!({"speed": 3}),
                    result: ActionResult::Success(serde_json::json!({"ok": true})),
                    verified: true,
                },
                ActionRecord {
                    thing_id: "t2".to_string(),
                    action_name: "reboot".to_string(),
                    params: serde_json::json!({}),
                    result: ActionResult::Failed("timeout".to_string()),
                    verified: false,
                },
                ActionRecord {
                    thing_id: "t3".to_string(),
                    action_name: "poll".to_string(),
                    params: serde_json::Value::Null,
                    result: ActionResult::UnknownCancelled,
                    verified: false,
                },
            ],
            verified: true,
            duration_ms: 1234,
            tool_calls: 5,
            tokens: 6789,
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let back: RunReport = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.run_id, report.run_id);
        assert_eq!(back.workspace_id, report.workspace_id);
        assert_eq!(back.trigger, report.trigger);
        assert_eq!(back.outcome, report.outcome);
        assert_eq!(back.summary, report.summary);
        assert_eq!(back.actions.len(), 3);
        assert_eq!(back.actions[0].thing_id, "t1");
        assert_eq!(back.actions[0].action_name, "set_fan");
        assert!(back.actions[0].verified);
        assert_eq!(back.verified, report.verified);
        assert_eq!(back.duration_ms, report.duration_ms);
        assert_eq!(back.tool_calls, report.tool_calls);
        assert_eq!(back.tokens, report.tokens);
    }

    #[test]
    fn outcome_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Outcome::BudgetExceeded).expect("serialize"),
            "\"budget_exceeded\""
        );
        assert_eq!(
            serde_json::to_string(&Outcome::NoActionNeeded).expect("serialize"),
            "\"no_action_needed\""
        );
        assert_eq!(serde_json::to_string(&Outcome::Acted).expect("serialize"), "\"acted\"");
    }
}

// ──────────────────────────────────────────────
// agent_tasks handler / persist 侧查询（自 cloud agent/host 迁入，Task 12）
// ──────────────────────────────────────────────

/// agent_runs 列表行（agent_tasks API）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunRow {
    pub id: String,
    pub trigger_type: String,
    pub trigger_context: Option<String>,
    pub outcome: String,
    pub summary: String,
    pub verified: bool,
    pub tool_calls: i64,
    pub tokens: i64,
    pub duration_ms: i64,
    pub acked_at: Option<String>,
    pub acked_by: Option<String>,
    pub created_at: String,
}

/// 检查 agent_run 是否存在。
pub(crate) async fn agent_run_exists(pool: &SqlitePool, run_id: &str) -> std::result::Result<bool, sqlx::Error> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await?;
    Ok(n > 0)
}

/// 分页列出 workspace 的 agent_runs。
pub(crate) async fn list_agent_run_rows(
    pool: &SqlitePool,
    workspace_id: &str,
    limit: i64,
    offset: i64,
) -> std::result::Result<Vec<AgentRunRow>, sqlx::Error> {
    sqlx::query_as::<_, AgentRunRow>(
        "SELECT id, trigger_type, trigger_context, outcome, summary, verified,                 tool_calls, tokens, duration_ms, acked_at, acked_by, created_at          FROM agent_runs WHERE workspace_id = ?          ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?",
    )
    .bind(workspace_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// 查 agent_run 的 (workspace_id, problem_key)。
pub(crate) async fn find_agent_run_owner(
    pool: &SqlitePool,
    run_id: &str,
) -> std::result::Result<Option<(String, Option<String>)>, sqlx::Error> {
    sqlx::query_as("SELECT workspace_id, problem_key FROM agent_runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(pool)
        .await
}

impl Db {
    /// 检查 agent_run 是否存在。
    pub async fn agent_run_exists(&self, run_id: &str) -> std::result::Result<bool, sqlx::Error> {
        agent_run_exists(self.pool(), run_id).await
    }

    /// 分页列出 workspace 的 agent_runs。
    pub async fn list_agent_run_rows(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> std::result::Result<Vec<AgentRunRow>, sqlx::Error> {
        list_agent_run_rows(self.pool(), workspace_id, limit, offset).await
    }

    /// 查 agent_run 的 (workspace_id, problem_key)。
    pub async fn find_agent_run_owner(
        &self,
        run_id: &str,
    ) -> std::result::Result<Option<(String, Option<String>)>, sqlx::Error> {
        find_agent_run_owner(self.pool(), run_id).await
    }
}

// ──────────────────────────────────────────────
// 启动快照查询（自 cloud bootstrap.rs 迁入，Task 12）
// ──────────────────────────────────────────────

/// 每工作区最近 N 条 run 的 report JSON，**旧→新**（prewarm 输入契约）。
pub(crate) async fn list_recent_agent_run_reports(
    pool: &SqlitePool,
    workspace_id: &str,
    limit: i64,
) -> std::result::Result<Vec<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT report FROM (
             SELECT report, created_at, rowid AS rid FROM agent_runs
             WHERE workspace_id = ?
             ORDER BY created_at DESC, rid DESC
             LIMIT ?
         ) ORDER BY created_at ASC, rid ASC",
    )
    .bind(workspace_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(json,)| json).collect())
}

/// O11 dedup 元数据行类型（7d 保留窗内 problem_key 行）。
pub type AgentProblemMetaTuple = (String, String, String, String, bool, Option<String>, String);

/// 7d 保留窗内的 problem_key 行（旧→新）。
pub(crate) async fn list_agent_problem_meta_rows(
    pool: &SqlitePool,
) -> std::result::Result<Vec<AgentProblemMetaTuple>, sqlx::Error> {
    sqlx::query_as(
        "SELECT workspace_id, problem_key, id, outcome, verified, acked_at, created_at
         FROM agent_runs
         WHERE problem_key IS NOT NULL AND created_at > datetime('now', '-7 days')
         ORDER BY created_at ASC, rowid ASC",
    )
    .fetch_all(pool)
    .await
}

impl Db {
    /// 每工作区最近 N 条 run 的 report JSON，**旧→新**。
    pub async fn list_recent_agent_run_reports(
        &self,
        workspace_id: &str,
        limit: i64,
    ) -> std::result::Result<Vec<String>, sqlx::Error> {
        list_recent_agent_run_reports(self.pool(), workspace_id, limit).await
    }

    /// 7d 保留窗内的 problem_key 行（旧→新）。
    pub async fn list_agent_problem_meta_rows(&self) -> std::result::Result<Vec<AgentProblemMetaTuple>, sqlx::Error> {
        list_agent_problem_meta_rows(self.pool()).await
    }
}
