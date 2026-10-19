//! 派生驱动管理器（spec §三）：反向索引、生命周期、stale 内存态、三类触发源入口。
//!
//! **硬性规则（SDD 终审 2026-10-20，已两次漏接后立下）**：任何新的物管理面
//! 变更点（REST/MCP/场景包实例化/批量导入/未来的管理面）在建物、删物、换父、
//! 增删派生属性时，必须接对应钩子：`notify_config_changed`（派生配置增删改）、
//! `notify_tree_changed`（建子物/删子物/换父）、`notify_thing_deleted`（删物）。
//! 漏接 = 派生静默失效（索引不热更新、不重算、不标 stale），无报错可查。
//!
//! - `start`：扫派生定义重建索引（F-F：表达式/refs JSON 损坏行 fail-closed
//!   标 stale 不阻断），为每个 owner 物注册 `DerivedDriver`（mode=Push）进
//!   与 DataServer 共享的 driver_cache——轮询循环按 mode 跳过，读数经
//!   value_sink → 共享管线回流；
//! - 热更新（抉择点②）：Manager 与每个驱动共享 `DerivedDefsHandle`
//!   （`Arc<RwLock<Vec<DerivedDef>>>`），配置变更直接改写，不给
//!   DriverWrapper/core ThingDriver 加任何下行访问器；
//! - 三类触发源：`notify_config_changed`（配置增删改）、
//!   `notify_tree_changed`（子树变更 → descendant_count 重算）、
//!   `notify_thing_deleted`（F-B：受影响派生标 stale + 索引/refs 热更新）。

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use tinyiothub_core::driver::{DriverMode, DriverReading, PropertyChangeSource, ThingDriver, ValueSink};
use tinyiothub_core::models::thing_property::ThingProperty;

use crate::data_server::DriverCache;
use crate::driver::derived::{
    DerivedDef, DerivedDefsHandle, DerivedDriver, DerivedStatus, DerivedStatusMap, DescendantCounter,
    ast_has_descendant_count, now_string, recompute_defs, rewrite_refs_to_ids,
};
use crate::driver::wrapper::DriverWrapper;
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
    driver_cache: DriverCache,
    sink: ValueSink,
    events: Arc<dyn PropertyChangeSource>,
    /// (ref_thing_id, ref_property_name) → [owner_thing_id]
    index: RwLock<HashMap<(String, String), Vec<String>>>,
    /// property_id → 状态（内存态不落库，spec §二 E4；与驱动共享）
    status: DerivedStatusMap,
    /// owner_thing_id → 驱动共享句柄（热更新/手动触发重算入口）
    owners: RwLock<HashMap<String, OwnerEntry>>,
}

impl DerivedDriverManager {
    pub fn new(
        queries: Arc<dyn DerivedPropertyQueries>,
        cache: Arc<dyn ThingCacheSource>,
        driver_cache: DriverCache,
        sink: ValueSink,
        events: Arc<dyn PropertyChangeSource>,
    ) -> Self {
        Self {
            queries,
            cache,
            driver_cache,
            sink,
            events,
            index: RwLock::new(HashMap::new()),
            status: Arc::new(RwLock::new(HashMap::new())),
            owners: RwLock::new(HashMap::new()),
        }
    }

