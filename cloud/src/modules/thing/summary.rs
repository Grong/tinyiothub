// Summary module — LLM-based ontology summary for things.
// Implements lazy compute with dirty markers and single-flight dedup.

use std::{sync::Arc, time::Duration};

use dashmap::DashMap;
use sqlx::{Row, SqlitePool};
use tokio::sync::Notify;

// ──────────────────────────────────────────────
// Dirty marker triggers
// ──────────────────────────────────────────────

/// Called when a thing's resources change (attach/detach/update).
pub async fn mark_dirty_for_resource_change(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE devices SET summary_status = 'dirty' WHERE id = ?")
        .bind(thing_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Called when thing name/parent changes — dirty thing + entire subtree.
pub async fn mark_dirty_for_name_or_parent_change(
    pool: &SqlitePool,
    thing_id: &str,
) -> Result<u64, sqlx::Error> {
    // Use recursive CTE to find subtree
    let result = sqlx::query(
        "WITH RECURSIVE subtree AS (
            SELECT id FROM devices WHERE id = ?
            UNION ALL
            SELECT d.id FROM devices d JOIN subtree s ON d.parent_id = s.id
        )
        UPDATE devices SET summary_status = 'dirty' WHERE id IN (SELECT id FROM subtree)",
    )
    .bind(thing_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ──────────────────────────────────────────────
// LLM Client trait + stub
// ──────────────────────────────────────────────

/// Trait for LLM completion — allows mocking in tests.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, String>;
}

/// Stub LLM client that returns a placeholder summary.
/// Replace with a real LLM integration later.
pub struct StubLlmClient;

#[async_trait::async_trait]
impl LlmClient for StubLlmClient {
    async fn complete(&self, _prompt: &str, _max_tokens: u32) -> Result<String, String> {
        // Tiny delay to simulate a real LLM round-trip.
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok("这是一个IoT设备，具备数据采集和远程控制能力，支持实时监测与指令下发。".to_string())
    }
}

// ──────────────────────────────────────────────
// Summary computer with single-flight
// ──────────────────────────────────────────────

/// Orchestrates lazy summary computation with single-flight deduplication.
///
/// When multiple callers request a summary for the same `thing_id` concurrently,
/// only one LLM call is made; the others wait for it to finish and then re-read
/// the persisted result from the database.
pub struct SummaryComputer {
    single_flight: Arc<DashMap<String, Arc<Notify>>>,
}

/// Removes a single-flight entry and wakes its waiters when dropped.
///
/// Held for the duration of a summary computation so that EVERY exit path —
/// success, error via early `?`, or panic — releases waiters. Without this,
/// one transient error leaves the entry occupied forever and all future
/// summary requests for that thing hang on a Notify that never fires.
struct FlightGuard {
    map: Arc<DashMap<String, Arc<Notify>>>,
    thing_id: String,
    notify: Arc<Notify>,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        self.map.remove(&self.thing_id);
        self.notify.notify_waiters();
    }
}

impl Default for SummaryComputer {
    fn default() -> Self {
        Self::new()
    }
}

impl SummaryComputer {
    pub fn new() -> Self {
        Self { single_flight: Arc::new(DashMap::new()) }
    }

    /// Get or compute the ontology summary for a thing.
    ///
    /// - If `summary_status = 'ok'` and a summary exists, returns it immediately.
    /// - If `dirty` / `null` / `failed`, computes a fresh summary via LLM.
    /// - Uses a single-flight gate so overlapping requests share one computation.
    pub async fn get_or_compute(
        &self,
        thing_id: &str,
        pool: &SqlitePool,
        llm: &dyn LlmClient,
    ) -> Result<Option<String>, SummaryError> {
        // 1. Read current status
        let row = sqlx::query("SELECT ontology_summary, summary_status FROM devices WHERE id = ?")
            .bind(thing_id)
            .fetch_optional(pool)
            .await?;

        let (cached_summary, status): (Option<String>, Option<String>) = match row {
            Some(r) => (r.get(0), r.get(1)),
            None => return Ok(None),
        };

        // 2. If status is 'ok' and we have a summary, return cached
        if status.as_deref() == Some("ok") && cached_summary.is_some() {
            return Ok(cached_summary);
        }

        // 3. Single-flight gate
        let notify = {
            use dashmap::mapref::entry::Entry;
            match self.single_flight.entry(thing_id.to_string()) {
                Entry::Occupied(o) => {
                    // Already inflight — wait for it to complete.
                    // enable() registers this waiter BEFORE the entry lock is
                    // released, closing the lost-wakeup window where the
                    // computer finishes (and its notify_waiters fires) between
                    // drop(o) and the first poll of notified(). The timeout is
                    // a final backstop so a waiter can never hang forever.
                    let notifier = o.get().clone();
                    let wait = notifier.notified();
                    tokio::pin!(wait);
                    wait.as_mut().enable();
                    drop(o);
                    let _ = tokio::time::timeout(Duration::from_secs(30), wait).await;
                    // Re-read from DB after notification
                    let row = sqlx::query("SELECT ontology_summary FROM devices WHERE id = ?")
                        .bind(thing_id)
                        .fetch_optional(pool)
                        .await?;
                    return Ok(row.and_then(|r| r.get(0)));
                }
                Entry::Vacant(v) => {
                    let n = Arc::new(Notify::new());
                    v.insert(n.clone());
                    n
                }
            }
        };

        // RAII guard: releases the single-flight entry on every exit path.
        let _flight =
            FlightGuard { map: self.single_flight.clone(), thing_id: thing_id.to_string(), notify };

        // 4. Build prompt from thing metadata, model, and docs
        let prompt = build_prompt_for_thing(thing_id, pool).await?;

        // 5. Call LLM with 10s timeout
        let llm_start = std::time::Instant::now();
        let result =
            tokio::time::timeout(Duration::from_secs(10), llm.complete(&prompt, 500)).await;

        // 6. Handle result: persist or mark failed
        // (returned directly; the FlightGuard drops after evaluation, before
        // the value leaves this scope)
        match result {
            Ok(Ok(text)) => {
                // Success: persist summary and mark status 'ok'
                sqlx::query(
                    "UPDATE devices SET ontology_summary = ?, summary_status = 'ok', \
                     updated_at = datetime('now') WHERE id = ?",
                )
                .bind(&text)
                .bind(thing_id)
                .execute(pool)
                .await?;
                tracing::info!(
                    thing_id = %thing_id,
                    duration_ms = llm_start.elapsed().as_millis() as i64,
                    metric = "summary_success",
                    "Ontology summary computed"
                );
                Ok(Some(text))
            }
            Ok(Err(e)) => {
                tracing::warn!(?e, thing_id = %thing_id, metric = "summary_failed", "LLM call failed");
                // Mark status as 'failed', keep whatever was cached
                sqlx::query(
                    "UPDATE devices SET summary_status = 'failed', \
                     updated_at = datetime('now') WHERE id = ?",
                )
                .bind(thing_id)
                .execute(pool)
                .await?;
                Ok(cached_summary)
            }
            Err(_elapsed) => {
                tracing::warn!(thing_id = %thing_id, metric = "summary_failed", reason = "timeout", "LLM call timed out");
                // Mark status as 'failed', keep whatever was cached
                sqlx::query(
                    "UPDATE devices SET summary_status = 'failed', \
                     updated_at = datetime('now') WHERE id = ?",
                )
                .bind(thing_id)
                .execute(pool)
                .await?;
                Ok(cached_summary)
            }
        }
    }
}
// ──────────────────────────────────────────────
// Prompt building
// ──────────────────────────────────────────────

/// Build a summary prompt with XML document fencing.
///
/// The `<user_document>` tags fence untrusted document content from the
/// system prompt instructions, following best practices for LLM safety.
pub fn build_prompt(
    thing_name: &str,
    thing_type: &str,
    breadcrumb: &str,
    template_def: &str,
    docs: &[(String, String)],
) -> String {
    let mut prompt = format!(
        "你是 IoT 物本体专家。请为该物写一段 ≤500 字的中文摘要，描述它是什么、有什么能力、关联哪些知识：\n\
         物名称: {}\n类型: {}\n路径: {}\n\n物模型:\n{}\n",
        thing_name, thing_type, breadcrumb, template_def
    );
    for (title, content) in docs.iter().take(5) {
        // chars(), not byte slicing — the corpus is Chinese (3 bytes/char)
        // and a byte cut on a non-boundary panics.
        let snippet: String = content.chars().take(2000).collect();
        prompt.push_str(&format!(
            "\n<user_document title=\"{}\">\n{}\n</user_document>\n",
            title, snippet
        ));
    }
    prompt
}

// ──────────────────────────────────────────────
// DB query helpers (not public)
// ──────────────────────────────────────────────

/// Fetch thing metadata and assemble a prompt from DB.
async fn build_prompt_for_thing(thing_id: &str, pool: &SqlitePool) -> Result<String, SummaryError> {
    // Fetch thing basic info
    let thing_row = sqlx::query("SELECT name, thing_type FROM devices WHERE id = ?")
        .bind(thing_id)
        .fetch_optional(pool)
        .await?;

    let (name, thing_type): (String, String) = match thing_row {
        Some(r) => (r.get(0), r.get(1)),
        None => return Ok("未找到该物。".to_string()),
    };

    // Build breadcrumb
    let breadcrumb = build_breadcrumb_string(thing_id, pool).await?;

    // Blueprint model (eng-review D6, 2026-07-27): the thing's model is its
    // own property/action instances. The creation template is not queried at
    // runtime.
    let model_def = fetch_thing_model_definition(thing_id, pool).await;

    // Fetch knowledge docs (max 5)
    let docs = fetch_knowledge_docs(thing_id, pool).await;

    Ok(build_prompt(&name, &thing_type, &breadcrumb, &model_def, &docs))
}

async fn build_breadcrumb_string(
    thing_id: &str,
    pool: &SqlitePool,
) -> Result<String, SummaryError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "WITH RECURSIVE ancestors AS (
            SELECT id, name, parent_id, 0 AS depth FROM devices WHERE id = ?
            UNION ALL
            SELECT d.id, d.name, d.parent_id, a.depth + 1
            FROM devices d JOIN ancestors a ON d.id = a.parent_id
            WHERE a.depth < 10
        ) SELECT name FROM ancestors ORDER BY depth DESC",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await?;

    let path: Vec<String> = rows.into_iter().map(|(name,)| name).collect();
    Ok(path.join(" / "))
}

