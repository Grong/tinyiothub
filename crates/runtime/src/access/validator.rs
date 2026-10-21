//! Call-boundary validation against a device declaration (E4a Task 11).
//!
//! Runtime-side contracts only: the cloud composition root supplies a
//! validator (see [`super::service::AccessService`]) and the tool layer
//! fails closed when one is wired.

use async_trait::async_trait;
use tinyiothub_core::driver::declaration::DeviceDeclaration;
use tinyiothub_core::driver::manifest::ManifestTool;
use tinyiothub_core::driver::schema::validate_params;

/// A call that passed declaration validation.
#[derive(Debug, Clone)]
pub struct ValidatedCall {
    pub tool: ManifestTool,
    pub timeout_ms: u64,
    pub clamped: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("no declaration for thing '{0}'")]
    NoDeclaration(String),
    #[error("action '{0}' not declared")]
    UndeclaredTool(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("declaration unavailable: {0}")]
    Unavailable(String),
}

#[async_trait]
pub trait AccessCallValidator: Send + Sync {
    async fn validate(
        &self,
        workspace_id: &str,
        thing_id: &str,
        tool: &str,
        params: Option<&serde_json::Value>,
    ) -> Result<ValidatedCall, CallError>;
}

/// Validate a call against a declaration: the tool must be declared, the
/// parameters must satisfy its JSON schema, and the timeout defaults to /
/// clamps at the declared maximum.
pub fn validate_call(
    decl: &DeviceDeclaration,
    tool: &str,
    params: Option<&serde_json::Value>,
    requested_timeout_ms: Option<u64>,
) -> Result<ValidatedCall, CallError> {
    let t = decl
        .tools
        .iter()
        .find(|t| t.name == tool)
        .ok_or_else(|| CallError::UndeclaredTool(tool.to_string()))?
        .clone();
    let params = params.cloned().unwrap_or_else(|| serde_json::json!({}));
    validate_params(&t.parameters, &params).map_err(CallError::InvalidParams)?;
    let max = t.timeout.max_ms.max(1);
    let requested = requested_timeout_ms.unwrap_or(t.timeout.default_ms).max(1);
    let timeout_ms = requested.min(max);
    Ok(ValidatedCall {
        tool: t,
        timeout_ms,
        clamped: timeout_ms != requested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_core::driver::manifest::{ToolKindDecl, ToolSafetyDecl, ToolTimeout};

    fn declaration() -> DeviceDeclaration {
        DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![],
            tools: vec![ManifestTool {
                name: "set_level".into(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "level": {"type": "integer", "minimum": 0, "maximum": 100}
                    },
                    "required": ["level"],
                    "additionalProperties": false
                }),
                timeout: ToolTimeout {
                    default_ms: 3000,
                    max_ms: 5000,
                },
                safety: ToolSafetyDecl::Control,
                resources: vec![],
                cancellable: true,
            }],
        }
    }

    #[test]
    fn undeclared_tool_is_rejected() {
        let err = validate_call(&declaration(), "reboot", None, None).unwrap_err();
        assert!(
            matches!(err, CallError::UndeclaredTool(ref t) if t == "reboot"),
            "{err:?}"
        );
    }

    #[test]
    fn out_of_range_param_is_rejected() {
        let err = validate_call(
            &declaration(),
            "set_level",
            Some(&serde_json::json!({"level": 200})),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(err, CallError::InvalidParams(ref m) if m.contains("maximum")),
            "{err:?}"
        );
    }

    #[test]
    fn timeout_defaults_to_declared_and_clamps_to_max() {
        let by_default = validate_call(
            &declaration(),
            "set_level",
            Some(&serde_json::json!({"level": 10})),
            None,
        )
        .expect("declared call");
        assert_eq!(by_default.timeout_ms, 3000);
        assert!(!by_default.clamped);

        let clamped = validate_call(
            &declaration(),
            "set_level",
            Some(&serde_json::json!({"level": 10})),
            Some(9000),
        )
        .expect("declared call");
        assert_eq!(clamped.timeout_ms, 5000);
        assert!(clamped.clamped);
    }
}
