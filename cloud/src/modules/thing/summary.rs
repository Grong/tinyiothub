// Summary module — LLM-based ontology summary for things.
// Implements lazy compute with dirty markers and single-flight dedup.

use std::sync::Arc;
use std::time::Duration;

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

/// Called when a template changes — dirty ALL things using that template.
pub async fn mark_dirty_for_template_change(
    pool: &SqlitePool,
    template_id: &str,
) -> Result<u64, sqlx::Error> {
    let result =
        sqlx::query("UPDATE devices SET summary_status = 'dirty' WHERE template_id = ?")
            .bind(template_id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected())
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

impl SummaryComputer {
    pub fn new() -> Self {
        Self {
            single_flight: Arc::new(DashMap::new()),
        }
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
        let row = sqlx::query(
            "SELECT ontology_summary, summary_status FROM devices WHERE id = ?",
        )
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
                    // Already inflight — wait for it to complete
                    let notifier = o.get().clone();
                    drop(o);
                    notifier.notified().await;
                    // Re-read from DB after notification
                    let row = sqlx::query(
                        "SELECT ontology_summary FROM devices WHERE id = ?",
                    )
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

        // 4. Build prompt from thing metadata, template, and docs
        let prompt = build_prompt_for_thing(thing_id, pool).await?;

        // 5. Call LLM with 10s timeout
        let result = tokio::time::timeout(Duration::from_secs(10), llm.complete(&prompt, 500))
            .await;

        // 6. Handle result: persist or mark failed
        let outcome = match result {
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
                Ok(Some(text))
            }
            Ok(Err(e)) => {
                tracing::warn!(?e, thing_id = %thing_id, "LLM call failed");
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
                tracing::warn!(thing_id = %thing_id, "LLM call timed out");
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
        };

        // Clean up single-flight entry and notify waiters
        self.single_flight.remove(thing_id);
        notify.notify_waiters();

        outcome
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
    for (_i, (title, content)) in docs.iter().take(5).enumerate() {
        let snippet = &content[..content.len().min(2000)];
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
async fn build_prompt_for_thing(
    thing_id: &str,
    pool: &SqlitePool,
) -> Result<String, SummaryError> {
    // Fetch thing basic info
    let thing_row = sqlx::query(
        "SELECT name, thing_type, template_id FROM devices WHERE id = ?",
    )
    .bind(thing_id)
    .fetch_optional(pool)
    .await?;

    let (name, thing_type, template_id): (String, String, Option<String>) = match thing_row {
        Some(r) => (r.get(0), r.get(1), r.get(2)),
        None => return Ok("未找到该物。".to_string()),
    };

    // Build breadcrumb
    let breadcrumb = build_breadcrumb_string(thing_id, pool).await?;

    // Fetch template definition if available
    let template_def = if let Some(ref tid) = template_id {
        fetch_template_definition(tid, pool).await.unwrap_or_default()
    } else {
        String::from("无物模型定义。")
    };

    // Fetch knowledge docs (max 5)
    let docs = fetch_knowledge_docs(thing_id, pool).await;

    Ok(build_prompt(&name, &thing_type, &breadcrumb, &template_def, &docs))
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

async fn fetch_template_definition(
    template_id: &str,
    pool: &SqlitePool,
) -> Result<String, SummaryError> {
    let row = sqlx::query(
        "SELECT name, description, properties, actions FROM thing_templates WHERE id = ?",
    )
    .bind(template_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => {
            let name: String = r.get(0);
            let desc: Option<String> = r.get(1);
            let props: String = r.get(2);
            let cmds: String = r.get(3);

            let mut def = format!("模板: {}\n", name);
            if let Some(d) = desc {
                def.push_str(&format!("描述: {}\n", d));
            }
            // Parse JSON arrays for a readable capabilities summary
            if let Ok(parsed_props) =
                serde_json::from_str::<Vec<serde_json::Value>>(&props)
            {
                let prop_names: Vec<String> = parsed_props
                    .iter()
                    .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect();
                if !prop_names.is_empty() {
                    def.push_str(&format!("属性: {}\n", prop_names.join(", ")));
                }
            }
            if let Ok(parsed_cmds) =
                serde_json::from_str::<Vec<serde_json::Value>>(&cmds)
            {
                let cmd_names: Vec<String> = parsed_cmds
                    .iter()
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                    .collect();
                if !cmd_names.is_empty() {
                    def.push_str(&format!("命令: {}\n", cmd_names.join(", ")));
                }
            }
            Ok(def)
        }
        None => Ok(String::from("无物模型定义。")),
    }
}

async fn fetch_knowledge_docs(
    thing_id: &str,
    pool: &SqlitePool,
) -> Vec<(String, String)> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, content FROM resources WHERE device_id = ? ORDER BY created_at DESC LIMIT 5",
    )
    .bind(thing_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|(name, content)| (name, content.unwrap_or_default()))
        .collect()
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
        sqlx::query("DROP TABLE IF EXISTS resources")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS thing_templates")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DROP TABLE IF EXISTS devices")
            .execute(pool)
            .await
            .unwrap();

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
            "CREATE TABLE IF NOT EXISTS thing_templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                properties TEXT NOT NULL DEFAULT '[]',
                actions TEXT NOT NULL DEFAULT '[]',
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
    async fn test_summary_dirty_on_template_change(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, template_id) \
             VALUES ('d1', 'Test', NULL, 't1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO devices (id, name, summary_status, template_id) \
             VALUES ('d2', 'Test2', NULL, 't1')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let affected = mark_dirty_for_template_change(&pool, "t1").await.unwrap();
        assert_eq!(affected, 2);

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM devices WHERE summary_status = 'dirty' AND template_id = 't1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 2);
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

        let affected =
            mark_dirty_for_name_or_parent_change(&pool, "root").await.unwrap();
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
            &[(
                "传感器手册".to_string(),
                "这是一份很长的文档...".to_string(),
            )],
        );
        assert!(prompt.contains("温度传感器A"));
        assert!(prompt.contains("<user_document"));
        assert!(prompt.contains("</user_document>"));
        assert!(prompt.contains("IoT 物本体专家"));
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
        let result = computer
            .get_or_compute("d1", &pool, &llm)
            .await
            .unwrap();

        assert_eq!(result, Some("已有摘要".to_string()));
    }

    #[sqlx::test]
    async fn test_summary_get_or_compute_computes(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query("INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', 'dirty')")
            .execute(&pool)
            .await
            .unwrap();

        let computer = SummaryComputer::new();
        let llm = StubLlmClient;
        let result = computer
            .get_or_compute("d1", &pool, &llm)
            .await
            .unwrap();

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
        let result = computer
            .get_or_compute("nonexistent", &pool, &llm)
            .await
            .unwrap();

        assert_eq!(result, None);
    }

    #[sqlx::test]
    async fn test_summary_single_flight(pool: SqlitePool) {
        setup_test_db(&pool).await;

        sqlx::query("INSERT INTO devices (id, name, summary_status) VALUES ('d1', 'Test', 'dirty')")
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
                Ok(format!(
                    "摘要版本{}",
                    self.count.load(Ordering::SeqCst)
                ))
            }
        }

        // Spawn first computation with a slow client
        let pool1 = pool.clone();
        let comp1 = computer.clone();
        let cc1 = CountingClient {
            count: call_count.clone(),
            delay_ms: 200,
        };
        let h = tokio::spawn(async move {
            comp1.get_or_compute("d1", &pool1, &cc1).await
        });

        // Give the spawned task a head start to acquire the single-flight lock
        tokio::time::sleep(Duration::from_millis(30)).await;

        // Second call: should wait on the single-flight Notify and get the
        // same result without calling the LLM again.
        let cc2 = CountingClient {
            count: call_count.clone(),
            delay_ms: 0,
        };
        let result2 = computer
            .get_or_compute("d1", &pool, &cc2)
            .await
            .unwrap();

        let result1 = h.await.unwrap().unwrap();

        // Both callers should get the same summary
        assert_eq!(result1, result2);
        assert!(result1.is_some());

        // LLM should have been called exactly once (by the first caller)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[sqlx::test]
    async fn test_summary_with_template_and_docs(pool: SqlitePool) {
        setup_test_db(&pool).await;

        // Insert a template
        sqlx::query(
            "INSERT INTO thing_templates (id, name, description, properties, actions) \
             VALUES ('t1', '温湿度传感器', '工业级温湿度监测', \
             '[{\"name\":\"temperature\"},{\"name\":\"humidity\"}]', \
             '[{\"name\":\"reboot\"}]')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a thing referencing the template
        sqlx::query(
            "INSERT INTO devices (id, name, thing_type, template_id, summary_status) \
             VALUES ('d1', '传感器A', 'device', 't1', 'dirty')",
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
        let result = computer
            .get_or_compute("d1", &pool, &llm)
            .await
            .unwrap();

        assert!(result.is_some());

        let status: String =
            sqlx::query_scalar("SELECT summary_status FROM devices WHERE id = 'd1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "ok");
    }
}
