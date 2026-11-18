//! E1 值守中枢：每工作区 WatchEngine 事件循环（Task 8 装配）。
//!
//! manager.start() 在 watch_deps 已接线时为本工作区 spawn 本循环：
//! 启动重织（WatchRuleStore.list_all 过滤本区 → 幂等 upsert）→ EventBus
//! 订阅分发 + API 写穿命令通道 + 周期超时清扫。
//!
//! 事件分支（spec §2.2 / §4.3）：
//! - PropertyChange → `engine.on_property_change`（live.apply 含其中）；
//! - Connection（上下线）→ `engine.on_availability`（Info=上线，其余=下线；
//!   LiveState 内部以 cache 真相为准，事件只是刷新信号）；
//! - DeviceDeleted → `engine.invalidate_thing`（活跃 watch 失效落账）；
//! - Lagged → `engine.handle_lagged()`（清空实时态从 cache 全量重建）。
//!
//! 多租户：engine 注册表只持本区规则（API 校验 + 重织过滤双保险）；事件侧
//! 按 cache 中物的 workspace_id 过滤，cache miss（删除竞态/幽灵）放行——
//! invalidate/upsert 对外区物本是 no-op。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::mpsc;

use tinyiothub_core::models::event::{Event, EventLevel, EventType, ThingEventType};
use tinyiothub_core::watch::WatchRule;
use tinyiothub_runtime::EventBus;
use tinyiothub_runtime::ports::{HubLedger, ThingCacheSource, WatchRuleStore};
use tinyiothub_runtime::watch::engine::{WatchEngine, WatchHit};
use tinyiothub_runtime::watch::live_state::{LiveState, ThingLiveSummary};

/// 超时清扫周期（超时默认 600s 量级，5s 粒度足够；命中路径不经过它）。
const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

/// watch engine 构建依赖（bootstrap 注入，照 HeartbeatDeps 先例聚合一束）。
/// 全部经 runtime 端口/EventBus 抽象——agent crate 不依赖 db。
#[derive(Clone)]
pub struct WatchDeps {
    /// LiveState 真相源（cache 先写后发）。
    pub cache: Arc<dyn ThingCacheSource>,
    /// 命中/超时/失效账本（spec §4.2）。
    pub ledger: Arc<dyn HubLedger>,
    /// 规则持久源（启动重织读面）。
    pub rules: Arc<dyn WatchRuleStore>,
    /// DataServer 领域事件总线。
    pub events: Arc<EventBus>,
    /// E4d maps 权威（zone_source=maps）；None = 未接线 → hierarchy 降级。
    pub zones: Option<Arc<tinyiothub_runtime::maps::ZoneService>>,
}

/// API 写穿命令（D11-⑤：调用方已写 DB 成功）。经每工作区命令通道投递给
/// engine 事件循环——内存注册表是易失投影，命令失败只记日志（下次启动
/// 重织自愈）。
pub enum WatchCommand {
    Upsert(WatchRule),
    Remove(String),
    /// 单物实时摘要查询（Task 10 工具读面：get_thing_profile 附 live）；
    /// oneshot 回传，engine 无该物实时态 → None。
    Summary {
        thing_id: String,
        reply: tokio::sync::oneshot::Sender<Option<ThingLiveSummary>>,
    },
    /// E2 turn 开头工作区快照查询；engine 未运行/无实时态 → 空快照。
    Snapshot {
        reply: tokio::sync::oneshot::Sender<Option<tinyiothub_runtime::watch::live_state::WorkspaceSnapshot>>,
    },
}

