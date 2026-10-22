//! E2 composite 顺序执行器（spec 2026-10-21-e2 §6.3）。
//!
//! 语义：逐步执行；每步仲裁互斥 + 账本 job 事件；失败停止并按已完成步骤
//! 逆序回滚（有 rollback 声明的）。部分成功必须显式（partial/rolled_back），
//! 绝不静默报成功。

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::runtime::thing_agent::seams::composite::{CompositeDef, CompositeOp};
use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL, Lease};
use tinyiothub_core::models::event::{ContentElement, Event, EventSource, RichContent, TextFormat};
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::HubLedger;

use super::tools::autonomous_invoke::dispatch_command_with_lease;
use super::tools::thing::resource_specs;
use crate::shared::runtime_ports::HubLedgerAdapter;

/// composite 全程租约 TTL：一次 claim 覆盖整条链（含回滚）的执行窗口。
const COMPOSITE_LEASE_TTL: std::time::Duration = DEFAULT_LEASE_TTL;

pub struct CompositeExecutor {
    pub pool: SqlitePool,
    pub workspace_id: String,
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub event_bus: Option<Arc<EventBus>>,
    pub arbitration: Option<Arc<ArbitrationRegistry>>,
    pub holder: String,
    /// E4c 声明资源解析：composite 开始时解析全部步骤（含回滚）的声明并
    /// 按并集 claim 一次。None = 未接线，沿用 E1 每设备合成资源 claim。
    pub resources: Option<Arc<dyn ResourceResolver>>,
}

struct StepResult {
    index: usize,
    status: &'static str,
    output: Option<Value>,
    error: Option<String>,
}

impl CompositeExecutor {
    pub async fn execute(&self, def: &CompositeDef) -> Value {
        let holder = format!("composite:{}:{}", def.name, self.holder);

        // E4c：整条 composite 只 claim 一次——并集覆盖所有步骤（含回滚）声明
        // 的资源，并持有到本函数返回（成功/失败/回滚全部完成）。步骤之间不
        // 释放、也不把每步租约移交给 worker：worker 是异步执行的，逐步骤
        // 移交会让下一步在上一命令仍在执行时 self-conflict（同一 holder）。
        // `_leases` 在作用域结束时 RAII 释放；claim 失败时已获租约同样在
        // `claim_run` 内释放（all-or-nothing）。
        let _leases = match self.claim_run(def, &holder).await {
            Ok(leases) => leases,
            Err(e) => {
                return json!({
                    "v": 1,
                    "compositeId": def.id,
                    "status": "failed",
                    "steps": [step_json(&StepResult {
                        index: 0,
                        status: "failed",
                        output: None,
                        error: Some(e),
                    })],
                    "message": "资源 claim 失败，未执行任何步骤",
                });
            }
        };

        let mut results: Vec<Value> = Vec::new();
        let mut completed: Vec<usize> = Vec::new();
        let mut failed = false;

        for (i, step) in def.steps.iter().enumerate() {
            match self.run_op(def, i, &step.op).await {
                Ok(out) => {
                    completed.push(i);
                    results.push(step_json(&StepResult {
                        index: i,
                        status: "ok",
                        output: Some(out),
                        error: None,
                    }));
                }
                Err(e) => {
                    results.push(step_json(&StepResult {
                        index: i,
                        status: "failed",
                        output: None,
                        error: Some(e),
                    }));
                    failed = true;
                    break;
                }
            }
        }

        if !failed {
            return json!({
                "v": 1,
                "compositeId": def.id,
                "status": "completed",
                "steps": results,
            });
        }

        // 回滚：已完成步骤逆序，仅执行声明了 rollback 的。
        let mut rollback_failed = false;
        for i in completed.iter().rev() {
            let Some(rollback) = &def.steps[*i].rollback else {
                continue;
            };
            let (status, error) = match self.run_op(def, *i, rollback).await {
                Ok(_) => ("rolled_back", None),
                Err(e) => {
                    rollback_failed = true;
                    ("rollback_failed", Some(e))
                }
            };
            results.push(json!({
                "index": i,
                "status": status,
                "error": error,
            }));
        }

        let status = if completed.is_empty() {
            "failed"
        } else if rollback_failed {
            "partial"
        } else {
            "rolled_back"
        };
        let message = match status {
            "failed" => "首步即失败，无部分成功",
            "partial" => "部分步骤已执行且回滚未完成/未声明——需人工核对",
            _ => "部分步骤已执行，回滚完成",
        };
        json!({
            "v": 1,
            "compositeId": def.id,
            "status": status,
            "steps": results,
            "message": message,
        })
    }