/// Build the model definition string from the thing's OWN property/action
/// instances (blueprint model — templates are creation-time blueprints only).
async fn fetch_thing_model_definition(thing_id: &str, pool: &SqlitePool) -> String {
    let props: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM thing_properties WHERE device_id = ? ORDER BY name")
            .bind(thing_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    let acts: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM thing_actions WHERE device_id = ? ORDER BY name")
            .bind(thing_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let mut def = String::new();
    if !props.is_empty() {
        let names: Vec<&str> = props.iter().map(|(n,)| n.as_str()).collect();
        def.push_str(&format!("属性: {}\n", names.join(", ")));
    }
    if !acts.is_empty() {
        let names: Vec<&str> = acts.iter().map(|(n,)| n.as_str()).collect();
        def.push_str(&format!("动作: {}\n", names.join(", ")));
    }
    if def.is_empty() {
        def.push_str("无物模型定义。");
    }
    def
}

async fn fetch_knowledge_docs(thing_id: &str, pool: &SqlitePool) -> Vec<(String, String)> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, content FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT 5",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(|(name, content)| (name, content.unwrap_or_default())).collect()
}

// ──────────────────────────────────────────────
// Error type
// ──────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum SummaryError {
    #[error("Database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("LLM timeout")]
    Timeout,
    #[error("LLM error: {0}")]
    LlmError(String),
}

