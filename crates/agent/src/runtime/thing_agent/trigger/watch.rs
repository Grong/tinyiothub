//! E1 watch 命中触发器：WatchEngine 的 WatchHit 经 mpsc 传入，组
//! WakeSignal{Watch} 唤醒值守 run。无 policy/governor 抑制——watch 命中
//! 本身已是意图（注册即显式订阅）。dedup key = watch:{rule_id}。

use std::sync::Arc;

use tokio::sync::mpsc;

use tinyiothub_runtime::watch::engine::WatchHit;

use super::Trigger;
use crate::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

/// E1 watch 命中触发器（spec 2026-10-20-e1 §1）：WatchEngine 命中经
/// hit_rx 传入即转发为 WakeSignal——命中本身已是意图（注册即显式订阅），
/// 无 policy/governor 抑制；命中通道关闭=shutdown。
pub struct WatchTrigger {
    pub workspace_id: String,
    pub hit_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<WatchHit>>>,
}

#[async_trait::async_trait]
impl Trigger for WatchTrigger {
    fn name(&self) -> &'static str {
        "watch"
    }

    async fn run(&self, tx: mpsc::Sender<WakeSignal>) -> anyhow::Result<()> {
        loop {
            let hit = {
                let mut rx = self.hit_rx.lock().await;
                rx.recv().await
            };
            let Some(hit) = hit else { return Ok(()) }; // channel 关闭=shutdown
            let signal = WakeSignal {
                workspace_id: hit.workspace_id.clone(),
                priority: Priority::Normal,
                dedup_key: Some(format!("watch:{}", hit.rule_id)),
                source: TriggerSource::Watch {
                    rule_id: hit.rule_id,
                    // 上游字段映射（Task 5 实况）：summary:ThingLiveSummary→
                    // 人读摘要串（health/zone/online/values Debug 单行）；
                    // current_value:Value→字符串值原样、Null→None、其余
                    // JSON 紧凑串；health/zone→Some(可读者)，空 zone→None。
                    summary: format!("{:?}", hit.summary),
                    current_value: match hit.current_value {
                        serde_json::Value::String(s) => Some(s),
                        serde_json::Value::Null => None,
                        other => Some(other.to_string()),
                    },
                    health: Some(format!("{:?}", hit.health)),
                    zone: if hit.zone.is_empty() { None } else { Some(hit.zone) },
                },
            };
            if tx.send(signal).await.is_err() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use tinyiothub_runtime::watch::live_state::{Health, ThingLiveSummary};

    fn hit(ws: &str, rule: &str) -> WatchHit {
        WatchHit {
            workspace_id: ws.into(),
            rule_id: rule.into(),
            summary: ThingLiveSummary {
                health: Health::Bad,
                zone: "车间/产线".into(),
                online: true,
                values: HashMap::from([("temp".to_string(), "85".to_string())]),
            },
            current_value: serde_json::json!("85"),
            health: Health::Bad,
            zone: "车间/产线".into(),
        }
    }

    fn trigger(hit_rx: mpsc::Receiver<WatchHit>) -> WatchTrigger {
        WatchTrigger {
            workspace_id: "ws_01".into(),
            hit_rx: Arc::new(tokio::sync::Mutex::new(hit_rx)),
        }
    }

    #[test]
    fn trigger_name_is_watch() {
        let (_hit_tx, hit_rx) = mpsc::channel::<WatchHit>(4);
        assert_eq!(trigger(hit_rx).name(), "watch");
    }

    #[tokio::test]
    async fn hit_emits_wake_signal_with_watch_source_and_dedup_key() {
        let (hit_tx, hit_rx) = mpsc::channel(4);
        let trig = trigger(hit_rx);
        let (tx, mut rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trig.run(tx).await });

        hit_tx.send(hit("ws_01", "rule_7")).await.unwrap();
        let sig = rx.recv().await.expect("命中应出 WakeSignal");
        assert_eq!(sig.workspace_id, "ws_01");
        assert_eq!(sig.priority, Priority::Normal);
        assert_eq!(sig.dedup_key.as_deref(), Some("watch:rule_7"));
        match sig.source {
            TriggerSource::Watch {
                rule_id,
                summary,
                current_value,
                health,
                zone,
            } => {
                assert_eq!(rule_id, "rule_7");
                assert!(!summary.is_empty(), "summary 携带实时摘要");
                assert_eq!(current_value.as_deref(), Some("85"));
                assert!(health.is_some(), "health 携带命中时健康度");
                assert_eq!(zone.as_deref(), Some("车间/产线"));
            }
            other => panic!("期望 TriggerSource::Watch，实际 {other:?}"),
        }

        drop(hit_tx); // 关命中通道放行 run 退出（否则阻塞在 recv）
        drop(rx);
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn hit_channel_close_exits_cleanly() {
        let (hit_tx, hit_rx) = mpsc::channel::<WatchHit>(4);
        let trig = trigger(hit_rx);
        let (tx, _rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trig.run(tx).await });
        drop(hit_tx); // 命中通道关闭=shutdown
        handle.await.expect("run task").expect("run result");
    }

    #[tokio::test]
    async fn wake_channel_close_exits_cleanly() {
        let (hit_tx, hit_rx) = mpsc::channel::<WatchHit>(4);
        let trig = trigger(hit_rx);
        let (tx, rx) = mpsc::channel(16);
        let handle = tokio::spawn(async move { trig.run(tx).await });
        hit_tx.send(hit("ws_01", "rule_9")).await.unwrap();
        drop(rx); // 收端已关——send 失败即退出，不空转
        handle.await.expect("run task").expect("run result");
    }
}
