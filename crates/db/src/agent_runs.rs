//! Agent runs 持久化：自治 run 报告（P-集中化 E6b，自 agent crate 迁入）。
//!
//! 类型随 repo 住 db（方案 B）：RunReport/Outcome/ActionRecord 为 DB 行类型
//! （report JSON 列的序列化格式在此定义）；agent crate 经 re-export 兼容。

use sqlx::SqlitePool;

use crate::error::Result;

// ──────────────────────────────────────────────
// 持久化类型（DB 行）— 自 agent/loop_/thing_agent 迁入
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Acted,
    NoActionNeeded,
    Failed,
    BudgetExceeded,
    Rejected,
}

impl Outcome {
    /// DB/metric 字符串（snake_case，与 serde 表示一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Acted => "acted",
            Outcome::NoActionNeeded => "no_action_needed",
            Outcome::Failed => "failed",
            Outcome::BudgetExceeded => "budget_exceeded",
            Outcome::Rejected => "rejected",
        }
    }

    /// 从 DB 字符串解析；未知值 None（调用方 fail-closed）。
    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "acted" => Outcome::Acted,
            "no_action_needed" => Outcome::NoActionNeeded,
            "failed" => Outcome::Failed,
            "budget_exceeded" => Outcome::BudgetExceeded,
            "rejected" => Outcome::Rejected,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionRecord {
    pub thing_id: String,
    pub action_name: String,
    pub params: serde_json::Value,
    pub result: ActionResult,
    pub verified: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionResult {
    Success(serde_json::Value),
    Failed(String),
    UnknownCancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub workspace_id: String,
    pub trigger: String, // TriggerSource 的序列化
    pub outcome: Outcome,
    pub summary: String,
    pub actions: Vec<ActionRecord>,
    pub verified: bool,
    pub duration_ms: u64,
    pub tool_calls: u32,
    pub tokens: u64,
}

/// 记忆/历史段统一条目格式：`"[acted] 调低设定值成功"`。
pub fn format_summary(outcome: &str, summary: &str) -> String {
    format!("[{outcome}] {summary}")
}

// ──────────────────────────────────────────────
// Repository
// ──────────────────────────────────────────────

pub struct AgentRunsRepository {
    pool: SqlitePool,
}

impl AgentRunsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// trigger 串前缀作为 trigger_type（"thing:t1:event:x" → "thing"）；无 ':'
/// 时用整串。
fn trigger_type_of(trigger: &str) -> &str {
    trigger.split(':').next().unwrap_or(trigger)
}

impl AgentRunsRepository {
    pub async fn insert_run(
        &self,
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
        .execute(&self.pool)
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

    pub async fn recent_summaries(&self, workspace_id: &str, limit: u32) -> Result<Vec<String>> {
        // rowid 决胜：created_at 秒级精度，同秒批量插入仍保持插入序。
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT outcome, summary FROM agent_runs
             WHERE workspace_id = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
        )
        .bind(workspace_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(o, s)| format_summary(&o, &s)).collect())
    }

    pub async fn history_by_dedup_key(&self, workspace_id: &str, key: &str, limit: u32) -> Result<Vec<String>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT outcome, summary FROM agent_runs
             WHERE workspace_id = ? AND dedup_key = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
        )
        .bind(workspace_id)
        .bind(key)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(o, s)| format_summary(&o, &s)).collect())
    }

    pub async fn recent_runs_by_dedup_key(&self, workspace_id: &str, key: &str, limit: u32) -> Result<Vec<RunReport>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT report FROM agent_runs
             WHERE workspace_id = ? AND dedup_key = ?
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?",
        )
        .bind(workspace_id)
        .bind(key)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(json,)| serde_json::from_str::<RunReport>(&json).map_err(Into::into))
            .collect()
    }

    pub async fn ack_run(&self, run_id: &str, actor: &str) -> Result<bool> {
        // 幂等：仅首认生效（acked_at IS NULL），重复确认/不存在 rows_affected = 0。
        let result = sqlx::query(
            "UPDATE agent_runs SET acked_at = datetime('now'), acked_by = ?
             WHERE id = ? AND acked_at IS NULL",
        )
        .bind(actor)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn last_problem_run(
        &self,
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
        .fetch_optional(&self.pool)
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

    pub async fn count_problem_runs(&self, workspace_id: &str, problem_key: &str, since_hours: u32) -> Result<u32> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_runs
             WHERE workspace_id = ? AND problem_key = ?
               AND created_at > datetime('now', ?)",
        )
        .bind(workspace_id)
        .bind(problem_key)
        .bind(format!("-{since_hours} hours"))
        .fetch_one(&self.pool)
        .await?;
        Ok(n as u32)
    }
}

