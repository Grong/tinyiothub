//! JsonlMemory — thing_agent 自治路径的持久化 memory 后端（Task 9b，
//! 修复 Task 9 审查 I1：state.rs 注入 NoopMemory 是生产行为退化）。
//!
//! 设计不变量：crates/agent 保持"零存储实现"，本实现住组合层（apps/cloud）。
//! 存储格式：每行一条 JSON（MemoryEntry 序列化），timestamp 为 RFC3339。
//!
//! 语义对齐原 zeroclaw sqlite 后端：
//!   - store：追加一行
//!   - recall：空查询 / 纯空白 / "*" = 按时间 DESC 取最近 N 条；
//!     关键词查询 = 内容含全部空格分词（大小写不敏感）；
//!     session_id / since / until 过滤，since/until 为 RFC3339 闭区间
//!   - forget：按 key 重写文件（load-过滤-rewrite）
//!   - 并发 append 在 Mutex 下串行化
//!
//! 单一 JSONL 文件由所有 workspace 共享，workspace 隔离经
//! WorkspaceScopedMemory 的 namespace 包装实现（与原 per-workspace
//! sqlite 文件等价）。

use std::io::Write as _;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use tinyiothub_agent::port::attribution::{Attributable, MemoryKind, Role};
use tinyiothub_agent::port::memory::{Memory, MemoryCategory, MemoryEntry};
use tokio::sync::Mutex;

pub struct JsonlMemory {
    path: PathBuf,
    mu: Mutex<()>,
}

impl JsonlMemory {
    /// Create a JsonlMemory backed by `path`. Parent directories are
    /// created eagerly so health_check/append never fail on a missing dir.
    pub fn new(path: PathBuf) -> Self {
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(?e, dir = %parent.display(), "JsonlMemory: failed to create parent dir");
        }
        Self {
            path,
            mu: Mutex::new(()),
        }
    }

    fn parse_time(s: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
    }

    /// RFC3339 闭区间过滤：created_at >= since && created_at <= until。
    /// 边界值非法时视为不过滤；entry 时间戳非法且有过滤条件时排除。
    fn within_time_range(entry_ts: &str, since: Option<&str>, until: Option<&str>) -> bool {
        let ts = Self::parse_time(entry_ts);
        if let Some(s) = since.and_then(Self::parse_time)
            && ts.is_none_or(|t| t < s)
        {
            return false;
        }
        if let Some(u) = until.and_then(Self::parse_time)
            && ts.is_none_or(|t| t > u)
        {
            return false;
        }
        true
    }

    /// 关键词匹配：空 / 纯空白 / "*" 匹配一切；否则内容须含全部空格
    /// 分词（大小写不敏感，子串匹配）。
    fn matches_query(content: &str, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() || q == "*" {
            return true;
        }
        let lower = content.to_lowercase();
        q.split_whitespace().all(|term| lower.contains(&term.to_lowercase()))
    }

    fn load(&self) -> anyhow::Result<Vec<MemoryEntry>> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut entries = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<MemoryEntry>(line) {
                Ok(e) => entries.push(e),
                Err(e) => {
                    tracing::warn!(?e, line = idx + 1, "JsonlMemory: skipping corrupt line");
                }
            }
        }
        Ok(entries)
    }

    fn append_line(&self, entry: &MemoryEntry) -> anyhow::Result<()> {
        let mut line = serde_json::to_string(entry)?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        f.write_all(line.as_bytes())?;
        Ok(())
    }

    fn rewrite(&self, entries: &[MemoryEntry]) -> anyhow::Result<()> {
        let mut buf = String::new();
        for e in entries {
            buf.push_str(&serde_json::to_string(e)?);
            buf.push('\n');
        }
        std::fs::write(&self.path, buf)?;
        Ok(())
    }

    fn scan(
        entries: Vec<MemoryEntry>,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Vec<MemoryEntry> {
        let mut filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| session_id.is_none_or(|s| e.session_id.as_deref() == Some(s)))
            .filter(|e| Self::within_time_range(&e.timestamp, since, until))
            .filter(|e| Self::matches_query(&e.content, query))
            .collect();
        // 时间 DESC；时间戳非法的排最后
        filtered.sort_by_key(|e| std::cmp::Reverse(Self::parse_time(&e.timestamp)));
        filtered.truncate(limit);
        filtered
    }

    fn make_entry(
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            key: key.to_string(),
            content: content.to_string(),
            category,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true),
            session_id: session_id.map(String::from),
            score: None,
            namespace: namespace.unwrap_or("default").to_string(),
            importance,
            superseded_by: None,
            agent_alias: None,
            agent_id: agent_id.map(String::from),
        }
    }
}