// ──────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    async fn setup_test_db(pool: &SqlitePool) {
        // sqlx::test runs the full migration chain, so the production schema
        // is present. Disable FK enforcement while replacing it with the
        // simplified test schema: with FK on, DROP TABLE runs an implicit
        // delete + schema revalidation that deadlocks against the
        // device_alarm_rules → thing_properties → devices reference chain.
        // The PRAGMA is per-connection, so the drops must run on the SAME
        // acquired connection — a fresh pool connection would have FK on.
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF").execute(&mut *conn).await.unwrap();
        for table in
            ["device_alarm_rules", "resources", "thing_properties", "thing_actions", "devices"]
        {
            sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {}", table)))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        drop(conn);

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                thing_type TEXT NOT NULL DEFAULT 'device',
                template_id TEXT,
                parent_id TEXT,
                ontology_summary TEXT,
                summary_status TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS thing_properties (
                id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS thing_actions (
                id TEXT PRIMARY KEY,
                device_id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS resources (
                id TEXT PRIMARY KEY,
                device_id TEXT,
                name TEXT NOT NULL,
                content TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    // ── Dirty marker tests ──────────────────────

    #[sqlx::test]
    async fn test_summary_dirty_on_resource_attach(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query("INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', NULL)")
            .execute(&pool)
            .await
            .unwrap();

        mark_dirty_for_resource_change(&pool, "d1").await.unwrap();

        let status: String =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'd1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "dirty");
    }

    #[sqlx::test]
    async fn test_summary_dirty_subtree(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, parent_id) \
             VALUES ('root', 'Root', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, parent_id) \
             VALUES ('c1', 'Child1', NULL, 'root')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, parent_id) \
             VALUES ('c2', 'Child2', NULL, 'c1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, parent_id) \
             VALUES ('other', 'Other', NULL, NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let affected = mark_dirty_for_name_or_parent_change(&pool, "root").await.unwrap();
        assert_eq!(affected, 3); // root + c1 + c2

        // 'other' should NOT be dirtied
        let other_status: Option<String> =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'other'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(other_status, None);
    }

    // ── Prompt building test ────────────────────

    #[test]
    fn test_build_prompt_fencing() {
        let prompt = build_prompt(
            "温度传感器A",
            "device",
            "工厂1 / 产线A / 温度传感器A",
            "属性: temperature, humidity\n命令: reboot",
            &[("传感器手册".to_string(), "这是一份很长的文档...".to_string())],
        );
        assert!(prompt.contains("温度传感器A"));
        assert!(prompt.contains("<user_document"));
        assert!(prompt.contains("</user_document>"));
        assert!(prompt.contains("IoT 物本体专家"));
    }

    #[test]
    fn test_build_prompt_utf8_boundary() {
        // Regression: byte-slicing `&content[..2000]` panics when byte 2000 is
        // not a char boundary. Chinese text is 3 bytes/char in UTF-8.
        let long_doc = "温".repeat(2500); // 2500 chars = 7500 bytes
        let prompt = build_prompt(
            "传感器A",
            "device",
            "工厂 / 传感器A",
            "属性: temperature",
            &[("手册".to_string(), long_doc)],
        );
        // Must not panic, and the fenced snippet must be capped at 2000 chars.
        let start = prompt.find("<user_document").unwrap();
        let end = prompt.find("</user_document>").unwrap();
        let section = &prompt[start..end];
        assert!(section.chars().count() <= 2100); // 2000 + title/tag overhead
    }

    #[sqlx::test]
    async fn test_single_flight_entry_released_on_error(pool: SqlitePool) {
        // Regression: an early `?` return after the single-flight gate (e.g.
        // the summary-persist UPDATE failing) used to leave the entry
        // occupied forever — every later request for the thing hung.
        setup_test_db(&pool).await;
        sqlx::query(
            "INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', 'dirty')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // LLM that closes the pool inside complete(): the subsequent
        // persist UPDATE fails, forcing the early-error exit path.
        struct PoolClosingClient(SqlitePool);
        #[async_trait::async_trait]
        impl LlmClient for PoolClosingClient {
            async fn complete(&self, _p: &str, _t: u32) -> Result<String, String> {
                self.0.close().await;
                Ok("摘要".to_string())
            }
        }

        let computer = SummaryComputer::new();
        let llm = PoolClosingClient(pool.clone());
        let result = computer.get_or_compute("d1", &pool, &llm).await;
        assert!(result.is_err());

        // The single-flight entry must be gone — no permanent deadlock.
        assert!(computer.single_flight.is_empty());
    }

    // ── get_or_compute tests ────────────────────

    #[sqlx::test]
    async fn test_summary_get_or_compute_cached(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query(
            "INSERT INTO devices (id, name, ontology_summary, summary_status) \
             VALUES ('d1', 'Test', '已有摘要', 'ok')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let computer = SummaryComputer::new();
        let llm = StubLlmClient;
        let result = computer.get_or_compute("d1", &pool, &llm).await.unwrap();

        assert_eq!(result, Some("已有摘要".to_string()));
    }

    #[sqlx::test]
    async fn test_summary_get_or_compute_computes(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query(
            "INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', 'dirty')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let computer = SummaryComputer::new();
        let llm = StubLlmClient;
        let result = computer.get_or_compute("d1", &pool, &llm).await.unwrap();

        assert!(result.is_some());
        assert!(!result.as_ref().unwrap().is_empty());

        // Verify status updated to 'ok'
        let status: String =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'd1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "ok");
    }

    #[sqlx::test]
    async fn test_summary_get_or_compute_not_found(pool: SqlitePool) {
        setup_test_db(&pool).await;

        let computer = SummaryComputer::new();
        let llm = StubLlmClient;
        let result = computer.get_or_compute("nonexistent", &pool, &llm).await.unwrap();

        assert_eq!(result, None);
    }

    #[sqlx::test]
    async fn test_summary_single_flight(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query(
            "INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', 'dirty')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let computer = Arc::new(SummaryComputer::new());
        let call_count = Arc::new(AtomicU32::new(0));

        // Counting client: slow first call so single-flight is observable
        struct CountingClient {
            count: Arc<AtomicU32>,
            delay_ms: u64,
        }

        #[async_trait::async_trait]
        impl LlmClient for CountingClient {
            async fn complete(&self, _p: &str, _t: u32) -> Result<String, String> {
                self.count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
                Ok(format!("摘要版本{}", self.count.load(Ordering::SeqCst)))
            }
        }

        // Spawn first computation with a slow client
        let pool1 = pool.clone();
        let comp1 = computer.clone();
        let cc1 = CountingClient { count: call_count.clone(), delay_ms: 200 };
        let h = tokio::spawn(async move { comp1.get_or_compute("d1", &pool1, &cc1).await });

        // Give the spawned task a head start to acquire the single-flight lock
        tokio::time::sleep(Duration::from_millis(30)).await;

        // Second call: should wait on the single-flight Notify and get the
        // same result without calling the LLM again.
        let cc2 = CountingClient { count: call_count.clone(), delay_ms: 0 };
        let result2 = computer.get_or_compute("d1", &pool, &cc2).await.unwrap();

        let result1 = h.await.unwrap().unwrap();

        // Both callers should get the same summary
        assert_eq!(result1, result2);
        assert!(result1.is_some());

        // LLM should have been called exactly once (by the first caller)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[sqlx::test]
    async fn test_summary_llm_failure_returns_stale_and_marks_failed(pool: SqlitePool) {
        // Design 二·③/六: LLM failure → return the OLD summary (stale
        // degradation), summary_status='failed', no error to the caller.
        setup_test_db(&pool).await;
        sqlx::query(
            "INSERT INTO devices (id, name, ontology_summary, summary_status)              VALUES ('d1', 'Test', '旧摘要', 'dirty')",
        )
        .execute(&pool)
        .await
        .unwrap();

        struct FailingClient;
        #[async_trait::async_trait]
        impl LlmClient for FailingClient {
            async fn complete(&self, _p: &str, _t: u32) -> Result<String, String> {
                Err("LLM unavailable".to_string())
            }
        }

        let computer = SummaryComputer::new();
        let result = computer.get_or_compute("d1", &pool, &FailingClient).await.unwrap();

        // Stale summary returned, status marked failed
        assert_eq!(result, Some("旧摘要".to_string()));
        let status: String =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'd1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "failed");

        // The single-flight entry is released (no deadlock after failure)
        assert!(computer.single_flight.is_empty());
    }

    #[sqlx::test]
    async fn test_summary_with_model_and_docs(pool: SqlitePool) {
        setup_test_db(&pool).await;

        // Thing with its own property/action instances (blueprint model)
        sqlx::query(
            "INSERT INTO devices (id, name, thing_type, summary_status) \
             VALUES ('d1', '传感器A', 'device', 'dirty')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO thing_properties (id, device_id, name) VALUES ('p1', 'd1', 'temperature')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO thing_actions (id, device_id, name) VALUES ('a1', 'd1', 'reboot')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a knowledge doc
        sqlx::query(
            "INSERT INTO resources (id, device_id, name, content) \
             VALUES ('r1', 'd1', '安装手册', '安装步骤说明...')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let computer = SummaryComputer::new();
        let llm = StubLlmClient;
        let result = computer.get_or_compute("d1", &pool, &llm).await.unwrap();

        assert!(result.is_some());

        let status: String =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'd1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "ok");
    }
}