    /// 启动：扫派生定义重建索引（F-F：JSON 损坏行 fail-closed 标 stale 不阻断），
    /// 为每个 owner 物注册 DerivedDriver（注册即首值补偿）。
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
            self.ensure_driver(&owner_id, defs).await?;
        }
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

    /// 注册或热更新 owner 驱动。热更新走共享 defs 句柄改写（抉择点②），
    /// 不触碰 DriverWrapper。
    ///
    /// 热路径自愈（Task 13 评审 Important-1）：REST 建物/场景包实例化不写
    /// device_cache（仅 bootstrap `load_device_cache` 重启路径全量写入），
    /// cache miss 时从库加载 owner 物（含属性）进缓存后继续注册——运行中
    /// 建物→配派生不再静默失效到重启。
    async fn ensure_driver(&self, owner_id: &str, defs: Vec<DerivedDef>) -> Result<(), String> {
        if let Some(handle) = self.owners.read().get(owner_id).map(|e| e.defs.clone()) {
            *handle.write() = defs;
            return Ok(());
        }
        let thing = match self.cache.get(owner_id) {
            Some(t) => t,
            None => {
                // DB 加载在写锁外做（check-then-act 原子性：下方双检兜底）。
                let Some(t) = self.queries.find_thing_with_properties(owner_id).await? else {
                    return Err(format!("owner 物 {owner_id} 不存在（缓存与库均无）"));
                };
                self.cache.insert(t.clone());
                t
            }
        };
        // await 窗口内可能已有并发注册——双检，后到者退化为热更新。
        if let Some(handle) = self.owners.read().get(owner_id).map(|e| e.defs.clone()) {
            *handle.write() = defs;
            return Ok(());
        }
        let counter: Arc<dyn DescendantCounter> = Arc::new(PortCounter {
            queries: self.queries.clone(),
            owner_id: owner_id.to_string(),
        });
        let defs_handle: DerivedDefsHandle = Arc::new(RwLock::new(defs));
        let mut driver = DerivedDriver::new(
            thing,
            defs_handle.clone(),
            self.cache.clone(),
            counter.clone(),
            self.status.clone(),
        );
        driver.start_push(self.sink.clone(), self.events.clone());
        // 与 DataServer 同一 driver_cache（parking_lot wrapper，迭代风格一致）：
        // mode()=Push 使轮询跳过；sink 消费侧按 thing_id 找到本 wrapper 走共享管线。
        //
        // 终审 I-1 防御：同 key 已有 mode=Poll 物理 wrapper 时绝不顶掉（顶掉 =
        // 该物轮询与命令静默死亡）。API 侧已拒绝物理驱动物建派生，此处是启动
        // 扫库/存量数据路径的兜底——log error 后跳过本 owner，不影响其余注册。
        if let Some(existing) = self.driver_cache.get(owner_id)
            && existing.read().inner_driver().mode() == DriverMode::Poll
        {
            tracing::error!(
                owner = %owner_id,
                "derived driver 注册跳过：driver_cache 已有物理驱动 wrapper（mode=Poll），拒绝覆盖"
            );
            return Ok(());
        }
        self.driver_cache.insert(
            owner_id.to_string(),
            Arc::new(parking_lot::RwLock::new(DriverWrapper::new(Box::new(driver)))),
        );
        self.owners.write().insert(
            owner_id.to_string(),
            OwnerEntry {
                defs: defs_handle,
                counter,
            },
        );
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
        if defs.is_empty() && !self.owners.read().contains_key(owner_thing_id) {
            return; // 无派生定义且无存量驱动，无需注册
        }
        if let Err(e) = self.ensure_driver(owner_thing_id, defs).await {
            tracing::error!(error = %e, owner = %owner_thing_id, "derived driver ensure failed");
        }
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
    /// 驱动 refs 热更新；owner 自身被删则移除驱动注册。
    pub async fn notify_thing_deleted(&self, thing_id: &str) {
        let mut affected: Vec<String> = {
            let mut idx = self.index.write();
            let affected = idx
                .iter()
                .filter(|((tid, _), _)| tid == thing_id)
                .flat_map(|(_, owners)| owners.clone())
                .collect::<Vec<_>>();
            idx.retain(|(tid, _), _| tid != thing_id);
            affected
        };
        affected.sort();
        affected.dedup();

        // owner 自身被删：摘除驱动（其订阅任务随缓存清空自然失活——所有引用
        // 解析都拿不到值，不再产生上报）。
        if self.owners.write().remove(thing_id).is_some() {
            self.driver_cache.invalidate(thing_id);
        }

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
    }

    /// Task 8 GET 注入读源：某派生属性的运行状态。
    pub fn derived_status(&self, property_id: &str) -> Option<DerivedStatus> {
        self.status.read().get(property_id).cloned()
    }
}
