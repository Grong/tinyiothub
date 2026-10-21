//! E1 值守中枢：WatchEngine——按字段倒排索引求值 + 命中语义 + 超时清扫。
//!
//! 事件流（spec §2.2）：DataServer 事件 → LiveState 更新 → 按 `(thing_id, field)`
//! 倒排索引取候选 watch → 纯函数 `WatchCondition::matches` 求值 → 命中后
//! ① hub_ledger 落一行 ② `WatchHit` 经 mpsc 发往 WatchTrigger。
//!
//! 命中语义：
//! - only-new-data：注册时条件已成立 → `armed=false`，须先不成立（rearm）再成立才命中；
//! - 边沿触发：命中后 `armed=false`，条件不成立一次后重新武装（health「变为该级」语义）；
//! - one-shot 命中即从注册表移除；persistent 命中后保持，且 1s 合并窗内不重复发；
//! - 命中/超时/失效落账本（kind=watch_hit/watch_timeout/watch_expired，payload 带 v=1）。
//!
//! 错误处理（spec §4.3）：账本写失败重试 3 次 → warn + `ledger_write_failures` 计数，
//! 不中断值守链路（账本可补，唤醒不能丢）。
//!
//! 单线程所有者：全部方法要 `&mut self`，由事件循环任务独占持有，无内部锁。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::sync::mpsc;

use tinyiothub_core::watch::{WatchCondition, WatchRule};

use super::live_state::{Health, LiveState, ThingLiveSummary};
use crate::ports::{HubLedger, WatchRuleStore};

/// persistent watch 命中合并窗（spec §2.2：同一 watch 1s 内多次命中合并为一次唤醒）。
const MERGE_WINDOW: Duration = Duration::from_secs(1);
/// 账本写重试次数（spec §4.3）。
const LEDGER_MAX_ATTEMPTS: usize = 3;

/// 命中事件：发往 WatchTrigger（下游组 WakeSignal{Watch}）。
#[derive(Debug, Clone)]
pub struct WatchHit {
    pub workspace_id: String,
    pub rule_id: String,
    pub summary: ThingLiveSummary,
    pub current_value: serde_json::Value,
    pub health: Health,
    pub zone: String,
}

/// 活跃 watch 内部态。
struct ActiveWatch {
    rule: WatchRule,
    /// 可否命中：注册时已成立 → false；条件不成立一次后 → true；命中后 → false。
    armed: bool,
    /// 引擎侧登记时刻（超时计时基准；不解析 rule.created_at 字符串）。
    registered_at: DateTime<Utc>,
}

/// watch 求值引擎。规则注册表 + (thing_id, field) 倒排索引。
pub struct WatchEngine {
    live: LiveState,
    ledger: Arc<dyn HubLedger>,
    /// 规则持久源（C-2 退役写穿：one-shot 命中/超时/物失效摘内存后删 DB 行，
    /// 防重启重织复活已退役规则）。
    store: Arc<dyn WatchRuleStore>,
    hit_tx: mpsc::Sender<WatchHit>,
    /// 注册表：rule_id → 活跃态。
    rules: HashMap<String, ActiveWatch>,
    /// 倒排索引：(thing_id, field) → rule_id 集合；field 为属性名或 __health/__availability。
    index: HashMap<(String, String), HashSet<String>>,
    /// persistent 1s 合并窗：rule_id → 上次命中时刻。
    last_hit_at: HashMap<String, Instant>,
    /// 账本写彻底失败计数（重试 3 次仍失败 +1，metric 兜底）。
    ledger_write_failures: AtomicU64,
}

impl WatchEngine {
    pub fn new(
        live: LiveState,
        ledger: Arc<dyn HubLedger>,
        store: Arc<dyn WatchRuleStore>,
        hit_tx: mpsc::Sender<WatchHit>,
    ) -> Self {
        Self {
            live,
            ledger,
            store,
            hit_tx,
            rules: HashMap::new(),
            index: HashMap::new(),
            last_hit_at: HashMap::new(),
            ledger_write_failures: AtomicU64::new(0),
        }
    }

