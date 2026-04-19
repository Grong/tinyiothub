//! Rule action — defines what happens when a rule fires.
//!
//! TODO: Migrate logic from `cloud/src/domain/alarm/services/rule_engine.rs`.

/// Action to execute when a rule triggers.
#[derive(Debug, Clone)]
pub enum RuleAction {
    SendNotification { channel: String, message: String },
    UpdateProperty { device_id: String, property: String, value: serde_json::Value },
    ExecuteCommand { device_id: String, command: String },
    Webhook { url: String, payload: serde_json::Value },
}
