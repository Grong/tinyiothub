//! Rule evaluator — evaluates conditions against telemetry data.
//!
//! TODO: Migrate logic from `cloud/src/domain/alarm/services/rule_engine.rs`.

use crate::rule::parser::RuleCondition;

/// Evaluates rule conditions against device telemetry.
#[derive(Debug, Default)]
pub struct RuleEvaluator;

impl RuleEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate a condition against a data point.
    pub fn evaluate(
        &self,
        _condition: &RuleCondition,
        _data: &serde_json::Value,
    ) -> Result<bool, String> {
        // TODO: implement actual evaluation
        Err("not yet implemented".into())
    }
}