    async fn run_op(&self, def: &CompositeDef, index: usize, op: &CompositeOp) -> Result<Value, String> {
        let thing_id = op.thing_id().to_string();

        // stop 闸门：全程租约已在 run 开始时取得，`set_stop` 会强制释放它，
        // 因此这里按步骤复检——stop 之后不再下发剩余命令（与单工具路径的
        // `stopped: <reason>` 文案一致）。
        if let Some(registry) = &self.arbitration
            && let Some(reason) = registry.stop_reason(&thing_id)
        {
            return Err(format!("stopped: {reason}"));
        }

        self.ledger(def, index, &thing_id, "accepted", None).await;

        let result = match op {
            CompositeOp::Call { action, params, .. } => {
                // composite 租约由 `execute` 全程持有；worker 不再接收每步
                // 租约（否则上一步的租约会在后续步骤期间仍占用资源）。
                let dispatched =
                    dispatch_command_with_lease(self.data_server.as_ref(), &thing_id, action, Some(params), None);
                if dispatched.success {
                    Ok(serde_json::from_str(&dispatched.output)
                        .unwrap_or_else(|_| json!({ "output": dispatched.output })))
                } else {
                    Err(dispatched.error.unwrap_or(dispatched.output))
                }
            }
            CompositeOp::Set { properties, .. } => self.write_properties(&thing_id, properties).await,
        };

        match &result {
            Ok(v) => self.ledger(def, index, &thing_id, "result", Some(v.clone())).await,
            Err(e) => {
                self.ledger(def, index, &thing_id, "rejected", Some(json!({ "error": e })))
                    .await
            }
        }
        result
    }

    /// 整条 composite 的声明资源并集：按 thing 分组（`BTreeMap` 保证按
    /// thing id 排序 acquire），每台物只 claim 一次，租约覆盖全部步骤
    /// （含回滚）。任一 thing busy → 已获租约全部释放并返回错误。
    ///
    /// resolver 未接线（None）时沿用 E1 每设备互斥：每台不同的物各
    /// `try_acquire` 一次并全部持有。
    async fn claim_run(&self, def: &CompositeDef, holder: &str) -> Result<Vec<Lease>, String> {
        let Some(registry) = &self.arbitration else {
            return Ok(Vec::new());
        };

        // 先解析全部声明，再统一 claim——解析失败不留下部分租约。
        let mut by_thing: BTreeMap<String, Vec<ResourceClaim>> = BTreeMap::new();
        for step in &def.steps {
            for op in std::iter::once(&step.op).chain(step.rollback.iter()) {
                let thing_id = op.thing_id().to_string();
                let claims = match &self.resources {
                    Some(resolver) => self.resolve_claims(resolver.as_ref(), &thing_id, op).await?,
                    None => Vec::new(),
                };
                by_thing.entry(thing_id).or_default().extend(claims);
            }
        }

        let mut leases: Vec<Lease> = Vec::with_capacity(by_thing.len());
        for (thing_id, mut claims) in by_thing {
            let acquired = match &self.resources {
                Some(_) => {
                    merge_claims(&mut claims);
                    registry.try_acquire_resources(
                        &thing_id,
                        holder.to_string(),
                        &resource_specs(&claims),
                        COMPOSITE_LEASE_TTL,
                    )
                }
                None => registry.try_acquire(&thing_id, holder.to_string(), COMPOSITE_LEASE_TTL),
            };
            match acquired {
                Ok(lease) => leases.push(lease),
                Err(busy) if self.resources.is_some() => return Err(busy.to_string()),
                Err(busy) => return Err(format!("busy: 设备正被 {} 占用", busy.holder)),
            }
        }
        Ok(leases)
    }

    /// 解析一步的资源声明。空声明（或无法解析出资源）回落 E1 合成
    /// `__device__` exclusive，保证未声明资源的变更步骤仍每设备互斥。
    async fn resolve_claims(
        &self,
        resolver: &dyn ResourceResolver,
        thing_id: &str,
        op: &CompositeOp,
    ) -> Result<Vec<ResourceClaim>, String> {
        let claims = match op {
            CompositeOp::Call { action, .. } if !action.trim().is_empty() => {
                resolver.resolve_action(&self.workspace_id, thing_id, action).await?
            }
            CompositeOp::Set { properties, .. } => {
                let names: Vec<String> = properties
                    .as_object()
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                resolver
                    .resolve_properties(&self.workspace_id, thing_id, &names)
                    .await?
            }
            // 无 action 的步骤：无声明可解，按合成设备资源处理。
            _ => return Ok(vec![device_claim()]),
        };
        Ok(if claims.is_empty() {
            vec![device_claim()]
        } else {
            claims
        })
    }

