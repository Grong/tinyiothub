//! Rule parser — parses rule definitions into executable AST.
//!
//! Supports simple comparison expressions: `field > value`, `field == value`, etc.

/// Supported comparison operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonOperator {
    Eq,  // ==
    Ne,  // !=
    Gt,  // >
    Gte, // >=
    Lt,  // <
    Lte, // <=
}

impl std::fmt::Display for ComparisonOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparisonOperator::Eq => write!(f, "=="),
            ComparisonOperator::Ne => write!(f, "!="),
            ComparisonOperator::Gt => write!(f, ">"),
            ComparisonOperator::Gte => write!(f, ">="),
            ComparisonOperator::Lt => write!(f, "<"),
            ComparisonOperator::Lte => write!(f, "<="),
        }
    }
}

/// Parsed rule condition.
#[derive(Debug, Clone)]
pub struct RuleCondition {
    pub field: String,
    pub operator: ComparisonOperator,
    pub value: serde_json::Value,
}

impl std::fmt::Display for RuleCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.field, self.operator, self.value)
    }
}

/// Rule parser.
#[derive(Debug, Default)]
pub struct RuleParser;

impl RuleParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a rule expression string into a condition.
    ///
    /// Supports simple comparison expressions like:
    /// - `temperature > 30`
    /// - `status == "active"`
    /// - `count >= 100`
    ///
    /// Numbers are parsed as JSON numbers, quoted strings as JSON strings,
    /// and unquoted `true`/`false`/`null` as JSON booleans/null.
    pub fn parse(&self, expression: &str) -> Result<RuleCondition, String> {
        let trimmed = expression.trim();
        if trimmed.is_empty() {
            return Err("empty expression".to_string());
        }

        // Try operators in order of longest first to avoid `>` matching before `>=`
        let operators = [
            (">=", ComparisonOperator::Gte),
            ("<=", ComparisonOperator::Lte),
            ("!=", ComparisonOperator::Ne),
            ("==", ComparisonOperator::Eq),
            (">", ComparisonOperator::Gt),
            ("<", ComparisonOperator::Lt),
        ];

        for (op_str, op) in &operators {
            if let Some(pos) = trimmed.find(op_str) {
                let field = trimmed[..pos].trim();
                let value_str = trimmed[pos + op_str.len()..].trim();

                if field.is_empty() {
                    return Err(format!("missing field before operator '{}'", op_str));
                }
                if value_str.is_empty() {
                    return Err(format!("missing value after operator '{}'", op_str));
                }

                let value = Self::parse_value(value_str)?;

                return Ok(RuleCondition {
                    field: field.to_string(),
                    operator: op.clone(),
                    value,
                });
            }
        }

        Err(format!("unsupported expression format: '{}'", expression))
    }

    fn parse_value(s: &str) -> Result<serde_json::Value, String> {
        // Try JSON literal first
        if s == "true" {
            return Ok(serde_json::Value::Bool(true));
        }
        if s == "false" {
            return Ok(serde_json::Value::Bool(false));
        }
        if s == "null" {
            return Ok(serde_json::Value::Null);
        }

        // Try quoted string
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            let unquoted = &s[1..s.len() - 1];
            return Ok(serde_json::Value::String(unquoted.to_string()));
        }

        // Try number
        if let Ok(n) = s.parse::<i64>() {
            return Ok(serde_json::Value::Number(serde_json::Number::from(n)));
        }
        if let Ok(n) = s.parse::<f64>()
            && let Some(num) = serde_json::Number::from_f64(n)
        {
            return Ok(serde_json::Value::Number(num));
        }

        // Fallback: treat as string
        Ok(serde_json::Value::String(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gt_number() {
        let parser = RuleParser::new();
        let cond = parser.parse("temperature > 30").unwrap();
        assert_eq!(cond.field, "temperature");
        assert_eq!(cond.operator, ComparisonOperator::Gt);
        assert_eq!(cond.value, serde_json::json!(30));
    }

    #[test]
    fn test_parse_eq_string() {
        let parser = RuleParser::new();
        let cond = parser.parse("status == \"active\"").unwrap();
        assert_eq!(cond.field, "status");
        assert_eq!(cond.operator, ComparisonOperator::Eq);
        assert_eq!(cond.value, serde_json::json!("active"));
    }

    #[test]
    fn test_parse_gte_float() {
        let parser = RuleParser::new();
        let cond = parser.parse("count >= 100.5").unwrap();
        assert_eq!(cond.operator, ComparisonOperator::Gte);
        assert_eq!(cond.value, serde_json::json!(100.5));
    }

    #[test]
    fn test_parse_empty_fails() {
        let parser = RuleParser::new();
        assert!(parser.parse("").is_err());
    }

    #[test]
    fn test_parse_unsupported_fails() {
        let parser = RuleParser::new();
        assert!(parser.parse("temperature").is_err());
    }
}
