// Confirmation token store for invoke_action

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::Value;

/// Pending action awaiting user confirmation.
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub token: String,
    pub thing_id: String,
    pub action_name: String,
    pub params: Option<Value>,
    pub workspace_id: String,
    pub created_at: Instant,
}

/// Pending-action confirmation store type (G3 — injected, no global).
pub type PendingActionStore = DashMap<String, PendingAction>;

const CONFIRMATION_TTL: Duration = Duration::from_secs(30 * 60);

/// 周期清扫间隔（生产）——远小于 TTL，滞留上限 = TTL + interval。
const SWEEP_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Store a pending action and return its confirmation token.
pub fn store_pending_action(
    store: &PendingActionStore,
    thing_id: String,
    action_name: String,
    params: Option<Value>,
    workspace_id: String,
) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    let pending = PendingAction {
        token: token.clone(),
        thing_id,
        action_name,
        params,
        workspace_id,
        created_at: Instant::now(),
    };
    store.insert(token.clone(), pending);
    token
}

/// Retrieve and consume a pending action by token (returns None if expired or not found).
pub fn take_pending_action(store: &PendingActionStore, token: &str) -> Option<PendingAction> {
    cleanup_expired_tokens(store);
    let entry = store.remove(token)?;
    if entry.1.created_at.elapsed() > CONFIRMATION_TTL {
        return None;
    }
    Some(entry.1)
}

/// Cleanup expired tokens (called on every take — keeps the map bounded).
pub fn cleanup_expired_tokens(store: &PendingActionStore) {
    store.retain(|_, v| v.created_at.elapsed() <= CONFIRMATION_TTL);
}

/// 周期清扫超期确认 token（F7）：take 时的懒清扫只覆盖有确认流量的路径，
/// 无流量的工作区条目会永久滞留（DashMap 无界增长）。返回的 JoinHandle 由
/// 调用方持有；任务无状态，关停 = abort handle（retain 按分片原子，无半截态）。
pub fn spawn_pending_action_sweeper(store: Arc<PendingActionStore>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let before = store.len();
            cleanup_expired_tokens(&store);
            let swept = before - store.len();
            if swept > 0 {
                tracing::info!(
                    swept,
                    remaining = store.len(),
                    "pending-action sweeper removed expired tokens"
                );
            }
        }
    })
}

/// 生产接线的默认间隔清扫（state.rs 组装点调用，进程级生命周期）。
pub fn spawn_pending_action_sweeper_default(store: Arc<PendingActionStore>) -> tokio::task::JoinHandle<()> {
    spawn_pending_action_sweeper(store, SWEEP_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个 created_at 已超期的条目（Instant 可回拨构造，无需等待真实 30min）。
    fn insert_entry(store: &PendingActionStore, token: &str, age: Duration) {
        store.insert(
            token.to_string(),
            PendingAction {
                token: token.to_string(),
                thing_id: "dev_1".to_string(),
                action_name: "reboot".to_string(),
                params: None,
                workspace_id: "ws_1".to_string(),
                created_at: Instant::now() - age,
            },
        );
    }

    #[tokio::test]
    async fn sweeper_removes_expired_keeps_fresh() {
        let store = Arc::new(PendingActionStore::new());
        insert_entry(&store, "expired", CONFIRMATION_TTL + Duration::from_secs(60));
        insert_entry(&store, "fresh", Duration::from_secs(5));

        let handle = spawn_pending_action_sweeper(store.clone(), Duration::from_millis(10));
        // 短间隔轮询直到清扫发生（上限 ~1s，无长 sleep）
        let deadline = Instant::now() + Duration::from_secs(1);
        while store.contains_key("expired") && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        handle.abort();

        assert!(!store.contains_key("expired"), "超期条目应被周期清扫");
        assert!(store.contains_key("fresh"), "未超期条目必须保留");
    }

    #[tokio::test]
    async fn take_consumes_and_rejects_expired() {
        let store = PendingActionStore::new();
        insert_entry(&store, "old", CONFIRMATION_TTL + Duration::from_secs(1));
        assert!(take_pending_action(&store, "old").is_none(), "超期 token 不得取出");

        let token = store_pending_action(&store, "dev_1".into(), "reboot".into(), None, "ws_1".into());
        assert!(take_pending_action(&store, &token).is_some(), "新 token 应可取");
        assert!(take_pending_action(&store, &token).is_none(), "取出即消费");
    }
}
