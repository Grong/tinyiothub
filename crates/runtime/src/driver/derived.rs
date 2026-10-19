//! 派生驱动（spec §一/§三）：平台内置推送驱动，挂在拥有派生属性的物上。
//! 订阅 EventBus 的 PropertyChange，命中引用表即重算并经 sink 上报。
//!
//! 设计要点：
//! - `mode() = Push`：不经 `process_all_drivers` 轮询拉取（spec §一）；
//! - 首值补偿：`start_push` 立即全量重算一次（spec §三）；
//! - lag 自愈：broadcast Lagged 计数进 `DERIVED_EVENTBUS_LAGGED` 并全量重算，
//!   输入永远是最新缓存（spec §三）；
//! - recompute 全链路 **无 panic 面**（T1 监督降级依据）：parse/eval 错误全部
//!   映射为 stale 状态，sink 发送失败仅丢弃，路径上无 unwrap/expect。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;
use tokio::sync::broadcast::error::RecvError;

use tinyiothub_core::driver::{DriverMode, DriverReading, PropertyChangeSource, ResultValue, ThingDriver, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::{EventType, ThingEventType};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;

use crate::expression::{self, Ast, EvalError, PropertyRef};
use crate::ports::ThingCacheSource;

/// 一条派生定义：AST + 解析后的引用（thing_id 级，spec §二 refs 以 id 为准）。
#[derive(Debug, Clone)]
pub struct DerivedDef {
    pub property_id: String,
    pub property_name: String,
    pub ast: Ast,
    /// (ref_thing_id, ref_property_name)——事件命中表，以 thing_id 为准。
    pub refs: Vec<(String, String)>,
    pub has_descendant_count: bool,
}

/// 派生属性运行状态（内存态不落库，spec §二 E4；Task 8 GET 注入读源）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DerivedStatus {
    pub stale: bool,
    pub stale_reason: Option<String>,
    pub last_eval_at: Option<String>,
}

/// property_id → 状态。Manager 与 DerivedDriver 共享（驱动重算时写入，
/// Manager 经 `derived_status` 透出）。
pub type DerivedStatusMap = Arc<RwLock<HashMap<String, DerivedStatus>>>;

/// 每 owner 物的 defs 共享句柄（热更新抉择点②：Manager 与驱动共享同一把
/// `RwLock<Vec<DerivedDef>>`，配置变更时 Manager 直接改写，不动 DriverWrapper /
/// core ThingDriver trait——零下行转型、零 trait 改动）。
pub type DerivedDefsHandle = Arc<RwLock<Vec<DerivedDef>>>;

/// descendant_count 求值器（抉择点①：recompute 全链路 async，先把计数值经
/// port 异步取好，再调同步 `expression::eval`——eval 保持纯逻辑不动）。
#[async_trait::async_trait]
pub trait DescendantCounter: Send + Sync {
    async fn count(&self, thing_type: Option<&str>) -> Result<f64, String>;
}

/// broadcast Lagged 累计（spec §三 观测指标；测试断言 >0）。
pub static DERIVED_EVENTBUS_LAGGED: AtomicU64 = AtomicU64::new(0);

pub struct DerivedDriver {
    thing: Thing,
    defs: DerivedDefsHandle,
    cache: Arc<dyn ThingCacheSource>,
    counter: Arc<dyn DescendantCounter>,
    status: DerivedStatusMap,
}

impl DerivedDriver {
    pub fn new(
        thing: Thing,
        defs: DerivedDefsHandle,
        cache: Arc<dyn ThingCacheSource>,
        counter: Arc<dyn DescendantCounter>,
        status: DerivedStatusMap,
    ) -> Self {
        Self {
            thing,
            defs,
            cache,
            counter,
            status,
        }
    }
}

/// 从缓存解析引用值：构建 DerivedDef 时 AST 各 PropertyRef.thing_name 已被
/// 改写为 thing_id（spec §二：refs 以 id 为准——物改名不影响计算），按 id
/// 直取；名解析仅作 refs JSON 损坏行的兼容兜底。
fn resolve_ref(cache: &Arc<dyn ThingCacheSource>, r: &PropertyRef) -> Option<String> {
    let thing = cache.get(&r.thing_name).or_else(|| cache.get_by_name(&r.thing_name))?;
    thing
        .properties?
        .into_iter()
        .find(|p| p.name == r.property_name)?
        .current_value
}