#[cfg(test)]
mod tests {
    use crate::agent_runs::{ActionRecord, ActionResult};
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    pub async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory sqlite");
        let migration = include_str!("../migrations/20260729000001_thing_agent_loop.sql");
        for stmt in migration.split(';') {
            let stmt = stmt.trim();
            // Skip the events ALTER — the events table is not part of this pool.
            if !stmt.is_empty() && !stmt.starts_with("ALTER TABLE") {
                sqlx::query(stmt).execute(&pool).await.expect("apply migration");
            }
        }
        pool
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
        let repo = AgentRunsRepository::new(pool.clone());

        let report = sample_report("run_1", "ws_1", "调低设定值成功");
        repo.insert_run(&report, Some("temp_high:t1"), Some("thing:t1:event:temp_high"))
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
        let repo = AgentRunsRepository::new(pool.clone());

        for i in 1..=7 {
            let mut report = sample_report(&format!("run_{i}"), "ws_1", &format!("摘要{i}"));
            if i == 3 {
                report.outcome = Outcome::Failed;
            }
            repo.insert_run(&report, None, None).await.expect("insert");
        }
        // 其他工作区不混入
        repo.insert_run(&sample_report("run_other", "ws_2", "别的"), None, None)
            .await
            .expect("insert other ws");