    /// 注册/覆盖规则（幂等：同 id 重注册先摘旧索引）。规则非法 → Err。
    /// 存在性校验单点在 API 层（DB 真相）；engine 不查实时态——only-new-data
    /// 语义天然容错：无实时态 = 不成立 = armed，首次事件进场即正常求值（C-1）。
    pub fn upsert_rule(&mut self, rule: WatchRule) -> Result<(), String> {
        rule.validate()?;
        let key = rule.condition.index_key();
        self.remove_rule(&rule.id);
        // only-new-data：注册时已成立 → 不武装，须先不成立再成立才命中。
        let armed = !self.current_match(&rule.condition);
        self.index.entry(key).or_default().insert(rule.id.clone());
        self.rules.insert(
            rule.id.clone(),
            ActiveWatch {
                rule,
                armed,
                registered_at: Utc::now(),
            },
        );
        Ok(())
    }

    /// 摘除规则；不存在返回 false。
    pub fn remove_rule(&mut self, id: &str) -> bool {
        let Some(watch) = self.rules.remove(id) else {
            return false;
        };
        let key = watch.rule.condition.index_key();
        if let Some(ids) = self.index.get_mut(&key) {
            ids.remove(id);
            if ids.is_empty() {
                self.index.remove(&key);
            }
        }
        self.last_hit_at.remove(id);
        true
    }

    /// 属性变更事件：先落 state_change 账（I-2 / spec §4.2 全量落账，新旧值
    /// 取自变更前后摘要），再更新 LiveState 并求值该物的 属性字段/__health/
    /// __availability（cache 真相刷新可能同时改变 health/online 派生）。
    /// `workspace_id` 仅作调用侧对账与落账（规则自带 workspace_id 为命中落账真相）。
    pub async fn on_property_change(&mut self, workspace_id: &str, thing_id: &str, property: &str, value: &str) {
        let old = self
            .live
            .summary(thing_id)
            .and_then(|s| s.values.get(property).cloned());
        self.append_ledger(
            workspace_id,
            "state_change",
            Some(thing_id),
            json!({ "v": 1, "field": property, "old": old, "new": value }),
        )
        .await;
        self.live.apply_property_change(thing_id, property, value);
        self.evaluate_thing(thing_id, &[property, "__health", "__availability"])
            .await;
    }

    /// 可用性事件：落 state_change 账（field=__availability）后更新 LiveState
    /// 并求值该物的 __availability/__health。
    pub async fn on_availability(&mut self, workspace_id: &str, thing_id: &str, online: bool) {
        let old = self.live.summary(thing_id).map(|s| s.online);
        self.append_ledger(
            workspace_id,
            "state_change",
            Some(thing_id),
            json!({ "v": 1, "field": "__availability", "old": old, "new": online }),
        )
        .await;
        self.live.set_availability(thing_id, online);
        self.evaluate_thing(thing_id, &["__availability", "__health"]).await;
    }