/// 终审 I-2：把 AST 中各 `PropertyRef.thing_name`（表达式原文物名）改写为
/// thing_id，求值全程走 id——物改名后重算仍出值。
///
/// 对齐依据：refs JSON 由保存侧校验（derived_validation）按
/// `expression::extract_refs` 的输出顺序（去重后首现序）落库，此处对同一
/// 表达式重新 parse 得到同序列表，按序 zip 对齐（同一属性名挂在不同物上时
/// 靠 (物名, 属性名) 键区分，不会错配）。refs JSON 比 AST 引用短（损坏行）
/// 时未对齐项保留原物名，退化到名解析兜底。
pub(crate) fn rewrite_refs_to_ids(ast: &Ast, refs: &[(String, String)]) -> Ast {
    let ast_refs = expression::extract_refs(ast);
    let mut id_of: HashMap<(String, String), String> = HashMap::new();
    for (r, (tid, _)) in ast_refs.iter().zip(refs.iter()) {
        id_of.insert((r.thing_name.clone(), r.property_name.clone()), tid.clone());
    }
    if id_of.is_empty() {
        return ast.clone();
    }
    rewrite_ast(ast, &id_of)
}

fn rewrite_ast(ast: &Ast, id_of: &HashMap<(String, String), String>) -> Ast {
    match ast {
        Ast::Num(n) => Ast::Num(*n),
        Ast::Ref(r) => {
            let key = (r.thing_name.clone(), r.property_name.clone());
            match id_of.get(&key) {
                Some(id) => Ast::Ref(PropertyRef {
                    thing_name: id.clone(),
                    property_name: r.property_name.clone(),
                }),
                None => Ast::Ref(r.clone()),
            }
        }
        Ast::DescendantCount(t) => Ast::DescendantCount(t.clone()),
        Ast::Add(a, b) => Ast::Add(Box::new(rewrite_ast(a, id_of)), Box::new(rewrite_ast(b, id_of))),
        Ast::Sub(a, b) => Ast::Sub(Box::new(rewrite_ast(a, id_of)), Box::new(rewrite_ast(b, id_of))),
        Ast::Mul(a, b) => Ast::Mul(Box::new(rewrite_ast(a, id_of)), Box::new(rewrite_ast(b, id_of))),
        Ast::Div(a, b) => Ast::Div(Box::new(rewrite_ast(a, id_of)), Box::new(rewrite_ast(b, id_of))),
    }
}

/// EvalError → stale 原因文案（"等待数据" 前缀被 spec/测试契约引用，勿改）。
fn stale_reason(e: &EvalError) -> String {
    match e {
        EvalError::MissingValue(r) => format!("等待数据：引用 {}.{} 无当前值", r.thing_name, r.property_name),
        EvalError::NotNumeric(r) => format!("引用 {}.{} 当前值非数值", r.thing_name, r.property_name),
        EvalError::DivisionByZero => "表达式除零".to_string(),
    }
}

pub(crate) fn now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn collect_descendant_args(ast: &Ast, out: &mut Vec<Option<String>>) {
    match ast {
        Ast::DescendantCount(t) if !out.contains(t) => {
            out.push(t.clone());
        }
        Ast::Add(a, b) | Ast::Sub(a, b) | Ast::Mul(a, b) | Ast::Div(a, b) => {
            collect_descendant_args(a, out);
            collect_descendant_args(b, out);
        }
        _ => {}
    }
}

pub(crate) fn ast_has_descendant_count(ast: &Ast) -> bool {
    let mut args = Vec::new();
    collect_descendant_args(ast, &mut args);
    !args.is_empty()
}

