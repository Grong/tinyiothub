//! E2 composite 顺序执行器（spec 2026-10-21-e2 §6.3）。
//!
//! 语义：逐步执行；每步仲裁互斥 + 账本 job 事件；失败停止并按已完成步骤
//! 逆序回滚（有 rollback 声明的）。部分成功必须显式（partial/rolled_back），
//! 绝不静默报成功。

use std::sync::Arc;

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::runtime::thing_agent::seams::composite::{CompositeDef, CompositeOp};
use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL};
use tinyiothub_core::models::event::{ContentElement, Event, EventSource, RichContent, TextFormat};
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver, device_claim};
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::HubLedger;

use super::tools::autonomous_invoke::dispatch_command_with_lease;
use super::tools::thing::resource_specs;
use crate::shared::runtime_ports::HubLedgerAdapter;

/// 每步租约 TTL（覆盖单次动作窗口）。
const STEP_LEASE_TTL: std::time::Duration = DEFAULT_LEASE_TTL;

pub struct CompositeExecutor {
    pub pool: SqlitePool,
    pub workspace_id: String,
    pub data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub event_bus: Option<Arc<EventBus>>,
    pub arbitration: Option<Arc<ArbitrationRegistry>>,
    pub holder: String,
    /// E4c 声明资源解析：每步按该步工具声明 claim。None = 未接线，沿用
    /// E1 每设备合成资源 claim。
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
        let holder = format!("composite:{}:{}", def.name, self.holder);

        // E4c：每步按该步工具的声明资源 claim（未声明/无 action → 合成
        // `__device__`），all-or-nothing；busy 文案带持有者 + 资源 id。
        // Call 步把租约移交给 worker（claim 覆盖真实执行窗口——评审 Issue 4）；
        // Set 步在步骤结束时随作用域释放。
        let mut lease = match &self.arbitration {
            Some(registry) => {
                let acquired = match &self.resources {
                    Some(resolver) => match self.resolve_claims(resolver.as_ref(), &thing_id, op).await {
                        Ok(claims) => {
                            registry.try_acquire_resources(&thing_id, holder, &resource_specs(&claims), STEP_LEASE_TTL)
                        }
                        Err(e) => return Err(format!("资源解析失败: {e}")),
                    },
                    None => registry.try_acquire(&thing_id, holder, STEP_LEASE_TTL),
                };
                match acquired {
                    Ok(lease) => Some(lease),
                    Err(busy) if self.resources.is_some() => return Err(busy.to_string()),
                    Err(busy) => return Err(format!("busy: 设备正被 {} 占用", busy.holder)),
                }
            }
            None => None,
        };

        self.ledger(def, index, &thing_id, "accepted", None).await;

        let result = match op {
            CompositeOp::Call { action, params, .. } => {
                // 本步租约已持有（上面 claim）——直接移交，避免嵌套 acquire。
                let dispatched = dispatch_command_with_lease(
                    self.data_server.as_ref(),
                    &thing_id,
                    action,
                    Some(params),
                    lease.take(),
                );
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
}