    async fn write_properties(&self, thing_id: &str, properties: &Value) -> Result<Value, String> {
        let Some(event_bus) = &self.event_bus else {
            return Err("event bus unavailable: property write NOT executed".to_string());
        };
        let Some(properties) = properties.as_object() else {
            return Err("properties 必须是对象".to_string());
        };
        let db = tinyiothub_storage::Db::new(self.pool.clone());
        let thing = db
            .find_thing_by_id(Some(&self.workspace_id), thing_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("物不存在：{thing_id}"))?;
        let props = db
            .find_thing_properties_by_thing_id(thing_id)
            .await
            .map_err(|e| e.to_string())?;
        let mut updated = Vec::new();
        for (name, value) in properties {
            let prop = props
                .iter()
                .find(|p| &p.name == name)
                .ok_or_else(|| format!("属性不存在: {name}"))?;
            if prop.is_read_only != 0 {
                return Err(format!("只读属性不支持写入: {name}"));
            }
            if prop.source.as_deref() == Some("derived") {
                return Err(format!("派生属性不支持手动写值: {name}"));
            }
            let text = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return Err(format!("属性值必须是标量: {name}")),
            };
            let source =
                EventSource::device_property(thing_id.to_string(), prop.id.clone(), format!("{thing_id}:{}", prop.id));
            let content = RichContent::new(
                format!(
                    "Property Changed: {} - {}",
                    thing.display_name.as_deref().unwrap_or(&thing.name),
                    prop.name
                ),
                vec![ContentElement::Text {
                    content: format!("Current value: {text}"),
                    format: TextFormat::Plain,
                }],
            );
            let event = Event::new_property_change_event(thing_id.to_string(), prop.id.clone(), source, content)
                .map_err(|e| e.to_string())?;
            event_bus.publish(event).await.map_err(|e| e.to_string())?;
            updated.push(name.clone());
        }
        Ok(json!({ "updated": updated }))
    }

    async fn ledger(&self, def: &CompositeDef, index: usize, thing_id: &str, status: &str, data: Option<Value>) {
        let ledger = HubLedgerAdapter::new(tinyiothub_storage::Db::new(self.pool.clone()));
        let payload = json!({
            "v": 1,
            "compositeId": def.id,
            "compositeName": def.name,
            "stepIndex": index,
            "status": status,
            "data": data,
        });
        if let Err(e) = HubLedger::append(&ledger, &self.workspace_id, "job", Some(thing_id), payload).await {
            // 账本可补，执行不能因账本故障中断——但必须在日志里可见。
            tracing::warn!(workspace_id = %self.workspace_id, composite_id = %def.id, error = %e, "composite ledger append failed");
        }
    }
}

/// 合并同一台物上跨步骤重复出现的资源声明：同一 id 只保留一条——
/// 任一声明 exclusive 即按 exclusive（fail-closed）；均为 shared 时
/// capacity 取更严格的最小值。
fn merge_claims(claims: &mut Vec<ResourceClaim>) {
    claims.sort_by(|a, b| a.id.cmp(&b.id));
    claims.dedup_by(|later, kept| {
        if later.id != kept.id {
            return false;
        }
        kept.exclusive |= later.exclusive;
        kept.capacity = if kept.exclusive {
            1
        } else {
            kept.capacity.min(later.capacity)
        };
        true
    });
}

fn step_json(s: &StepResult) -> Value {
    let mut v = json!({ "index": s.index, "status": s.status });
    if let Some(out) = &s.output {
        v["output"] = out.clone();
    }
    if let Some(err) = &s.error {
        v["error"] = json!(err);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_json_shape() {
        let v = step_json(&StepResult {
            index: 2,
            status: "failed",
            output: None,
            error: Some("boom".into()),
        });
        assert_eq!(v["index"], 2);
        assert_eq!(v["status"], "failed");
        assert_eq!(v["error"], "boom");
        assert!(v.get("output").is_none());
    }

    #[test]
    fn merge_claims_exclusive_wins_and_shared_takes_min_capacity() {
        let mut claims = vec![
            ResourceClaim {
                id: "stream".into(),
                exclusive: false,
                capacity: 4,
            },
            ResourceClaim {
                id: "actuator".into(),
                exclusive: false,
                capacity: 3,
            },
            ResourceClaim {
                id: "actuator".into(),
                exclusive: false,
                capacity: 2,
            },
            ResourceClaim {
                id: "stream".into(),
                exclusive: true,
                capacity: 1,
            },
        ];
        merge_claims(&mut claims);
        assert_eq!(
            claims,
            vec![
                ResourceClaim {
                    id: "actuator".into(),
                    exclusive: false,
                    capacity: 2,
                },
                ResourceClaim {
                    id: "stream".into(),
                    exclusive: true,
                    capacity: 1,
                },
            ]
        );
    }
}