impl Attributable for JsonlMemory {
    fn role(&self) -> Role {
        Role::Memory(MemoryKind::Json)
    }
    fn alias(&self) -> &str {
        "jsonl_memory"
    }
}

#[async_trait]
impl Memory for JsonlMemory {
    fn name(&self) -> &str {
        "jsonl"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.store_with_agent(key, content, category, session_id, None, None, None)
            .await
    }

    async fn store_with_agent(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
        agent_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let entry = Self::make_entry(key, content, category, session_id, namespace, importance, agent_id);
        let _g = self.mu.lock().await;
        self.append_line(&entry)
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let _g = self.mu.lock().await;
        Ok(Self::scan(self.load()?, query, limit, session_id, since, until))
    }

    async fn recall_for_agents(
        &self,
        allowed_agent_ids: &[&str],
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let _g = self.mu.lock().await;
        let entries = self.load()?;
        // 空 allowlist = 不过滤；非空 = agent_id 命中或为 NULL（legacy 行）
        let entries: Vec<MemoryEntry> = if allowed_agent_ids.is_empty() {
            entries
        } else {
            entries
                .into_iter()
                .filter(|e| e.agent_id.as_deref().is_none_or(|id| allowed_agent_ids.contains(&id)))
                .collect()
        };
        Ok(Self::scan(entries, query, limit, session_id, since, until))
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let _g = self.mu.lock().await;
        Ok(self.load()?.into_iter().find(|e| e.key == key))
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let _g = self.mu.lock().await;
        Ok(self
            .load()?
            .into_iter()
            .filter(|e| category.is_none_or(|c| e.category == *c))
            .filter(|e| session_id.is_none_or(|s| e.session_id.as_deref() == Some(s)))
            .collect())
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        let _g = self.mu.lock().await;
        let entries = self.load()?;
        if !entries.iter().any(|e| e.key == key) {
            return Ok(false);
        }
        let kept: Vec<MemoryEntry> = entries.into_iter().filter(|e| e.key != key).collect();
        self.rewrite(&kept)?;
        Ok(true)
    }

