//! AgentEvent broadcast contract — cross-subsystem event bus for the agent loop.
//!
//! `AgentEventBus` is a thin wrapper over `tokio::sync::broadcast`:
//! - `emit` stamps a process-local monotonic `seq` and `occurred_at` (DB fencing
//!   relies on `occurred_at`; schema unchanged, no version column).
//! - When the channel is full the oldest message is overwritten; subscribers
//!   detect loss via `RecvError::Lagged`.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use tinyiothub_core::agent_runs::RunReport;
use tinyiothub_core::heartbeat::{HeartbeatResult, TrustConfig};

use super::event::dlq::DeadLetterEntry;

#[derive(Clone)]
pub struct AgentEvent {
    pub seq: u64,                   // 进程内单调序号（调试/丢事件检测）
    pub occurred_at: DateTime<Utc>, // DB fencing 用（DB schema 不变，不用 version 列）
    pub kind: AgentEventKind,
}

#[derive(Clone)]
pub enum AgentEventKind {
    /// thing_agent run 完成记录（含完整 RunReport，幂等 insert-or-ignore by run_id）
    RunRecorded { report: Box<RunReport>, problem_key: Option<String>, dedup_key: Option<String> },
    /// 心跳 tick 结果（orchestrator 的 insert_result 替代）
    HeartbeatResultReady { result: Box<HeartbeatResult> },
    /// trust config 变更（状态跃迁，非每 tick）
    TrustConfigChanged { workspace_id: String, config: Box<TrustConfig> },
    /// 心跳任务列表变更（CRUD 后全量替换语义）
    HeartbeatTasksChanged { workspace_id: String },
    /// DLQ 条目（cloud subscriber 写 dlq 表）
    DlqEntryAdded { entry: Box<DeadLetterEntry> },
}

pub struct AgentEventBus {
    tx: broadcast::Sender<AgentEvent>,
    seq: AtomicU64,
}

impl AgentEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, seq: AtomicU64::new(0) }
    }

    pub fn emit(&self, kind: AgentEventKind) {
        let event = AgentEvent {
            seq: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
            occurred_at: Utc::now(),
            kind,
        };
        // 无订阅者时 send 返回 Err——持久化出口允许零订阅（测试/早期启动），忽略
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_stamps_monotonic_seq_and_delivers_to_subscriber() {
        let bus = AgentEventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: "ws1".into() });
        bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: "ws2".into() });
        let e1 = rx.recv().await.unwrap();
        let e2 = rx.recv().await.unwrap();
        assert!(e1.seq < e2.seq);
        assert!(e1.occurred_at <= e2.occurred_at);
    }

    #[tokio::test]
    async fn slow_subscriber_observes_lagged() {
        let bus = AgentEventBus::new(2);
        let mut rx = bus.subscribe();
        for i in 0..5 {
            bus.emit(AgentEventKind::HeartbeatTasksChanged { workspace_id: format!("ws{i}") });
        }
        assert!(matches!(rx.recv().await, Err(broadcast::error::RecvError::Lagged(_))));
    }
}