/// 单条 def 求值：先异步预取 descendant_count 值（每种类型参数一次），再同步 eval。
async fn eval_def(
    def: &DerivedDef,
    cache: &Arc<dyn ThingCacheSource>,
    counter: &Arc<dyn DescendantCounter>,
) -> Result<f64, String> {
    let mut args = Vec::new();
    collect_descendant_args(&def.ast, &mut args);
    let mut counts: HashMap<Option<String>, f64> = HashMap::new();
    for arg in args {
        let v = counter.count(arg.as_deref()).await?;
        counts.insert(arg, v);
    }
    expression::eval(&def.ast, &|r: &PropertyRef| resolve_ref(cache, r), &|t: Option<
        &str,
    >| {
        counts
            .get(&t.map(str::to_string))
            .copied()
            .ok_or_else(|| "descendant_count 未预取".to_string())
    })
    .map_err(|e| stale_reason(&e))
}

/// 重算一批 defs：逐条求值、写 stale 内存态（spec §二 求值规则——任一引用
/// 无值则该 def 跳过不写值），返回成功读数。调用方负责把读数发进 sink。
///
/// 全路径无 panic（T1）：求值错误映射为 stale，不向上抛。
pub(crate) async fn recompute_defs(
    defs: &[DerivedDef],
    cache: &Arc<dyn ThingCacheSource>,
    counter: &Arc<dyn DescendantCounter>,
    status: &DerivedStatusMap,
) -> Vec<ResultValue> {
    let mut values = Vec::new();
    for def in defs {
        match eval_def(def, cache, counter).await {
            Ok(v) => {
                status.write().insert(
                    def.property_id.clone(),
                    DerivedStatus {
                        stale: false,
                        stale_reason: None,
                        last_eval_at: Some(now_string()),
                    },
                );
                values.push(ResultValue::new(
                    def.property_name.clone(),
                    "float".to_string(),
                    Some(trim_float(v)),
                ));
            }
            Err(reason) => {
                status.write().insert(
                    def.property_id.clone(),
                    DerivedStatus {
                        stale: true,
                        stale_reason: Some(reason),
                        last_eval_at: Some(now_string()),
                    },
                );
            }
        }
    }
    values
}

fn trim_float(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

impl ThingDriver for DerivedDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }

    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }

    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }

    fn start_push(&mut self, sink: ValueSink, events: Arc<dyn PropertyChangeSource>) {
        let thing_id = self.thing.id.clone();
        let defs = self.defs.clone();
        let cache = self.cache.clone();
        let counter = self.counter.clone();
        let status = self.status.clone();

        // 首值补偿（spec §三：启动即全量重算）。
        {
            let sink = sink.clone();
            let defs = defs.clone();
            let cache = cache.clone();
            let counter = counter.clone();
            let status = status.clone();
            let thing_id = thing_id.clone();
            tokio::spawn(async move {
                let snapshot = defs.read().clone();
                let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                if !values.is_empty() {
                    let _ = sink.send(DriverReading { thing_id, values }).await;
                }
            });
        }

        // 事件订阅循环。注意：subscribe 在 spawn 之前同步完成——调用方
        // （Manager.start 返回后）即可凭 subscriber_count 确认订阅已建立（E5）。
        let mut rx = events.subscribe();
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
                        let snapshot = defs.read().clone();
                        // 命中引用表才重算（refs 以 thing_id 为准）。
                        let hit = snapshot.iter().any(|d| d.refs.iter().any(|(tid, _)| *tid == src_thing));
                        if !hit {
                            continue;
                        }
                        let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                        if !values.is_empty() {
                            let _ = sink
                                .send(DriverReading {
                                    thing_id: thing_id.clone(),
                                    values,
                                })
                                .await;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        DERIVED_EVENTBUS_LAGGED.fetch_add(n, Ordering::Relaxed);
                        // 自愈（spec §三）：lag 后全量重算，输入永远是最新缓存。
                        let snapshot = defs.read().clone();
                        let values = recompute_defs(&snapshot, &cache, &counter, &status).await;
                        if !values.is_empty() {
                            let _ = sink
                                .send(DriverReading {
                                    thing_id: thing_id.clone(),
                                    values,
                                })
                                .await;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        Ok(vec![]) // 推送驱动不经轮询拉取（process_all_drivers 按 mode()=Push 跳过）
    }

    fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool, Error> {
        Err(Error::DriverError("该物不支持动作".into()))
    }
}