        let summaries = repo.recent_summaries("ws_1", 5).await.expect("recent");
        assert_eq!(summaries.len(), 5);
        // 最新在前（run_7 → run_3），含 outcome 前缀
        assert_eq!(summaries[0], "[acted] 摘要7");
        assert_eq!(summaries[4], "[failed] 摘要3");
        assert!(!summaries.iter().any(|s| s.contains("摘要1") || s.contains("摘要2")));
    }

    #[tokio::test]
    pub async fn history_by_dedup_key_filters_and_caps() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());

        for i in 1..=4 {
            repo.insert_run(
                &sample_report(&format!("k1_{i}"), "ws_1", &format!("同类{i}")),
                None,
                Some("key1"),
            )
            .await
            .expect("insert");
        }
        repo.insert_run(&sample_report("k2_1", "ws_1", "另一类"), None, Some("key2"))
            .await
            .expect("insert");
        repo.insert_run(&sample_report("k1_other_ws", "ws_2", "他区"), None, Some("key1"))
            .await
            .expect("insert");

        let history = repo.history_by_dedup_key("ws_1", "key1", 3).await.expect("history");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0], "[acted] 同类4");
        assert_eq!(history[2], "[acted] 同类2");
        assert!(!history.iter().any(|s| s.contains("另一类") || s.contains("他区")));

        assert!(
            repo.history_by_dedup_key("ws_1", "missing", 3)
                .await
                .expect("empty")
                .is_empty()
        );
    }

    #[tokio::test]
    pub async fn recent_runs_by_dedup_key_returns_parsed_reports_newest_first() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());

        repo.insert_run(&sample_report("acted", "ws_1", "已处理"), None, Some("key1"))
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
        repo.insert_run(&denied, None, Some("key1"))
            .await
            .expect("insert denied");
        repo.insert_run(&sample_report("other_key", "ws_1", "其他 key"), None, Some("key2"))
            .await
            .expect("insert other key");
        repo.insert_run(&sample_report("other_ws", "ws_2", "他区"), None, Some("key1"))
            .await
            .expect("insert other ws");

        let runs = repo.recent_runs_by_dedup_key("ws_1", "key1", 3).await.expect("recent");
        assert_eq!(runs.len(), 2, "只返回 key1 且只返回 ws_1");
        assert_eq!(runs[0].run_id, "denied");
        assert_eq!(runs[0].outcome, Outcome::Rejected);
        assert_eq!(runs[0].actions[0].action_name, "reboot");
        assert_eq!(runs[1].run_id, "acted");
        assert_eq!(runs[1].outcome, Outcome::Acted);

        assert!(
            repo.recent_runs_by_dedup_key("ws_1", "missing", 3)
                .await
                .expect("empty")
                .is_empty()
        );
    }

    #[tokio::test]
    pub async fn ack_run_is_idempotent() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());
        repo.insert_run(&sample_report("run_1", "ws_1", "s"), None, None)
            .await
            .expect("insert");

        assert!(repo.ack_run("run_1", "user_1").await.expect("first ack"));
        // 重复确认：false，且 acked_by 不被覆盖
        assert!(!repo.ack_run("run_1", "user_2").await.expect("second ack"));
        // 不存在的 run：false
        assert!(!repo.ack_run("run_missing", "user_1").await.expect("missing ack"));

        let (acked_by,): (String,) = sqlx::query_as("SELECT acked_by FROM agent_runs WHERE id = 'run_1'")
            .fetch_one(&pool)
            .await
            .expect("acked_by");
        assert_eq!(acked_by, "user_1");
    }

    #[tokio::test]
    pub async fn last_problem_run_respects_window_and_returns_flags() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());

        // 窗口外（-7h）：不得命中
        insert_raw(&pool, "old", "ws_1", "failed", Some("p1"), None, 0, "-7 hours").await;
        assert!(repo.last_problem_run("ws_1", "p1", 6).await.expect("query").is_none());

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

        let (outcome, verified, acked) = repo
            .last_problem_run("ws_1", "p1", 6)
            .await
            .expect("query")
            .expect("found");
        assert_eq!(outcome, Outcome::BudgetExceeded);
        assert!(verified);
        assert!(acked);

        // 6h 边界：-5h59m 在内，-6h1m 在外
        insert_raw(&pool, "edge_in", "ws_1", "failed", Some("p2"), None, 0, "-359 minutes").await;
        insert_raw(&pool, "edge_out", "ws_1", "acted", Some("p2"), None, 0, "-361 minutes").await;
        let (outcome, ..) = repo
            .last_problem_run("ws_1", "p2", 6)
            .await
            .expect("query")
            .expect("edge row in window");
        assert_eq!(outcome, Outcome::Failed);

        // 其他 problem_key / 工作区互不影响
        assert!(repo.last_problem_run("ws_2", "p1", 6).await.expect("query").is_none());

        // 未知 outcome 字符串 fail-closed 到 Failed（T18 dedup 保守方向）
        insert_raw(&pool, "legacy", "ws_1", "success", Some("p3"), None, 0, "-1 hours").await;
        let (outcome, ..) = repo
            .last_problem_run("ws_1", "p3", 6)
            .await
            .expect("query")
            .expect("legacy row");
        assert_eq!(outcome, Outcome::Failed);
    }

    #[tokio::test]
    pub async fn count_problem_runs_respects_window_key_and_workspace() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());

        insert_raw(&pool, "in_1", "ws_1", "acted", Some("p1"), None, 0, "-1 hours").await;
        insert_raw(&pool, "in_2", "ws_1", "acted", Some("p1"), None, 0, "-5 hours").await;
        // 窗口外（-7h）不计入；6h 边界：-359min 在内，-361min 在外
        insert_raw(&pool, "out", "ws_1", "acted", Some("p1"), None, 0, "-7 hours").await;
        insert_raw(&pool, "edge_in", "ws_1", "acted", Some("p1"), None, 0, "-359 minutes").await;
        insert_raw(&pool, "edge_out", "ws_1", "acted", Some("p1"), None, 0, "-361 minutes").await;
        // 其他 problem_key / 工作区不计入
        insert_raw(&pool, "other_key", "ws_1", "acted", Some("p2"), None, 0, "-1 hours").await;
        insert_raw(&pool, "other_ws", "ws_2", "acted", Some("p1"), None, 0, "-1 hours").await;

        assert_eq!(repo.count_problem_runs("ws_1", "p1", 6).await.expect("count"), 3);
        assert_eq!(repo.count_problem_runs("ws_1", "p1", 8).await.expect("count"), 5);
        assert_eq!(repo.count_problem_runs("ws_1", "p2", 6).await.expect("count"), 1);
        assert_eq!(repo.count_problem_runs("ws_2", "p1", 6).await.expect("count"), 1);
        assert_eq!(repo.count_problem_runs("ws_1", "missing", 6).await.expect("count"), 0);
    }

    #[tokio::test]
    pub async fn agent_daily_cost_view_aggregates_by_workspace_and_day() {
        let pool = test_pool().await;
        let repo = AgentRunsRepository::new(pool.clone());

        let mut r1 = sample_report("run_1", "ws_1", "s1");
        r1.tokens = 100;
        r1.duration_ms = 10;
        let mut r2 = sample_report("run_2", "ws_1", "s2");
        r2.tokens = 200;
        r2.duration_ms = 20;
        repo.insert_run(&r1, None, None).await.expect("insert r1");
        repo.insert_run(&r2, None, None).await.expect("insert r2");
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
