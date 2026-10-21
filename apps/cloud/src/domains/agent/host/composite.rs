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
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::HubLedger;

use super::tools::autonomous_invoke::dispatch_command;
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
        let _lease = match &self.arbitration {
            Some(registry) => Some(
                registry
                    .try_acquire(
                        &thing_id,
                        format!("composite:{}:{}", def.name, self.holder),
                        STEP_LEASE_TTL,
                    )
                    .map_err(|busy| format!("busy: 设备正被 {} 占用", busy.holder))?,
            ),
            None => None,
        };

        self.ledger(def, index, &thing_id, "accepted", None).await;

        let result = match op {
            CompositeOp::Call { action, params, .. } => {
                let dispatched = dispatch_command(
                    self.data_server.as_ref(),
                    &thing_id,
                    action,
                    Some(params),
                    None, // 已在上面持有租约，避免嵌套 acquire
                    &format!("composite:{}", def.name),
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
