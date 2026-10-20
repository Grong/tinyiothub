//! 派生引擎管理器（spec §三；X4 单引擎化）：全 workspace **一个** EventBus
//! 订阅 + 反向索引 + 直发 sink，不再有 per-owner DerivedDriver wrapper。
//!
//! **硬性规则（SDD 终审 2026-10-20，已两次漏接后立下）**：任何新的物管理面
//! 变更点（REST/MCP/场景包实例化/批量导入/未来的管理面）在建物、删物、换父、
//! 增删派生属性时，必须接对应钩子：`notify_config_changed`（派生配置增删改）、
//! `notify_tree_changed`（建子物/删子物/换父）、`notify_thing_deleted`（删物）。
//! 漏接 = 派生静默失效（索引不热更新、不重算、不标 stale），无报错可查。
//!
//! - `start`：扫派生定义重建索引（F-F：表达式/refs JSON 损坏行 fail-closed
//!   标 stale 不阻断），为每个 owner 物建 defs 句柄 + 首值补偿；引擎订阅任务
//!   全 workspace 唯一——有 owner 即跑、无 owner 即停（`sync_engine`）。读数
//!   经 value_sink → `DataServer::consume_driver_reading` 共享管线回流
//!   （无 wrapper 的 owner 物走 device_cache 回退，Task 1）；
//! - 热更新（抉择点②）：Manager 与引擎共享每 owner 的 `DerivedDefsHandle`
//!   （`Arc<RwLock<Vec<DerivedDef>>>`），配置变更直接改写；
//! - 事件命中：反向索引 (ref_thing_id, ref_property_name) → [owner]，命中后
//!   def 级 property 过滤——只重算引用该 (thing, property) 的 defs（收编
//!   Task 7 台账"只比 thing_id 过触发"Minor）；
//! - 三类触发源：`notify_config_changed`（配置增删改）、
//!   `notify_tree_changed`（子树变更 → descendant_count 重算）、
//!   `notify_thing_deleted`（F-B：受影响派生标 stale + 索引/refs 热更新）。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use parking_lot::RwLock;
use tokio::sync::broadcast::error::RecvError;

use tinyiothub_core::driver::{DriverReading, PropertyChangeSource, ValueSink};
use tinyiothub_core::models::event::{Event, EventType, ThingEventType};
use tinyiothub_core::models::thing_property::ThingProperty;

use crate::driver::derived::{
    DERIVED_EVENTBUS_LAGGED, DerivedDef, DerivedDefsHandle, DerivedStatus, DerivedStatusMap, DescendantCounter,
    ast_has_descendant_count, now_string, recompute_defs, rewrite_refs_to_ids,
};
use crate::expression;
use crate::ports::{DerivedPropertyQueries, ThingCacheSource};

struct OwnerEntry {
    defs: DerivedDefsHandle,
    counter: Arc<dyn DescendantCounter>,
}

/// `DerivedPropertyQueries` → `DescendantCounter`（root = owner 物）。
struct PortCounter {
    queries: Arc<dyn DerivedPropertyQueries>,
    owner_id: String,
}

#[async_trait::async_trait]
impl DescendantCounter for PortCounter {
    async fn count(&self, thing_type: Option<&str>) -> Result<f64, String> {
        self.queries
            .count_descendants(&self.owner_id, thing_type)
            .await
            .map(|n| n as f64)
    }
}