    async fn forget_for_agent(&self, key: &str, agent_id: &str) -> anyhow::Result<bool> {
        let _g = self.mu.lock().await;
        let entries = self.load()?;
        let hit = entries
            .iter()
            .any(|e| e.key == key && e.agent_id.as_deref() == Some(agent_id));
        if !hit {
            return Ok(false);
        }
        let kept: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| !(e.key == key && e.agent_id.as_deref() == Some(agent_id)))
            .collect();
        self.rewrite(&kept)?;
        Ok(true)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let _g = self.mu.lock().await;
        Ok(self.load()?.len())
    }

    async fn health_check(&self) -> bool {
        let _g = self.mu.lock().await;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn temp_memory() -> (TempDir, JsonlMemory) {
        let dir = tempfile::tempdir().unwrap();
        let mem = JsonlMemory::new(dir.path().join("agent_memory.jsonl"));
        (dir, mem)
    }

    fn entry_at(key: &str, content: &str, ts: &str) -> MemoryEntry {
        MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            key: key.to_string(),
            content: content.to_string(),
            category: MemoryCategory::Core,
            timestamp: ts.to_string(),
            session_id: None,
            score: None,
            namespace: "default".to_string(),
            importance: None,
            superseded_by: None,
            agent_alias: None,
            agent_id: None,
        }
    }

    /// 直接写一行（测试专用，绕过 store 的 Utc::now() 时间戳）
    fn write_line(mem: &JsonlMemory, entry: &MemoryEntry) {
        mem.append_line(entry).unwrap();
    }

    #[tokio::test]
    async fn store_and_recall_keyword() {
        let (_d, mem) = temp_memory();
        mem.store("k1", "temperature sensor reads 42", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("k2", "humidity sensor reads 60", MemoryCategory::Core, None)
            .await
            .unwrap();

        let hits = mem.recall("temperature", 10, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "k1");

        // 多词需全部命中
        let hits = mem.recall("sensor 60", 10, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "k2");

        // 大小写不敏感
        let hits = mem.recall("SENSOR", 10, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 2);

        // 无命中
        assert!(mem.recall("pressure", 10, None, None, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn recall_empty_query_returns_recent_desc() {
        let (_d, mem) = temp_memory();
        write_line(&mem, &entry_at("old", "alpha", "2026-01-01T00:00:00Z"));
        write_line(&mem, &entry_at("mid", "beta", "2026-06-01T00:00:00Z"));
        write_line(&mem, &entry_at("new", "gamma", "2026-09-01T00:00:00Z"));

        for q in ["", "   ", "*"] {
            let hits = mem.recall(q, 10, None, None, None).await.unwrap();
            let keys: Vec<&str> = hits.iter().map(|e| e.key.as_str()).collect();
            assert_eq!(keys, ["new", "mid", "old"], "query={q:?}");
        }

        // limit 截断
        let hits = mem.recall("", 2, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].key, "new");
    }

    #[tokio::test]
    async fn recall_since_until_inclusive() {
        let (_d, mem) = temp_memory();
        write_line(&mem, &entry_at("a", "x", "2026-03-01T00:00:00Z"));
        write_line(&mem, &entry_at("b", "x", "2026-03-15T12:00:00Z"));
        write_line(&mem, &entry_at("c", "x", "2026-03-31T23:59:59Z"));

        // 闭区间：两端边界均命中
        let hits = mem
            .recall(
                "x",
                10,
                None,
                Some("2026-03-01T00:00:00Z"),
                Some("2026-03-31T23:59:59Z"),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 3);

        // 收缩区间
        let hits = mem
            .recall(
                "x",
                10,
                None,
                Some("2026-03-02T00:00:00Z"),
                Some("2026-03-30T00:00:00Z"),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "b");
    }

    #[tokio::test]
    async fn store_with_agent_sets_namespace_and_agent_id() {
        let (_d, mem) = temp_memory();
        mem.store_with_agent(
            "k",
            "content",
            MemoryCategory::Conversation,
            Some("sess-1"),
            Some("ws-a"),
            Some(0.8),
            Some("agent-1"),
        )
        .await
        .unwrap();
        mem.store_with_agent(
            "k",
            "other",
            MemoryCategory::Core,
            None,
            Some("ws-b"),
            None,
            Some("agent-2"),
        )
        .await
        .unwrap();

        // session 过滤
        let hits = mem.recall("*", 10, Some("sess-1"), None, None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].namespace, "ws-a");
        assert_eq!(hits[0].importance, Some(0.8));

        // agent allowlist 过滤
        let hits = mem
            .recall_for_agents(&["agent-2"], "*", 10, None, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].namespace, "ws-b");

        // 空 allowlist = 不过滤
        let hits = mem.recall_for_agents(&[], "*", 10, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[tokio::test]
    async fn get_list_count() {
        let (_d, mem) = temp_memory();
        mem.store("k1", "one", MemoryCategory::Core, Some("s1")).await.unwrap();
        mem.store("k2", "two", MemoryCategory::Daily, None).await.unwrap();

        assert_eq!(mem.get("k1").await.unwrap().unwrap().content, "one");
        assert!(mem.get("missing").await.unwrap().is_none());

        assert_eq!(mem.list(None, None).await.unwrap().len(), 2);
        assert_eq!(mem.list(Some(&MemoryCategory::Daily), None).await.unwrap().len(), 1);
        assert_eq!(mem.list(None, Some("s1")).await.unwrap().len(), 1);
        assert_eq!(mem.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn forget_removes_by_key() {
        let (_d, mem) = temp_memory();
        mem.store("gone", "x", MemoryCategory::Core, None).await.unwrap();
        mem.store("stay", "y", MemoryCategory::Core, None).await.unwrap();

        assert!(mem.forget("gone").await.unwrap());
        assert!(!mem.forget("gone").await.unwrap());
        assert_eq!(mem.count().await.unwrap(), 1);
        assert_eq!(mem.get("stay").await.unwrap().unwrap().key, "stay");
    }

    #[tokio::test]
    async fn forget_for_agent_only_removes_matching_row() {
        let (_d, mem) = temp_memory();
        mem.store_with_agent("shared", "a1", MemoryCategory::Core, None, None, None, Some("agent-1"))
            .await
            .unwrap();
        mem.store_with_agent("shared", "a2", MemoryCategory::Core, None, None, None, Some("agent-2"))
            .await
            .unwrap();

        assert!(mem.forget_for_agent("shared", "agent-1").await.unwrap());
        assert!(!mem.forget_for_agent("shared", "agent-1").await.unwrap());
        let hits = mem.recall("*", 10, None, None, None).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].agent_id.as_deref(), Some("agent-2"));
    }

    #[tokio::test]
    async fn concurrent_appends_lose_no_lines() {
        let (_d, mem) = temp_memory();
        let mem = Arc::new(mem);
        let mut handles = Vec::new();
        for i in 0..100 {
            let m = Arc::clone(&mem);
            handles.push(tokio::spawn(async move {
                m.store(&format!("key-{i}"), &format!("content {i}"), MemoryCategory::Core, None)
                    .await
                    .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(mem.count().await.unwrap(), 100);
        let raw = std::fs::read_to_string(&mem.path).unwrap();
        assert_eq!(raw.lines().count(), 100);
    }

    #[tokio::test]
    async fn health_check_true_when_file_openable() {
        let (_d, mem) = temp_memory();
        assert!(mem.health_check().await);
        // 文件被 health_check 创建
        assert!(mem.path.exists());
    }
}
