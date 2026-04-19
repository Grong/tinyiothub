//! Rule parser — parses rule definitions into executable AST.
//!
//! TODO: Migrate logic from `cloud/src/domain/alarm/services/rule_engine.rs`.

/// Parsed rule condition.
#[derive(Debug, Clone)]
pub struct RuleCondition {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

/// Rule parser.
#[derive(Debug, Default)]
pub struct RuleParser;

impl RuleParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a rule expression string into a condition tree.
    pub fn parse(&self, _expression: &str) -> Result<RuleCondition, String> {
        // TODO: implement actual parsing
        Err("not yet implemented".into())
    }
}
