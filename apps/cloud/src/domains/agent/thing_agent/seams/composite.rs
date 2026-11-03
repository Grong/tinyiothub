//! E2 composite 编排原语（spec 2026-10-21-e2 §6）：声明类型 + 校验。
//!
//! 纯类型、零存储：声明存 `agent_composites`（cloud 侧），执行器在组合层。
//! 只支持顺序链（1..=10 步）；跨工作区引用一律拒绝（执行器校验物存在性）。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 契约版本。
pub const COMPOSITE_V: u8 = 1;
/// 顺序链步数上限。
pub const MAX_COMPOSITE_STEPS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeDef {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub v: u8,
    pub steps: Vec<CompositeStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeStep {
    /// 展平：`{"op":"call","thing_id":...}`。
    #[serde(flatten)]
    pub op: CompositeOp,
    /// 回滚动作：链失败时对已完成步骤逆序执行。
    #[serde(default)]
    pub rollback: Option<CompositeOp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CompositeOp {
    /// 设备动作（call_device 语义）。
    Call {
        thing_id: String,
        action: String,
        #[serde(default)]
        params: Value,
    },
    /// 属性写入（set_device 语义）。
    Set { thing_id: String, properties: Value },
}

impl CompositeOp {
    pub fn thing_id(&self) -> &str {
        match self {
            Self::Call { thing_id, .. } | Self::Set { thing_id, .. } => thing_id,
        }
    }

    /// 结构性校验（引用存在性由组合层按工作区查库）。
    fn validate(&self, step: usize) -> Result<(), String> {
        if self.thing_id().trim().is_empty() {
            return Err(format!("step {step}: thing_id 不能为空"));
        }
        match self {
            Self::Call { action, .. } if action.trim().is_empty() => Err(format!("step {step}: action 不能为空")),
            Self::Set { properties, .. } if !properties.is_object() => {
                Err(format!("step {step}: properties 必须是对象"))
            }
            _ => Ok(()),
        }
    }
}

/// 声明校验：非空、1..=10 步、版本=1、每步字段完整。
pub fn validate_composite(def: &CompositeDef) -> Result<(), String> {
    if def.id.trim().is_empty() {
        return Err("composite id 不能为空".to_string());
    }
    if def.v != COMPOSITE_V {
        return Err(format!("不支持的 composite 版本 {}（支持 {COMPOSITE_V}）", def.v));
    }
    if def.name.trim().is_empty() {
        return Err("composite name 不能为空".to_string());
    }
    if def.steps.is_empty() {
        return Err("composite 至少需要 1 步".to_string());
    }
    if def.steps.len() > MAX_COMPOSITE_STEPS {
        return Err(format!("composite 步数上限 {MAX_COMPOSITE_STEPS}"));
    }
    for (i, step) in def.steps.iter().enumerate() {
        step.op.validate(i)?;
        if let Some(rollback) = &step.rollback {
            rollback.validate(i)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn def(steps: Vec<CompositeStep>) -> CompositeDef {
        CompositeDef {
            id: "c1".into(),
            workspace_id: "ws".into(),
            name: "storm_guard".into(),
            description: "".into(),
            v: 1,
            steps,
        }
    }

    fn call(thing: &str, action: &str) -> CompositeStep {
        CompositeStep {
            op: CompositeOp::Call {
                thing_id: thing.into(),
                action: action.into(),
                params: json!({}),
            },
            rollback: None,
        }
    }

    #[test]
    fn valid_chain_roundtrips_json() {
        let d = def(vec![call("t1", "close"), call("t2", "retract")]);
        validate_composite(&d).unwrap();
        let json = serde_json::to_string(&d).unwrap();
        let back: CompositeDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
        assert!(json.contains("\"op\":\"call\""));
    }

    #[test]
    fn rejects_empty_and_oversized_chains() {
        assert!(validate_composite(&def(vec![])).is_err());
        let steps: Vec<CompositeStep> = (0..11).map(|i| call(&format!("t{i}"), "a")).collect();
        assert!(validate_composite(&def(steps)).is_err());
    }

    #[test]
    fn rejects_unknown_version_and_blank_fields() {
        let mut d = def(vec![call("t1", "a")]);
        d.v = 2;
        assert!(validate_composite(&d).is_err());
        assert!(validate_composite(&def(vec![call("", "a")])).is_err());
        assert!(validate_composite(&def(vec![call("t1", "")])).is_err());
    }

    #[test]
    fn rollback_op_is_validated_too() {
        let mut step = call("t1", "close");
        step.rollback = Some(CompositeOp::Call {
            thing_id: "t1".into(),
            action: "".into(),
            params: json!({}),
        });
        assert!(validate_composite(&def(vec![step])).is_err());
    }
}