pub struct DerivedDriverManager {
    queries: Arc<dyn DerivedPropertyQueries>,
    cache: Arc<dyn ThingCacheSource>,
    sink: ValueSink,
    events: Arc<dyn PropertyChangeSource>,
    /// (ref_thing_id, ref_property_name) → [owner_thing_id]
    index: Arc<RwLock<HashMap<(String, String), Vec<String>>>>,
    /// property_id → 状态（内存态不落库，spec §二 E4；与引擎共享）
    status: DerivedStatusMap,
    /// owner_thing_id → 引擎共享句柄（热更新/手动触发重算入口）
    owners: Arc<RwLock<HashMap<String, OwnerEntry>>>,
    /// 全 workspace 唯一的事件订阅任务（X4 单引擎；有 owner 即跑、无 owner 即停）
    engine: RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl DerivedDriverManager {
    pub fn new(
        queries: Arc<dyn DerivedPropertyQueries>,
        cache: Arc<dyn ThingCacheSource>,
        sink: ValueSink,
        events: Arc<dyn PropertyChangeSource>,
    ) -> Self {
        Self {
            queries,
            cache,
            sink,
            events,
            index: Arc::new(RwLock::new(HashMap::new())),
            status: Arc::new(RwLock::new(HashMap::new())),
            owners: Arc::new(RwLock::new(HashMap::new())),
            engine: RwLock::new(None),
        }
    }

    /// 启动：扫派生定义重建索引（F-F：JSON 损坏行 fail-closed 标 stale 不阻断），
    /// 为每个 owner 物建 defs 句柄 + 首值补偿，末尾起引擎（单订阅）。
    pub async fn start(&self) -> Result<(), String> {
        let derived = self.queries.list_derived().await?;
        let mut by_owner: HashMap<String, Vec<DerivedDef>> = HashMap::new();
        for p in &derived {
            match Self::def_of(p) {
                Ok(def) => {
                    for (tid, pname) in &def.refs {
                        self.index
                            .write()
                            .entry((tid.clone(), pname.clone()))
                            .or_default()
                            .push(p.thing_id.clone());
                    }
                    by_owner.entry(p.thing_id.clone()).or_default().push(def);
                }
                Err(reason) => {
                    // F-F fail-closed：损坏行标 stale，不阻断其余定义。
                    self.status.write().insert(
                        p.id.clone(),
                        DerivedStatus {
                            stale: true,
                            stale_reason: Some(reason),
                            last_eval_at: None,
                        },
                    );
                }
            }
        }
        for (owner_id, defs) in by_owner {
            self.ensure_owner(&owner_id, defs).await?;
        }
        self.sync_engine();
        Ok(())
    }

    fn def_of(p: &ThingProperty) -> Result<DerivedDef, String> {
        let expr = p.expression.clone().unwrap_or_default();
        let ast = expression::parse(&expr).map_err(|e| format!("表达式解析失败: {e}"))?;
        let raw: Vec<serde_json::Value> =
            serde_json::from_str(p.refs.as_deref().unwrap_or("[]")).map_err(|e| format!("refs JSON 损坏: {e}"))?;
        let refs = raw
            .iter()
            .filter_map(|v| {
                Some((
                    v.get("thing_id")?.as_str()?.to_string(),
                    v.get("property_name")?.as_str()?.to_string(),
                ))
            })
            .collect::<Vec<_>>();
        let has_dc = ast_has_descendant_count(&ast);
        // 终审 I-2：AST 物名改写为 thing_id（spec §二 refs 以 id 为准），
        // 求值不再依赖物名——引用物改名后重算仍出值。
        let ast = rewrite_refs_to_ids(&ast, &refs);
        Ok(DerivedDef {
            property_id: p.id.clone(),
            property_name: p.name.clone(),
            ast,
            refs,
            has_descendant_count: has_dc,
        })
    }

    /// 引擎生命周期 = 有 owner 即跑、无 owner 即停（全 workspace 单订阅）。
    /// subscribe 在 spawn 前同步完成（E5）：本函数返回即订阅已建立，调用方
    /// 可凭 `subscriber_count` 确认，禁裸 sleep。
    ///
    /// 竞态防护（X4 复审 Important）：owners 快照与 spawn/abort 决策必须在同一
    /// 临界区——`engine.write()` 持锁期间重读 owners，消除"读快照 → 拿决策锁"
    /// 窗口（并发 notify 曾可产出两种错误终态：A 已登记但引擎被误停 / 零 owner
    /// 但孤儿订阅存活）。正确性依据：owners 的变更恒先于其对应 sync_engine 调用
    /// （notify_* 内顺序），决策经 engine 写锁串行化后到者必见最新 owners。
    /// 锁序恒为 engine → owners；引擎订阅任务只读 owners/index/status、从不取
    /// engine 锁，notify_* 写 owners 时不持 engine 锁——不构成锁序环
    /// （parking_lot RwLock 读套写非升级路径，无死锁）。
    fn sync_engine(&self) {
        let mut engine = self.engine.write();
        let should_run = !self.owners.read().is_empty();
        match (should_run, engine.is_some()) {
            (true, false) => *engine = Some(self.spawn_engine()),
            (false, true) => {
                if let Some(h) = engine.take() {
                    h.abort();
                }
            }
            _ => {}
        }
    }

    /// 全 workspace 唯一订阅任务：PropertyChange → 反向索引命中（thing 级
    /// 收集 + property 级精确匹配）→ def 级过滤重算 → 直发 sink；Lagged
    /// 计数并全量重算自愈（spec §三，输入永远是最新缓存）。
    fn spawn_engine(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.events.subscribe(); // E5：同步 subscribe，见 sync_engine
        let index = self.index.clone();
        let owners = self.owners.clone();
        let cache = self.cache.clone();
        let sink = self.sink.clone();
        let status = self.status.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if !matches!(event.event_type(), EventType::Device(ThingEventType::PropertyChange)) {
                            continue;
                        }
                        let Some(src_thing) = event.source().thing_id().map(str::to_string) else {
                            continue;
                        };
                        // def 级 property 过滤的输入；解析不出 → None 退化 thing 级。
                        let prop_name = Self::event_property_name(&cache, &event);
                        let matches = |t: &String, p: &String| {
                            *t == src_thing && prop_name.as_ref().is_none_or(|pn| p == pn)
                        };
                        let mut hit_owners: Vec<String> = {
                            let idx = index.read();
                            idx.iter()
                                .filter(|((tid, pname), _)| matches(tid, pname))
                                .flat_map(|(_, os)| os.iter().cloned())
                                .collect()
                        };
                        hit_owners.sort();
                        hit_owners.dedup();
                        for owner in hit_owners {
                            let Some((defs_handle, counter)) = owners
                                .read()
                                .get(&owner)
                                .map(|e| (e.defs.clone(), e.counter.clone()))
                            else {
                                continue;
                            };
                            // 只重算引用该 (thing, property) 的 defs。
                            let snapshot: Vec<DerivedDef> = defs_handle
                                .read()
                                .iter()
                                .filter(|d| d.refs.iter().any(|(t, p)| matches(t, p)))
                                .cloned()
                                .collect();
                            if snapshot.is_empty() {
                                continue;
                            }
                            let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                            if !values.is_empty() {
                                let _ = sink.send(DriverReading { thing_id: owner, values }).await;
                            }
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        DERIVED_EVENTBUS_LAGGED.fetch_add(n, Ordering::Relaxed);
                        // 自愈（spec §三）：lag 后全量重算，输入永远是最新缓存。
                        let targets: Vec<(String, DerivedDefsHandle, Arc<dyn DescendantCounter>)> = owners
                            .read()
                            .iter()
                            .map(|(id, e)| (id.clone(), e.defs.clone(), e.counter.clone()))
                            .collect();
                        for (owner, defs_handle, counter) in targets {
                            let snapshot = defs_handle.read().clone();
                            let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                            if !values.is_empty() {
                                let _ = sink.send(DriverReading { thing_id: owner, values }).await;
                            }
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        })
    }

    /// 事件源属性名解析：device_property 源的 source_id = "{thing_id}:{property_id}"，
    /// 按源物属性 id → 名解析（refs 以名匹配）；非属性源/物已出缓存返回 None。
    fn event_property_name(cache: &Arc<dyn ThingCacheSource>, event: &Event) -> Option<String> {
        let src = event.source();
        let thing_id = src.thing_id()?;
        let prop_id = src.source_id().strip_prefix(thing_id)?.strip_prefix(':')?;
        let thing = cache.get(thing_id)?;
        thing
            .properties?
            .into_iter()
            .find(|p| p.id == prop_id)
            .map(|p| p.name)
    }

    /// 建/热更新 owner 的 defs 句柄 + 首值补偿。热更新走共享 defs 句柄改写
    /// （抉择点②）。不再注册任何 driver wrapper（X4：引擎单订阅直发 sink，
    /// sink 消费侧经 device_cache 回退写 owner 物）。
    ///
    /// 热路径自愈（Task 13 评审 Important-1）：REST 建物/场景包实例化不写
    /// device_cache（仅 bootstrap `load_device_cache` 重启路径全量写入），
    /// cache miss 时从库加载 owner 物（含属性）进缓存——运行中建物→配派生
    /// 不再静默失效到重启。
    async fn ensure_owner(&self, owner_id: &str, defs: Vec<DerivedDef>) -> Result<(), String> {
        if let Some(handle) = self.owners.read().get(owner_id).map(|e| e.defs.clone()) {
            *handle.write() = defs;
            return Ok(());
        }
        if self.cache.get(owner_id).is_none() {
            // DB 加载在写锁外做（check-then-act 原子性：下方双检兜底）。
            let Some(t) = self.queries.find_thing_with_properties(owner_id).await? else {
                return Err(format!("owner 物 {owner_id} 不存在（缓存与库均无）"));
            };
            self.cache.insert(t);
        }
        // await 窗口内可能已有并发登记——双检，后到者退化为热更新。
        if let Some(handle) = self.owners.read().get(owner_id).map(|e| e.defs.clone()) {
            *handle.write() = defs;
            return Ok(());
        }
        let counter: Arc<dyn DescendantCounter> = Arc::new(PortCounter {
            queries: self.queries.clone(),
            owner_id: owner_id.to_string(),
        });
        let defs_handle: DerivedDefsHandle = Arc::new(RwLock::new(defs));
        self.owners.write().insert(
            owner_id.to_string(),
            OwnerEntry {
                defs: defs_handle.clone(),
                counter: counter.clone(),
            },
        );
        // 首值补偿（spec §三：登记即全量重算）。独立 spawn——与引擎订阅循环
        // 任务隔离（T1：补偿任务宿主 panic 不扩散，下一事件触发即自愈）。
        {
            let cache = self.cache.clone();
            let status = self.status.clone();
            let sink = self.sink.clone();
            let owner = owner_id.to_string();
            tokio::spawn(async move {
                let snapshot = defs_handle.read().clone();
                let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                if !values.is_empty() {
                    let _ = sink.send(DriverReading { thing_id: owner, values }).await;
                }
            });
        }
        Ok(())
    }

    /// 触发源①：物管理面配置增删改——重建该 owner 的 defs + 反向索引。
    pub async fn notify_config_changed(&self, owner_thing_id: &str) {
        let derived = match self.queries.list_derived().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, owner = %owner_thing_id, "derived config reload failed");
                return;
            }
        };
        // 撤掉该 owner 旧索引项
        self.index.write().retain(|_, owners| {
            owners.retain(|o| o != owner_thing_id);
            !owners.is_empty()
        });
        let mut defs = Vec::new();
        for p in derived.iter().filter(|p| p.thing_id == owner_thing_id) {
            match Self::def_of(p) {
                Ok(def) => {
                    for (tid, pname) in &def.refs {
                        self.index
                            .write()
                            .entry((tid.clone(), pname.clone()))
                            .or_default()
                            .push(owner_thing_id.to_string());
                    }
                    defs.push(def);
                }
                Err(reason) => {
                    self.status.write().insert(
                        p.id.clone(),
                        DerivedStatus {
                            stale: true,
                            stale_reason: Some(reason),
                            last_eval_at: None,
                        },
                    );
                }
            }
        }
        if defs.is_empty() {
            // 终审 M-a：defs 空时回收 owner 登记——空 defs 的 OwnerEntry 永留
            // owners 表会让 sync_engine 的 should_run 恒 true（订阅空转到删物）。
            self.owners.write().remove(owner_thing_id);
            self.sync_engine();
            return;
        }
        if let Err(e) = self.ensure_owner(owner_thing_id, defs).await {
            tracing::error!(error = %e, owner = %owner_thing_id, "derived owner ensure failed");
        }
        self.sync_engine();
    }

    /// 触发源②：子树变更——含 descendant_count 的 defs 重算。
    /// 保守策略：重算全部含 dc 的 owner（树变更低频；计数经 port 查库，
    /// 不按 root 过滤子树——dc 求值本身以 owner 为根，语义不受影响）。
    pub async fn notify_tree_changed(&self, root_thing_id: &str) {
        let targets: Vec<(String, DerivedDefsHandle, Arc<dyn DescendantCounter>)> = self
            .owners
            .read()
            .iter()
            .filter(|(_, e)| e.defs.read().iter().any(|d| d.has_descendant_count))
            .map(|(id, e)| (id.clone(), e.defs.clone(), e.counter.clone()))
            .collect();
        tracing::debug!(root = %root_thing_id, owners = targets.len(), "derived tree-change recompute");
        for (owner_id, defs_handle, counter) in targets {
            let snapshot = defs_handle.read().clone();
            let values = recompute_defs(&snapshot, &self.cache, &counter, &self.status).await;
            if !values.is_empty() {
                let _ = self
                    .sink
                    .send(DriverReading {
                        thing_id: owner_id,
                        values,
                    })
                    .await;
            }
        }
    }

    /// 触发源③：物删除（F-B）——受影响派生标 stale（"引用缺失"）、反向索引与
    /// refs 热更新；owner 自身被删则摘除登记（owner 删光时引擎随之停订阅）。
    pub async fn notify_thing_deleted(&self, thing_id: &str) {
        let mut affected: Vec<String> = {
            let mut idx = self.index.write();
            let affected = idx
                .iter()
                .filter(|((tid, _), _)| tid == thing_id)
                .flat_map(|(_, owners)| owners.clone())
                .collect::<Vec<_>>();
            idx.retain(|(tid, _), _| tid != thing_id);
            // owner 侧残留一并 scrub（终审 M-a）：被删物若是派生 owner，其 id
            // 可能留在其他 ref key 的 owner vec 里（引擎侧 owners.get miss 有
            // continue 兜底，内存级无害，但索引应保持干净）。
            idx.retain(|_, owners| {
                owners.retain(|o| o != thing_id);
                !owners.is_empty()
            });
            affected
        };
        affected.sort();
        affected.dedup();

        // owner 自身被删：摘除 defs 句柄——所有引用解析都拿不到值，不再产生上报。
        self.owners.write().remove(thing_id);

        for owner in affected {
            if owner == thing_id {
                continue;
            }
            let Some(handle) = self.owners.read().get(&owner).map(|e| e.defs.clone()) else {
                continue;
            };
            for d in handle.write().iter_mut() {
                if d.refs.iter().any(|(tid, _)| tid == thing_id) {
                    d.refs.retain(|(tid, _)| tid != thing_id);
                    self.status.write().insert(
                        d.property_id.clone(),
                        DerivedStatus {
                            stale: true,
                            stale_reason: Some(format!("引用缺失：物 {thing_id} 已删除")),
                            last_eval_at: Some(now_string()),
                        },
                    );
                }
            }
        }
        self.sync_engine();
    }

    /// Task 8 GET 注入读源：某派生属性的运行状态。
    pub fn derived_status(&self, property_id: &str) -> Option<DerivedStatus> {
        self.status.read().get(property_id).cloned()
    }
}