/// 每工作区 watch engine 事件循环。通道关闭/任务 abort = 退役（stop 路径）。
pub async fn watch_engine_loop(
    workspace_id: String,
    deps: WatchDeps,
    hit_tx: mpsc::Sender<WatchHit>,
    mut cmd_rx: mpsc::Receiver<WatchCommand>,
) {
    // 先订阅再重织：重织窗口内的事件经 broadcast 缓冲，不丢。
    let mut rx = deps.events.subscribe();
    let live = match deps.zones.clone() {
        Some(zones) => LiveState::with_zone_service(deps.cache.clone(), zones),
        None => LiveState::new(deps.cache.clone()),
    };
    let mut engine = WatchEngine::new(
        live,
        deps.ledger.clone(),
        deps.rules.clone(), // C-2：退役路径（one-shot 命中/超时/物失效）写穿删除 DB 行
        hit_tx,
    );

    // 启动重织（spec §2.1）：持久源全量 → 本区规则幂等 upsert；单条非法
    // 规则跳过不株连（引用物已删等），整表读失败空注册表起步 + error。
    match deps.rules.list_all().await {
        Ok(rules) => {
            for rule in rules.into_iter().filter(|r| r.workspace_id == workspace_id) {
                if let Err(e) = engine.upsert_rule(rule).await {
                    tracing::warn!(workspace_id = %workspace_id, error = %e, "watch 启动重织跳过非法规则");
                }
            }
        }
        Err(e) => tracing::error!(workspace_id = %workspace_id, error = %e, "watch 启动重织失败（空注册表起步）"),
    }

    let mut sweep = tokio::time::interval(SWEEP_INTERVAL);
    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break }; // sender 随 stop 丢弃
                match cmd {
                    WatchCommand::Upsert(rule) => {
                        if let Err(e) = engine.upsert_rule(rule).await {
                            tracing::warn!(workspace_id = %workspace_id, error = %e, "watch 规则 upsert 拒绝");
                        }
                    }
                    WatchCommand::Remove(id) => {
                        engine.remove_rule(&id);
                    }
                    WatchCommand::Summary { thing_id, reply } => {
                        // 读面查询：调用方已脱离（工具超时应答），send 失败只丢本次。
                        let _ = reply.send(engine.summary(&thing_id).await);
                    }
                    WatchCommand::Snapshot { reply } => {
                        let _ = reply.send(Some(engine.workspace_snapshot(&workspace_id).await));
                    }
                }
            }
            ev = rx.recv() => match ev {
                Ok(event) => dispatch_event(&mut engine, &deps.cache, &workspace_id, &event).await,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(workspace_id = %workspace_id, skipped = n, "watch EventBus Lagged——全量重同步");
                    engine.handle_lagged();
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = sweep.tick() => engine.sweep_timeouts(chrono::Utc::now()).await,
        }
    }
}

/// 单事件分发：只触碰 PropertyChange/Connection/DeviceDeleted 三类；
/// 他区事件（cache 知情）不进场。
async fn dispatch_event(
    engine: &mut WatchEngine,
    cache: &Arc<dyn ThingCacheSource>,
    workspace_id: &str,
    event: &Event,
) {
    let EventType::Device(kind) = event.event_type() else {
        return;
    };
    let Some(thing_id) = event.source().thing_id() else {
        return;
    };
    // 工作区隔离：cache 知情的他区事件跳过；cache miss（删除竞态/幽灵事件
    // 先于真相到达）放行——LiveState 对幽灵容错，invalidate 对外区 no-op。
    if let Some(thing) = cache.get(thing_id)
        && thing.workspace_id.as_deref() != Some(workspace_id)
    {
        return;
    }
    match kind {
        ThingEventType::PropertyChange => {
            let metadata = event.content().metadata();
            let (Some(property), Some(value)) = (
                metadata.get("property_name").and_then(|v| v.as_str()),
                metadata.get("value").and_then(|v| v.as_str()),
            ) else {
                return;
            };
            engine.on_property_change(workspace_id, thing_id, property, value).await;
        }
        // 上下线：Info=上线（wrapper::on_connected），Warning/Error=下线/
        // 连接失败；LiveState 以 cache 真相为准，信号只触发刷新。
        ThingEventType::Connection => {
            engine
                .on_availability(workspace_id, thing_id, event.level() == EventLevel::Info)
                .await;
        }
        ThingEventType::DeviceDeleted => engine.invalidate_thing(thing_id).await,
        _ => {}
    }
}