    /// 超时清扫：登记时长超 timeout_secs 的**非 persistent** 规则移除并落
    /// watch_timeout（matched:false）+ 退役写穿 DB。persistent 规则免超时
    ///（I-1 裁定：持久规则型语义，「持久」名符其实）。
    pub async fn sweep_timeouts(&mut self, now: DateTime<Utc>) {
        let expired: Vec<String> = self
            .rules
            .iter()
            .filter(|(_, w)| {
                !w.rule.persistent && (now - w.registered_at).num_seconds() >= i64::from(w.rule.timeout_secs)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            let Some(rule) = self.rules.get(&id).map(|w| w.rule.clone()) else {
                continue;
            };
            let thing_id = condition_thing_id(&rule.condition).to_string();
            let payload = json!({
                "v": 1,
                "rule_id": rule.id,
                "matched": false,
                "timeout_secs": rule.timeout_secs,
            });
            self.append_ledger(&rule.workspace_id, "watch_timeout", Some(&thing_id), payload)
                .await;
            self.remove_rule(&id);
            self.delete_persisted(&rule.workspace_id, &id).await;
        }
    }

    /// 物删除：该物全部活跃 watch 失效，逐条落 watch_expired（spec §4.3 设备
    /// 删除行）+ 退役写穿 DB。
    pub async fn invalidate_thing(&mut self, thing_id: &str) {
        let ids: Vec<String> = self
            .rules
            .iter()
            .filter(|(_, w)| condition_thing_id(&w.rule.condition) == thing_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let Some(rule) = self.rules.get(&id).map(|w| w.rule.clone()) else {
                continue;
            };
            let payload = json!({ "v": 1, "rule_id": rule.id, "reason": "thing_deleted" });
            self.append_ledger(&rule.workspace_id, "watch_expired", Some(thing_id), payload)
                .await;
            self.remove_rule(&id);
            self.delete_persisted(&rule.workspace_id, &id).await;
        }
    }

    /// EventBus Lagged 空洞（Task 8 接线点）：清空实时态，从 cache 全量重建
    ///（cache 先写后发不变量，重读即真相）。
    pub fn handle_lagged(&mut self) {
        self.live.handle_lagged();
    }

    /// 单物实时摘要（Task 10 工具读面；委托 LiveState::summary，
    /// 无实时态 → None）。
    pub fn summary(&self, thing_id: &str) -> Option<ThingLiveSummary> {
        self.live.summary(thing_id)
    }

    /// E2 turn 开头快照（spec §4）：本工作区实时态，health worst-first。
    pub fn workspace_snapshot(&self, workspace_id: &str) -> super::live_state::WorkspaceSnapshot {
        self.live.workspace_snapshot(workspace_id)
    }

    /// 账本写彻底失败计数（metric 兜底）。
    pub fn ledger_write_failures(&self) -> u64 {
        self.ledger_write_failures.load(Ordering::Relaxed)
    }

    /// 活跃规则数（限额观测用）。
    pub fn active_rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 对该物的给定字段逐一求值：从 LiveState 摘要取当前值，走倒排索引。
    async fn evaluate_thing(&mut self, thing_id: &str, fields: &[&str]) {
        let Some(summary) = self.live.summary(thing_id) else {
            return;
        };
        for field in fields {
            let Some(value) = Self::field_value(&summary, field) else {
                continue;
            };
            self.evaluate_field(thing_id, field, value).await;
        }
    }

    /// 单字段求值：armed 语义 + persistent 1s 合并窗 + 命中分发。
    async fn evaluate_field(&mut self, thing_id: &str, field: &str, value: serde_json::Value) {
        let key = (thing_id.to_string(), field.to_string());
        let Some(ids) = self.index.get(&key) else {
            return;
        };
        let ids: Vec<String> = ids.iter().cloned().collect();
        for id in ids {
            let Some(watch) = self.rules.get(&id) else {
                continue;
            };
            if !watch.rule.condition.matches(thing_id, field, &value) {
                // 不成立 → 重新武装（only-new-data / 边沿触发语义）。
                if let Some(w) = self.rules.get_mut(&id) {
                    w.armed = true;
                }
                continue;
            }
            if !watch.armed {
                continue;
            }
            if watch.rule.persistent && self.last_hit_at.get(&id).is_some_and(|t| t.elapsed() < MERGE_WINDOW) {
                continue; // 1s 合并窗内不重复发
            }
            self.fire_hit(&id, value.clone()).await;
        }
    }

    /// 命中：落账 → 发 WatchHit → one-shot 移除 / persistent 保持并记录命中时刻。
    async fn fire_hit(&mut self, rule_id: &str, current_value: serde_json::Value) {
        let Some(rule) = self.rules.get(rule_id).map(|w| w.rule.clone()) else {
            return;
        };
        let thing_id = condition_thing_id(&rule.condition).to_string();
        // spec §4.3：摘要装配异常降级为空摘要，值守链路不炸。
        let summary = self.live.summary(&thing_id).unwrap_or(ThingLiveSummary {
            health: Health::Unknown,
            zone: String::new(),
            online: false,
            values: HashMap::new(),
        });
        let payload = json!({
            "v": 1,
            "rule_id": rule.id,
            "matched": true,
            "current_value": current_value,
            "health": health_str(summary.health),
            "zone": summary.zone,
        });
        self.append_ledger(&rule.workspace_id, "watch_hit", Some(&thing_id), payload)
            .await;
        let hit = WatchHit {
            workspace_id: rule.workspace_id.clone(),
            rule_id: rule.id.clone(),
            health: summary.health,
            zone: summary.zone.clone(),
            summary,
            current_value,
        };
        if self.hit_tx.send(hit).await.is_err() {
            tracing::warn!(rule_id = %rule.id, "watch hit 通道已关闭，命中事件丢弃");
        }
        if rule.persistent {
            self.last_hit_at.insert(rule_id.to_string(), Instant::now());
            // 边沿触发：命中后须先不成立再成立才再次命中（合并窗之外的第二道闸）。
            if let Some(w) = self.rules.get_mut(rule_id) {
                w.armed = false;
            }
        } else {
            self.remove_rule(rule_id);
            self.delete_persisted(&rule.workspace_id, rule_id).await;
        }
    }

    /// 退役写穿（C-2）：内存摘除后 best-effort 删持久源行——失败只 warn，
    /// 不阻断值守链路（重启重织时宽松 upsert 兜底，限额泄漏可手动 DELETE）。
    async fn delete_persisted(&self, workspace_id: &str, rule_id: &str) {
        if let Err(e) = self.store.delete(workspace_id, rule_id).await {
            tracing::warn!(rule_id, error = %e, "watch 规则退役写穿失败（DB 行残留，下次重启重织兜底）");
        }
    }

    /// 账本写：重试 3 次 → warn + 计数，不中断值守链路（账本可补，唤醒不能丢）。
    async fn append_ledger(&self, workspace_id: &str, kind: &str, thing_id: Option<&str>, payload: serde_json::Value) {
        for attempt in 1..=LEDGER_MAX_ATTEMPTS {
            match self.ledger.append(workspace_id, kind, thing_id, payload.clone()).await {
                Ok(()) => return,
                Err(e) => {
                    tracing::warn!(kind, attempt, error = %e, "hub_ledger 写失败，重试");
                }
            }
        }
        self.ledger_write_failures.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(
            kind,
            metric = "watch_ledger_write_failed",
            "hub_ledger 写失败 {LEDGER_MAX_ATTEMPTS} 次，放弃（不中断值守链路）"
        );
    }

    /// 注册时初始成立判定（only-new-data）：无实时态/无当前值视为不成立。
    fn current_match(&self, condition: &WatchCondition) -> bool {
        let (thing_id, field) = condition.index_key();
        let Some(summary) = self.live.summary(&thing_id) else {
            return false;
        };
        let Some(value) = Self::field_value(&summary, &field) else {
            return false;
        };
        condition.matches(&thing_id, &field, &value)
    }

    /// 摘要 → 字段当前值：哨兵字段取自派生量；属性值为 String，尽力解析为 JSON
    /// （"31.5"→数值，"on"→字符串），与 `matches` 的类型语义对齐。
    fn field_value(summary: &ThingLiveSummary, field: &str) -> Option<serde_json::Value> {
        match field {
            "__health" => Some(json!(health_str(summary.health))),
            "__availability" => Some(json!(summary.online)),
            _ => summary
                .values
                .get(field)
                .map(|s| serde_json::from_str(s).unwrap_or_else(|_| json!(s))),
        }
    }
}

/// 条件挂载物 id（落账 thing_id 用）。
fn condition_thing_id(condition: &WatchCondition) -> &str {
    match condition {
        WatchCondition::Property { thing_id, .. }
        | WatchCondition::Health { thing_id, .. }
        | WatchCondition::Availability { thing_id, .. } => thing_id,
    }
}

/// health 派生量 → 词表字符串（与 WatchCondition::Health 匹配值一致）。
fn health_str(health: Health) -> &'static str {
    match health {
        Health::Ok => "ok",
        Health::Attention => "attention",
        Health::Bad => "bad",
        Health::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;

    use async_trait::async_trait;
    use tinyiothub_core::models::thing::{Thing, ThingStatus};
    use tinyiothub_core::watch::{CompareOp, HealthLevel};

    use crate::ports::{ThingCacheSource, WatchRuleStore};

    /// 内存桩：最小可读 ThingCacheSource。
    struct StubCache {
        things: Mutex<HashMap<String, Thing>>,
    }

    impl StubCache {
        fn with_things(things: Vec<Thing>) -> Self {
            Self {
                things: Mutex::new(things.into_iter().map(|t| (t.id.clone(), t)).collect()),
            }
        }
    }

    impl ThingCacheSource for StubCache {
        fn all(&self) -> Vec<Thing> {
            self.things.lock().unwrap().values().cloned().collect()
        }
        fn get(&self, id: &str) -> Option<Thing> {
            self.things.lock().unwrap().get(id).cloned()
        }
        fn get_by_name(&self, name: &str) -> Option<Thing> {
            self.things.lock().unwrap().values().find(|t| t.name == name).cloned()
        }
        fn insert(&self, device: Thing) {
            self.things.lock().unwrap().insert(device.id.clone(), device);
        }
        fn update(&self, device: Thing) {
            self.insert(device);
        }
        fn remove(&self, id: &str) {
            self.things.lock().unwrap().remove(id);
        }
    }

    /// 规则持久源桩：内存表，模拟 API 写穿后的 DB 行。
    #[derive(Default)]
    struct StubRuleStore {
        rules: Mutex<HashMap<String, WatchRule>>,
    }

    impl StubRuleStore {
        fn contains(&self, id: &str) -> bool {
            self.rules.lock().unwrap().contains_key(id)
        }
    }

    #[async_trait]
    impl WatchRuleStore for StubRuleStore {
        async fn list_all(&self) -> Result<Vec<WatchRule>, String> {
            Ok(self.rules.lock().unwrap().values().cloned().collect())
        }
        async fn insert(&self, rule: &WatchRule) -> Result<(), String> {
            self.rules.lock().unwrap().insert(rule.id.clone(), rule.clone());
            Ok(())
        }
        async fn delete(&self, _workspace_id: &str, id: &str) -> Result<bool, String> {
            Ok(self.rules.lock().unwrap().remove(id).is_some())
        }
    }

    /// 账本桩：记录全部 append。
    #[derive(Default)]
    struct RecordingLedger {
        entries: Mutex<Vec<(String, String, Option<String>, serde_json::Value)>>,
    }

    #[async_trait]
    impl HubLedger for RecordingLedger {
        async fn append(
            &self,
            workspace_id: &str,
            kind: &str,
            thing_id: Option<&str>,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.entries.lock().unwrap().push((
                workspace_id.to_string(),
                kind.to_string(),
                thing_id.map(str::to_string),
                payload,
            ));
            Ok(())
        }
    }

    /// 账本桩：永远失败，记尝试次数。
    #[derive(Default)]
    struct FailingLedger {
        attempts: AtomicUsize,
    }

    #[async_trait]
    impl HubLedger for FailingLedger {
        async fn append(
            &self,
            _workspace_id: &str,
            _kind: &str,
            _thing_id: Option<&str>,
            _payload: serde_json::Value,
        ) -> Result<(), String> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            Err("db down".into())
        }
    }

