//! Rule evaluator — evaluates conditions against telemetry data.

use crate::rule::parser::{ComparisonOperator, RuleCondition};

/// Evaluates rule conditions against device telemetry.
#[derive(Debug, Default)]
pub struct RuleEvaluator;

impl RuleEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate a condition against a data point.
    ///
    /// Extracts the field from the data object and compares it against
    /// the condition value using the specified operator.
    pub fn evaluate(
        &self,
        condition: &RuleCondition,
        data: &serde_json::Value,
    ) -> Result<bool, String> {
        let data_value = match data.get(&condition.field) {
            Some(v) => v,
            None => return Ok(false), // field not present = condition not met
        };

        let result = match &condition.operator {
            ComparisonOperator::Eq => Self::json_eq(data_value, &condition.value),
            ComparisonOperator::Ne => !Self::json_eq(data_value, &condition.value),
            ComparisonOperator::Gt => Self::json_gt(data_value, &condition.value)?,
            ComparisonOperator::Gte => {
                Self::json_gt(data_value, &condition.value)?
                    || Self::json_eq(data_value, &condition.value)
            }
            ComparisonOperator::Lt => Self::json_lt(data_value, &condition.value)?,
            ComparisonOperator::Lte => {
                Self::json_lt(data_value, &condition.value)?
                    || Self::json_eq(data_value, &condition.value)
            }
        };

        Ok(result)
    }

    fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
        // Coerce numbers: compare f64 representation
        if let (Some(a_num), Some(b_num)) = (Self::as_f64(a), Self::as_f64(b)) {
            return (a_num - b_num).abs() < f64::EPSILON;
        }
        a == b
    }

    fn json_gt(a: &serde_json::Value, b: &serde_json::Value) -> Result<bool, String> {
        match (Self::as_f64(a), Self::as_f64(b)) {
            (Some(a_num), Some(b_num)) => Ok(a_num > b_num),
            _ => Err(format!(
                "cannot compare non-numeric values: {} > {}",
                a, b
            )),
        }
    }

    fn json_lt(a: &serde_json::Value, b: &serde_json::Value) -> Result<bool, String> {
        match (Self::as_f64(a), Self::as_f64(b)) {
            (Some(a_num), Some(b_num)) => Ok(a_num < b_num),
            _ => Err(format!(
                "cannot compare non-numeric values: {} < {}",
                a, b
            )),
        }
    }

    fn as_f64(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::parser::{ComparisonOperator, RuleCondition};

    fn make_condition(field: &str, op: ComparisonOperator, value: serde_json::Value) -> RuleCondition {
        RuleCondition {
            field: field.to_string(),
            operator: op,
            value,
        }
    }

    #[test]
    fn test_evaluate_eq_number() {
        let evaluator = RuleEvaluator::new();
        let cond = make_condition("temp", ComparisonOperator::Eq, serde_json::json!(25));
        let data = serde_json::json!({"temp": 25});
        assert!(evaluator.evaluate(&cond, &data).unwrap());
    }

    #[test]
    fn test_evaluate_gt_number() {
        let evaluator = RuleEvaluator::new();
        let cond = make_condition("temp", ComparisonOperator::Gt, serde_json::json!(20));
        let data = serde_json::json!({"temp": 25});
        assert!(evaluator.evaluate(&cond, &data).unwrap());
    }

    #[test]
    fn test_evaluate_missing_field() {
        let evaluator = RuleEvaluator::new();
        let cond = make_condition("missing", ComparisonOperator::Eq, serde_json::json!(1));
        let data = serde_json::json!({"other": 1});
        assert!(!evaluator.evaluate(&cond, &data).unwrap());
    }

    #[test]
    fn test_evaluate_string_eq() {
        let evaluator = RuleEvaluator::new();
        let cond = make_condition("status", ComparisonOperator::Eq, serde_json::json!("active"));
        let data = serde_json::json!({"status": "active"});
        assert!(evaluator.evaluate(&cond, &data).unwrap());
    }

    #[test]
    fn test_evaluate_non_numeric_comparison_fails() {
        let evaluator = RuleEvaluator::new();
        let cond = make_condition("name", ComparisonOperator::Gt, serde_json::json!("a"));
        let data = serde_json::json!({"name": "b"});
        assert!(evaluator.evaluate(&cond, &data).is_err());
    }
}