    fn thing(id: &str) -> Thing {
        Thing {
            id: id.into(),
            name: id.into(),
            status: ThingStatus::Online,
            ..Thing::default()
        }
    }

    fn gt30(thing_id: &str) -> WatchCondition {
        WatchCondition::Property {
            thing_id: thing_id.into(),
            property: "温度".into(),
            op: CompareOp::Gt,
            value: json!(30.0),
        }
    }

    fn rule(id: &str, condition: WatchCondition, persistent: bool) -> WatchRule {
        WatchRule {
            id: id.into(),
            workspace_id: "w1".into(),
            condition,
            persistent,
            timeout_secs: 600,
            created_at: "2026-10-20 00:00:00".into(),
        }
    }

    fn setup(
        things: Vec<Thing>,
    ) -> (
        Arc<StubCache>,
        WatchEngine,
        mpsc::Receiver<WatchHit>,
        Arc<RecordingLedger>,
        Arc<StubRuleStore>,
    ) {
        let cache = Arc::new(StubCache::with_things(things));
        let live = LiveState::new(cache.clone());
        let ledger = Arc::new(RecordingLedger::default());
        let store = Arc::new(StubRuleStore::default());
        let (tx, rx) = mpsc::channel(16);
        let engine = WatchEngine::new(live, ledger.clone(), store.clone(), tx);
        (cache, engine, rx, ledger, store)
    }

    /// 命中发表：hit_tx 收到 WatchHit，账本落 state_change + watch_hit（v=1, matched:true）。
    #[tokio::test]
    async fn hit_sends_channel_and_logs_ledger() {
        let (_cache, mut engine, mut rx, ledger, _store) = setup(vec![thing("t1")]);
        engine.upsert_rule(rule("r1", gt30("t1"), false)).unwrap();
        engine.on_property_change("w1", "t1", "温度", "31.5").await;
        let hit = rx.try_recv().unwrap();
        assert_eq!(hit.rule_id, "r1");
        assert_eq!(hit.workspace_id, "w1");
        assert_eq!(hit.current_value, json!(31.5));
        assert_eq!(hit.summary.values.get("温度").map(String::as_str), Some("31.5"));
        let entries = ledger.entries.lock().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, "state_change"); // I-2：先落状态变更
        assert_eq!(entries[1].0, "w1");
        assert_eq!(entries[1].1, "watch_hit");
        assert_eq!(entries[1].2.as_deref(), Some("t1"));
        assert_eq!(entries[1].3["v"], json!(1));
        assert_eq!(entries[1].3["matched"], json!(true));
    }

    /// only-new-data：注册时已成立 → 先不成立再成立才命中。
    #[tokio::test]
    async fn only_new_data_requires_toggle_before_hit() {
        let (_cache, mut engine, mut rx, _ledger, _store) = setup(vec![thing("t1")]);
        engine.on_property_change("w1", "t1", "温度", "31").await; // 注册前条件已成立
        engine.upsert_rule(rule("r1", gt30("t1"), false)).unwrap();
        engine.on_property_change("w1", "t1", "温度", "32").await; // 仍成立 → 不命中
        assert!(rx.try_recv().is_err());
        engine.on_property_change("w1", "t1", "温度", "25").await; // 不成立 → rearm
        assert!(rx.try_recv().is_err());
        engine.on_property_change("w1", "t1", "温度", "33").await; // 再成立 → 命中
        assert!(rx.try_recv().is_ok());
    }

    /// one-shot：命中即从注册表移除 + 退役写穿 DB（C-2），后续成立不再发。
    #[tokio::test]
    async fn one_shot_removed_after_hit() {
        let (_cache, mut engine, mut rx, _ledger, store) = setup(vec![thing("t1")]);
        let r = rule("r1", gt30("t1"), false);
        store.insert(&r).await.unwrap(); // 模拟 API 写穿后的 DB 行
        engine.upsert_rule(r).unwrap();
        engine.on_property_change("w1", "t1", "温度", "31").await;
        assert!(rx.try_recv().is_ok());
        assert!(!engine.rules.contains_key("r1"));
        assert!(!store.contains("r1"), "one-shot 命中后 DB 行须写穿删除（防重启复活）");
        engine.on_property_change("w1", "t1", "温度", "32").await;
        assert!(rx.try_recv().is_err());
        assert!(!engine.remove_rule("r1"));
    }

    /// persistent：命中后保持；flap 在 1s 合并窗内不重复发；窗过后再跳变可再命中。
    #[tokio::test]
    async fn persistent_merges_hits_within_1s() {
        let (_cache, mut engine, mut rx, ledger, _store) = setup(vec![thing("t1")]);
        engine.upsert_rule(rule("r1", gt30("t1"), true)).unwrap();
        engine.on_property_change("w1", "t1", "温度", "31").await;
        assert!(rx.try_recv().is_ok()); // 第一次命中
        engine.on_property_change("w1", "t1", "温度", "20").await; // 不成立 → rearm
        engine.on_property_change("w1", "t1", "温度", "32").await; // 1s 内再成立 → 合并
        assert!(rx.try_recv().is_err());
        // 合并窗内不落第二笔 watch_hit（state_change 行不在此断言范围）
        assert_eq!(
            ledger
                .entries
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.1 == "watch_hit")
                .count(),
            1
        );
        // 窗过后（回拨上次命中时刻）：先不成立再成立 → 再次命中
        engine
            .last_hit_at
            .insert("r1".into(), Instant::now() - Duration::from_secs(2));
        engine.on_property_change("w1", "t1", "温度", "20").await;
        engine.on_property_change("w1", "t1", "温度", "33").await;
        assert!(rx.try_recv().is_ok());
        assert!(engine.rules.contains_key("r1")); // persistent 命中后保持
    }

    /// 超时清扫：到期规则移除 + 落 watch_timeout（matched:false）+ 退役写穿
    /// DB（C-2），到期后不再命中。
    #[tokio::test]
    async fn timeout_sweep_logs_and_removes() {
        let (_cache, mut engine, mut rx, ledger, store) = setup(vec![thing("t1")]);
        let r = rule("r1", gt30("t1"), false);
        store.insert(&r).await.unwrap();
        engine.upsert_rule(r).unwrap();
        engine.sweep_timeouts(Utc::now() + chrono::Duration::seconds(599)).await;
        assert!(engine.rules.contains_key("r1")); // 未到期不动
        assert!(store.contains("r1"));
        assert!(ledger.entries.lock().unwrap().is_empty());
        engine.sweep_timeouts(Utc::now() + chrono::Duration::seconds(601)).await;
        assert!(!engine.rules.contains_key("r1"));
        assert!(!store.contains("r1"), "超时退役须写穿删除 DB 行");
        {
            let entries = ledger.entries.lock().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].1, "watch_timeout");
            assert_eq!(entries[0].2.as_deref(), Some("t1"));
            assert_eq!(entries[0].3["v"], json!(1));
            assert_eq!(entries[0].3["matched"], json!(false));
        }
        engine.on_property_change("w1", "t1", "温度", "31").await;
        assert!(rx.try_recv().is_err()); // 已超时移除，不再命中
    }

    /// I-1：persistent 规则免超时清扫——过超时点仍保持注册且可正常命中。
    #[tokio::test]
    async fn persistent_rules_are_not_swept() {
        let (_cache, mut engine, mut rx, ledger, _store) = setup(vec![thing("t1")]);
        engine.upsert_rule(rule("r1", gt30("t1"), true)).unwrap();
        engine
            .sweep_timeouts(Utc::now() + chrono::Duration::seconds(3601))
            .await;
        assert!(engine.rules.contains_key("r1"), "persistent 规则不应被超时清扫");
        assert!(ledger.entries.lock().unwrap().is_empty());
        engine.on_property_change("w1", "t1", "温度", "31").await;
        assert!(rx.try_recv().is_ok(), "过超时点后 persistent 规则仍正常命中");
    }

    /// C-1：存在性校验单点在 API 层（DB 真相）；engine 对未知物规则登记为
    /// armed（only-new-data 天然容错），首次事件进场即正常命中。非法规则
    /// （timeout_secs=0）仍拒绝。
    #[tokio::test]
    async fn upsert_unknown_thing_arms_and_hits_on_first_event() {
        let (_cache, mut engine, mut rx, _ledger, _store) = setup(vec![]);
        engine.upsert_rule(rule("r1", gt30("ghost"), false)).unwrap();
        assert!(engine.rules["r1"].armed, "无实时态 = 不成立 = armed");
        let mut bad = rule("r2", gt30("t1"), false);
        bad.timeout_secs = 0;
        assert!(engine.upsert_rule(bad).is_err());
        assert_eq!(engine.active_rule_count(), 1);
        // 首次事件进场即正常求值命中
        engine.on_property_change("w1", "ghost", "温度", "31").await;
        assert_eq!(rx.try_recv().unwrap().rule_id, "r1");
    }

    /// I-2 / spec §4.2：属性状态变更全量落账（kind=state_change，v=1，带字段
    /// 与新旧值），温度 25→31 的演变可回放。
    #[tokio::test]
    async fn property_change_appends_state_change_ledger_rows() {
        let (_cache, mut engine, _rx, ledger, _store) = setup(vec![thing("t1")]);
        engine.on_property_change("w1", "t1", "温度", "25").await;
        engine.on_property_change("w1", "t1", "温度", "31").await;
        let entries = ledger.entries.lock().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.1 == "state_change"));
        assert!(entries.iter().all(|e| e.0 == "w1" && e.2.as_deref() == Some("t1")));
        assert_eq!(entries[0].3["v"], json!(1));
        assert_eq!(entries[0].3["field"], json!("温度"));
        assert_eq!(entries[0].3["old"], json!(null));
        assert_eq!(entries[0].3["new"], json!("25"));
        assert_eq!(entries[1].3["old"], json!("25"));
        assert_eq!(entries[1].3["new"], json!("31"));
    }

    /// 边界：账本写失败重试 3 次 → warn + 计数，命中投递不中断。
    /// （I-2 后一次命中路径写两笔：state_change + watch_hit，各重试 3 次。）
    #[tokio::test]
    async fn ledger_write_failure_retries_and_does_not_interrupt() {
        let cache = Arc::new(StubCache::with_things(vec![thing("t1")]));
        let live = LiveState::new(cache);
        let ledger = Arc::new(FailingLedger::default());
        let store = Arc::new(StubRuleStore::default());
        let (tx, mut rx) = mpsc::channel(16);
        let mut engine = WatchEngine::new(live, ledger.clone(), store, tx);
        engine.upsert_rule(rule("r1", gt30("t1"), false)).unwrap();
        engine.on_property_change("w1", "t1", "温度", "31").await;
        assert!(rx.try_recv().is_ok()); // 值守链路不因账本故障中断
        assert_eq!(ledger.attempts.load(Ordering::Relaxed), 2 * LEDGER_MAX_ATTEMPTS);
        assert_eq!(engine.ledger_write_failures(), 2);
    }

    /// 边界：invalidate_thing 使该物活跃 watch 全失效 + 逐条落 watch_expired +
    /// 退役写穿 DB（C-2）；他物不受影响。
    #[tokio::test]
    async fn invalidate_thing_expires_watches() {
        let (_cache, mut engine, mut rx, ledger, store) = setup(vec![thing("t1"), thing("t2")]);
        for r in [
            rule("r1", gt30("t1"), false),
            rule(
                "r2",
                WatchCondition::Availability {
                    thing_id: "t1".into(),
                    online: false,
                },
                true,
            ),
            rule("r3", gt30("t2"), false),
        ] {
            store.insert(&r).await.unwrap();
            engine.upsert_rule(r).unwrap();
        }
        engine.invalidate_thing("t1").await;
        assert!(!engine.rules.contains_key("r1"));
        assert!(!engine.rules.contains_key("r2"));
        assert!(engine.rules.contains_key("r3"));
        assert!(
            !store.contains("r1") && !store.contains("r2"),
            "失效规则须写穿删除 DB 行"
        );
        assert!(store.contains("r3"));
        {
            let entries = ledger.entries.lock().unwrap();
            assert_eq!(entries.len(), 2);
            assert!(entries.iter().all(|e| e.1 == "watch_expired"));
            assert!(entries.iter().all(|e| e.2.as_deref() == Some("t1")));
            assert!(entries.iter().all(|e| e.3["v"] == json!(1)));
        }
        // 他物规则仍正常命中
        engine.on_property_change("w1", "t2", "温度", "31").await;
        assert_eq!(rx.try_recv().unwrap().rule_id, "r3");
    }

    /// health 词表：LiveState 派生 health 变为 bad 时命中（cache 真相刷新驱动）。
    #[tokio::test]
    async fn health_watch_hits_when_status_turns_bad() {
        let (cache, mut engine, mut rx, _ledger, _store) = setup(vec![thing("t1")]);
        engine
            .upsert_rule(rule(
                "r1",
                WatchCondition::Health {
                    thing_id: "t1".into(),
                    level: HealthLevel::Bad,
                },
                false,
            ))
            .unwrap();
        engine.on_property_change("w1", "t1", "温度", "20").await; // 仍 Online → 不命中
        assert!(rx.try_recv().is_err());
        cache.update(Thing {
            id: "t1".into(),
            name: "t1".into(),
            status: ThingStatus::Error,
            ..Thing::default()
        });
        engine.on_property_change("w1", "t1", "温度", "21").await; // 刷新真相 → Bad → 命中
        let hit = rx.try_recv().unwrap();
        assert_eq!(hit.health, Health::Bad);
    }

    /// availability 词表：设备下线切换命中（cache 先写后发，cache 真相驱动）。
    #[tokio::test]
    async fn availability_watch_hits_on_offline() {
        let (cache, mut engine, mut rx, _ledger, _store) = setup(vec![thing("t1")]);
        engine
            .upsert_rule(rule(
                "r1",
                WatchCondition::Availability {
                    thing_id: "t1".into(),
                    online: false,
                },
                false,
            ))
            .unwrap();
        engine.on_availability("w1", "t1", true).await; // 在线事件 → 不命中
        assert!(rx.try_recv().is_err());
        cache.update(Thing {
            id: "t1".into(),
            name: "t1".into(),
            status: ThingStatus::Offline,
            ..Thing::default()
        });
        engine.on_availability("w1", "t1", false).await; // 离线 → 命中
        assert!(rx.try_recv().is_ok());
    }

    /// 幂等：同 id 重注册摘除旧索引，旧条件不再命中、新条件命中一次。
    #[tokio::test]
    async fn upsert_same_id_replaces_without_duplicate() {
        let (_cache, mut engine, mut rx, _ledger, _store) = setup(vec![thing("t1")]);
        engine.upsert_rule(rule("r1", gt30("t1"), false)).unwrap();
        engine
            .upsert_rule(rule(
                "r1",
                WatchCondition::Property {
                    thing_id: "t1".into(),
                    property: "湿度".into(),
                    op: CompareOp::Gt,
                    value: json!(30.0),
                },
                false,
            ))
            .unwrap();
        assert_eq!(engine.active_rule_count(), 1);
        engine.on_property_change("w1", "t1", "温度", "31").await; // 旧条件已摘除
        assert!(rx.try_recv().is_err());
        engine.on_property_change("w1", "t1", "湿度", "31").await; // 新条件命中一次
        assert!(rx.try_recv().is_ok());
        assert!(rx.try_recv().is_err());
    }
}
