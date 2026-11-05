//! Tool Registry — centralized tool discovery and metadata.
//!
//! [`ToolDescriptor`] is the single declaration source for what a tool is
//! allowed to do and how it is exposed: the LLM tool subset, the catalog API,
//! the approval/policy gate, redacted argument summaries, and generic UI
//! metadata all derive from the same descriptor. Adding a tool must not require
//! editing core or a frontend event allowlist.
//!
//! The registry itself is pure logic (no I/O); composition layers register
//! descriptors and adapt runtime tools to them.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::trust::{ToolSafety, classify_tool_safety};

/// Schema for structured LLM output enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchema {
    /// JSON Schema describing the expected output shape.
    pub json_schema: serde_json::Value,
    /// Whether the LLM should be forced into JSON mode for this tool.
    pub enforce_json_mode: bool,
}

/// How a tool affects the outside world. Drives default approval and display
/// treatment; a declared value always wins over name heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffectKind {
    /// Observation only; no durable write.
    Read,
    /// Durable write inside the platform.
    Write,
    /// Irreversible or safety-relevant device action.
    Destructive,
    /// Delegated external side effect (MCP, device driver, remote service).
    External,
}

impl ToolEffectKind {
    /// Default effect kind inferred from the trust classification.
    pub fn from_safety(safety: ToolSafety) -> Self {
        match safety {
            ToolSafety::ReadOnly => Self::Read,
            ToolSafety::Write => Self::Write,
            ToolSafety::Destructive => Self::Destructive,
        }
    }
}

/// Capability a tool needs before it may be exposed to the model. The
/// composition root resolves the actually available capabilities from the
/// provider/model configuration and capability ports (for example
/// `supports_vision`); it never guesses in the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Vision,
    Video,
    Audio,
    Network,
    Text,
}

impl Capability {
    pub fn all() -> Vec<Self> {
        vec![Self::Vision, Self::Video, Self::Audio, Self::Network, Self::Text]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vision => "vision",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Network => "network",
            Self::Text => "text",
        }
    }

    /// Whether `available` covers this capability. Video implies vision: a
    /// video source is unusable on a provider that cannot see images, so a
    /// non-vision provider must never be allowed to invent observations.
    pub fn is_satisfied_by(self, available: &[Self]) -> bool {
        match self {
            Self::Video => available.contains(&Self::Video) && available.contains(&Self::Vision),
            other => available.contains(&other),
        }
    }
}

/// Approval requirement declared by a tool, optionally tightened by a
/// Workflow policy. Tightening is the only permitted direction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPolicy {
    pub mandatory: bool,
    pub timeout_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self::none()
    }
}

impl ApprovalPolicy {
    /// No approval requirement.
    pub fn none() -> Self {
        Self {
            mandatory: false,
            timeout_secs: 60,
            reason: None,
        }
    }

    /// Mandatory approval with a reason surfaced to the operator.
    pub fn mandatory(reason: impl Into<String>) -> Self {
        Self {
            mandatory: true,
            timeout_secs: 60,
            reason: Some(reason.into()),
        }
    }

    /// Compose the tool's mandatory policy with an optional Workflow policy.
    ///
    /// A Workflow may add approval or extend the response window; it can never
    /// remove mandatory approval or shorten the window below the tool's
    /// declared requirement.
    pub fn tighten(&self, workflow: Option<&Self>) -> Self {
        let Some(workflow) = workflow else {
            return self.clone();
        };
        let mandatory = self.mandatory || workflow.mandatory;
        Self {
            mandatory,
            timeout_secs: if mandatory {
                self.timeout_secs.max(workflow.timeout_secs)
            } else {
                self.timeout_secs
            },
            reason: if self.mandatory {
                self.reason.clone().or_else(|| workflow.reason.clone())
            } else {
                workflow.reason.clone()
            },
        }
    }

    /// Parse the Workflow/persisted JSON shape. Shapes without at least one
    /// recognized policy key yield `None` so persistence compatibility does not
    /// invent a policy (an empty object must not silently mean "no approval").
    pub fn from_workflow_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let required_keys = ["requiresApproval", "requires_approval", "mandatory", "approvalRequired"];
        let timeout_keys = ["timeoutSecs", "timeout_secs", "timeout"];
        let recognized = required_keys.iter().any(|key| object.contains_key(*key))
            || timeout_keys.iter().any(|key| object.contains_key(*key))
            || object.contains_key("reason");
        if !recognized {
            return None;
        }
        let mandatory = required_keys
            .iter()
            .find_map(|key| object.get(*key).and_then(serde_json::Value::as_bool))
            .unwrap_or(false);
        let timeout_secs = timeout_keys
            .iter()
            .find_map(|key| object.get(*key).and_then(serde_json::Value::as_u64))
            .unwrap_or(60);
        let reason = object
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        Some(Self {
            mandatory,
            timeout_secs,
            reason,
        })
    }
}

/// Fail-closed error when a pinned Workflow revision's mandatory policy no
/// longer matches the currently resolved policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPolicyError {
    /// The pinned revision and the current effective policy differ. The old
    /// configuration must not bypass newer mandatory requirements.
    RevisionPolicyMismatch {
        pinned: ApprovalPolicy,
        current: ApprovalPolicy,
    },
}

impl fmt::Display for ToolPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RevisionPolicyMismatch { pinned, current } => write!(
                formatter,
                "tool policy revision mismatch: pinned={pinned:?} current={current:?}"
            ),
        }
    }
}

impl std::error::Error for ToolPolicyError {}

/// Verify the resolved policy still matches the policy pinned into a Workflow
/// revision. Any difference fails closed and requires a new revision.
pub fn verify_revision_policy(pinned: &ApprovalPolicy, current: &ApprovalPolicy) -> Result<(), ToolPolicyError> {
    if pinned == current {
        return Ok(());
    }
    Err(ToolPolicyError::RevisionPolicyMismatch {
        pinned: pinned.clone(),
        current: current.clone(),
    })
}

pub use tinyiothub_core::redaction::{DEFAULT_SENSITIVE_KEYS, RedactionPolicy};

/// Generic UI metadata declared by the tool. The frontend renders this without
/// any per-tool event allowlist or unescaped HTML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationDescriptor {
    pub label: String,
    pub group: String,
    pub group_label: String,
    pub danger: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl PresentationDescriptor {
    /// Conservative presentation inferred from the name and effect kind. Tools
    /// may override every field; this is only the fallback so a newly declared
    /// tool still appears in the catalog without central edits.
    pub fn inferred(name: &str, danger: bool) -> Self {
        let (group, group_label) = infer_group(name);
        Self {
            label: name.to_string(),
            group: group.to_string(),
            group_label: group_label.to_string(),
            danger,
            enabled: !danger,
            icon: None,
        }
    }
}

fn infer_group(name: &str) -> (&'static str, &'static str) {
    if name == "search_workspace_resources" {
        ("workspace", "工作空间")
    } else if matches!(
        name,
        "list_devices"
            | "read_device"
            | "call_device"
            | "set_device"
            | "stop"
            | "watch"
            | "unwatch"
            | "list_composites"
            | "run_composite"
    ) || name.starts_with("search_")
        || matches!(name, "read_properties" | "write_properties" | "send_command")
    {
        ("device", "设备管理")
    } else if name.starts_with("alarm_") {
        ("alarm", "告警管理")
    } else if matches!(name, "list_drivers" | "test_driver") {
        ("driver", "驱动管理")
    } else if name.ends_with("_schedule") || name.ends_with("_schedules") {
        ("job", "任务管理")
    } else if matches!(
        name,
        "list_things"
            | "get_thing"
            | "get_thing_profile"
            | "get_thing_tree"
            | "read_property"
            | "invoke_action"
            | "query_events"
            | "search_knowledge"
            | "read_document"
            | "create_thing"
            | "delete_thing"
    ) {
        ("thing", "物本体")
    } else {
        ("other", "其他")
    }
}

/// A registered tool's full declaration.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub category: String,
    pub safety: ToolSafety,
    /// JSON Schema of the tool arguments.
    pub parameters: serde_json::Value,
    pub output_schema: Option<OutputSchema>,
    pub requires_approval: bool,
    /// Short instruction telling the model when to use this tool.
    pub prompt_summary: String,
    pub effect_kind: ToolEffectKind,
    pub required_capabilities: Vec<Capability>,
    pub approval_policy: ApprovalPolicy,
    pub redaction: RedactionPolicy,
    pub presentation: PresentationDescriptor,
    /// Whether the tool enters the model's tool list and catalog. Hidden
    /// aliases stay registered for approval resolution only.
    pub exposed: bool,
}

impl ToolDescriptor {
    /// Build a descriptor from the minimal declaration (name/description/
    /// argument schema). Trust, effect kind, approval, and presentation fall
    /// back to safe inferences; a tool overrides whichever fields it needs.
    pub fn declared(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        let name = name.into();
        let safety = classify_tool_safety(&name);
        let effect_kind = ToolEffectKind::from_safety(safety);
        let mandatory = matches!(safety, ToolSafety::Destructive);
        let approval_policy = if mandatory {
            ApprovalPolicy::mandatory("destructive tool")
        } else {
            ApprovalPolicy::none()
        };
        let danger = matches!(safety, ToolSafety::Destructive);
        Self {
            presentation: PresentationDescriptor::inferred(&name, danger),
            name,
            description: description.into(),
            category: "general".to_string(),
            safety,
            parameters,
            output_schema: None,
            requires_approval: mandatory,
            prompt_summary: String::new(),
            effect_kind,
            required_capabilities: Vec::new(),
            approval_policy,
            redaction: RedactionPolicy::default(),
            exposed: true,
        }
    }

    /// Description sent with the LLM tool declaration. The prompt summary is
    /// appended when present so a descriptor controls both the stable
    /// description and the usage hint.
    pub fn llm_description(&self) -> String {
        if self.prompt_summary.trim().is_empty() {
            self.description.clone()
        } else {
            format!("{}\n\n{}", self.description, self.prompt_summary)
        }
    }

    /// One entry of the LLM function-calling tools array.
    pub fn llm_tool_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "description": self.llm_description(),
            "parameters": self.parameters,
        })
    }

    /// Capabilities required by this tool that `available` does not cover.
    pub fn missing_capabilities(&self, available: &[Capability]) -> Vec<Capability> {
        self.required_capabilities
            .iter()
            .copied()
            .filter(|capability| !capability.is_satisfied_by(available))
            .collect()
    }

    /// Effective approval policy after applying an optional Workflow policy.
    /// Workflow policy can only tighten the declared requirement.
    pub fn effective_approval(&self, workflow: Option<&ApprovalPolicy>) -> ApprovalPolicy {
        self.approval_policy.tighten(workflow)
    }

    /// Validate arguments against the declared JSON schema. This is a
    /// deliberately small required/type check (the dispatch layer still owns
    /// full schema enforcement); it fails closed before an Effect is created.
    pub fn validate_arguments(&self, arguments: &serde_json::Value) -> Result<(), ToolRegistryError> {
        let object = arguments
            .as_object()
            .ok_or_else(|| ToolRegistryError::InvalidArguments {
                name: self.name.clone(),
                reason: "arguments must be a JSON object".to_string(),
            })?;
        let Some(schema) = self.parameters.as_object() else {
            return Ok(());
        };
        if let Some(required) = schema.get("required").and_then(|value| value.as_array()) {
            for key in required.iter().filter_map(|value| value.as_str()) {
                if !object.contains_key(key) {
                    return Err(ToolRegistryError::InvalidArguments {
                        name: self.name.clone(),
                        reason: format!("missing required argument `{key}`"),
                    });
                }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(|value| value.as_object()) {
            for (key, value) in object {
                let Some(expected) = properties.get(key).and_then(|property| property.get("type")) else {
                    continue;
                };
                let Some(expected) = expected.as_str() else {
                    continue;
                };
                if !json_type_matches(value, expected) {
                    return Err(ToolRegistryError::InvalidArguments {
                        name: self.name.clone(),
                        reason: format!("argument `{key}` must be {expected}"),
                    });
                }
            }
        }
        Ok(())
    }

    /// Deeply redacted arguments.
    pub fn redacted_arguments(&self, arguments: &serde_json::Value) -> serde_json::Value {
        self.redaction.redact(arguments)
    }

    /// Compact, bounded, secret-redacted summary suitable for a public DTO/SSE
    /// metadata field. The raw `request_json` never enters this string.
    pub fn arguments_summary(&self, arguments: &serde_json::Value) -> Result<String, ToolRegistryError> {
        self.validate_arguments(arguments)?;
        let redacted = self.redacted_arguments(arguments);
        let summary = serde_json::to_string(&redacted).map_err(|error| ToolRegistryError::InvalidArguments {
            name: self.name.clone(),
            reason: error.to_string(),
        })?;
        Ok(truncate_chars(&summary, 512))
    }

    /// Generic catalog entry consumed by `/tools/catalog`.
    pub fn catalog_entry(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.name,
            "name": self.name,
            "label": self.presentation.label,
            "description": self.description,
            "danger": self.presentation.danger,
            "enabled": self.presentation.enabled,
            "effectKind": self.effect_kind,
            "requiresApproval": self.effective_approval(None).mandatory,
            "requiredCapabilities": self.required_capabilities,
            "icon": self.presentation.icon,
        })
    }
}

fn json_type_matches(value: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(max).collect();
    truncated.push('…');
    truncated
}

/// Errors from descriptor-based registry operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRegistryError {
    /// The tool is not declared.
    UnknownTool { name: String },
    /// Arguments do not match the declared schema.
    InvalidArguments { name: String, reason: String },
}

impl fmt::Display for ToolRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool { name } => write!(formatter, "unknown tool `{name}`"),
            Self::InvalidArguments { name, reason } => write!(formatter, "invalid arguments for `{name}`: {reason}"),
        }
    }
}

impl std::error::Error for ToolRegistryError {}

/// Central registry for tool discovery and validation. Every exposure surface
/// is generated from the same descriptors.
pub trait ToolRegistry: Send + Sync {
    /// All registered declarations (exposed and hidden).
    fn descriptors(&self) -> Vec<Arc<ToolDescriptor>>;

    /// Look up a tool by name (including hidden approval-only aliases).
    fn get(&self, name: &str) -> Option<Arc<ToolDescriptor>> {
        self.descriptors()
            .into_iter()
            .find(|descriptor| descriptor.name == name)
    }

    /// List all registered tools, optionally filtered by category.
    fn list(&self, category: Option<&str>) -> Vec<Arc<ToolDescriptor>> {
        self.descriptors()
            .into_iter()
            .filter(|descriptor| category.is_none_or(|category| descriptor.category == category))
            .collect()
    }

    /// Generate the LLM function-calling schema for every available exposed
    /// tool.
    fn generate_llm_schema(&self, category: Option<&str>) -> serde_json::Value {
        self.generate_llm_schema_for(&Capability::all(), category)
    }

    /// Generate the LLM tool subset for the capabilities the composition root
    /// actually resolved. A missing capability removes the tool; the run-level
    /// gate must fail before the model call when a required tool disappears.
    fn generate_llm_schema_for(&self, available: &[Capability], category: Option<&str>) -> serde_json::Value {
        let tools: Vec<serde_json::Value> = self
            .list(category)
            .into_iter()
            .filter(|descriptor| descriptor.exposed && descriptor.missing_capabilities(available).is_empty())
            .map(|descriptor| descriptor.llm_tool_schema())
            .collect();
        serde_json::json!({ "tools": tools })
    }

    /// Exposed descriptors available under `available`.
    fn exposed_descriptors(&self, available: &[Capability]) -> Vec<Arc<ToolDescriptor>> {
        self.descriptors()
            .into_iter()
            .filter(|descriptor| descriptor.exposed && descriptor.missing_capabilities(available).is_empty())
            .collect()
    }

    /// Build the `/tools/catalog` payload from the same declarations.
    fn catalog(&self) -> serde_json::Value {
        let mut groups: BTreeMap<String, (String, Vec<serde_json::Value>)> = BTreeMap::new();
        for descriptor in self.descriptors().into_iter().filter(|descriptor| descriptor.exposed) {
            let presentation = &descriptor.presentation;
            groups
                .entry(presentation.group.clone())
                .or_insert_with(|| (presentation.group_label.clone(), Vec::new()))
                .1
                .push(descriptor.catalog_entry());
        }
        // Preserve the established workbench group order; newly declared groups
        // are appended alphabetically so a new tool never needs a frontend list
        // edit to become visible.
        const PREFERRED_GROUP_ORDER: &[&str] = &[
            "thing",
            "device",
            "alarm",
            "monitoring",
            "driver",
            "workspace",
            "job",
            "other",
        ];
        let group_rank = |id: &str| {
            PREFERRED_GROUP_ORDER
                .iter()
                .position(|preferred| *preferred == id)
                .unwrap_or(PREFERRED_GROUP_ORDER.len())
        };
        let mut entries: Vec<(String, (String, Vec<serde_json::Value>))> = groups.into_iter().collect();
        entries.sort_by(|(left, _), (right, _)| group_rank(left).cmp(&group_rank(right)).then_with(|| left.cmp(right)));
        let groups: Vec<serde_json::Value> = entries
            .into_iter()
            .map(|(id, (label, tools))| {
                serde_json::json!({
                    "id": id,
                    "label": label,
                    "source": "core",
                    "tools": tools,
                })
            })
            .collect();
        serde_json::json!({ "groups": groups })
    }

    /// Redacted argument summary for public DTO/SSE metadata.
    fn redacted_arguments(&self, name: &str, arguments: &serde_json::Value) -> Result<String, ToolRegistryError> {
        let descriptor = self
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool { name: name.to_string() })?;
        descriptor.arguments_summary(arguments)
    }

    /// Validate arguments before dispatch.
    fn validate_arguments(&self, name: &str, arguments: &serde_json::Value) -> Result<(), ToolRegistryError> {
        let descriptor = self
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool { name: name.to_string() })?;
        descriptor.validate_arguments(arguments)
    }

    /// Effective approval policy for a tool, optional Workflow tightening
    /// applied. Workflow policy can never loosen the declaration.
    fn effective_approval(
        &self,
        name: &str,
        workflow: Option<&ApprovalPolicy>,
    ) -> Result<ApprovalPolicy, ToolRegistryError> {
        let descriptor = self
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool { name: name.to_string() })?;
        Ok(descriptor.effective_approval(workflow))
    }

    /// Missing capabilities for a tool under the resolved capability set.
    fn missing_capabilities(&self, name: &str, available: &[Capability]) -> Result<Vec<Capability>, ToolRegistryError> {
        let descriptor = self
            .get(name)
            .ok_or_else(|| ToolRegistryError::UnknownTool { name: name.to_string() })?;
        Ok(descriptor.missing_capabilities(available))
    }

    /// Check if a tool name is registered.
    fn exists(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

/// In-memory registry built purely from declarations. Composition layers use
/// this directly, or adapt their runtime tool list into descriptors and re-use
/// the default trait methods.
#[derive(Default, Clone)]
pub struct DeclaredToolRegistry {
    descriptors: Vec<Arc<ToolDescriptor>>,
}

impl DeclaredToolRegistry {
    pub fn new(descriptors: Vec<ToolDescriptor>) -> Self {
        Self {
            descriptors: descriptors.into_iter().map(Arc::new).collect(),
        }
    }

    pub fn register(&mut self, descriptor: ToolDescriptor) {
        self.descriptors.push(Arc::new(descriptor));
    }
}

impl ToolRegistry for DeclaredToolRegistry {
    fn descriptors(&self) -> Vec<Arc<ToolDescriptor>> {
        self.descriptors.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn new_tool() -> ToolDescriptor {
        ToolDescriptor::declared(
            "inspect_camera",
            "Capture a camera frame",
            json!({
                "type": "object",
                "properties": {
                    "thingId": { "type": "string" },
                    "credentials": { "type": "object" },
                    "frames": { "type": "array" }
                },
                "required": ["thingId"]
            }),
        )
    }

    #[test]
    fn declaration_only_tool_reaches_llm_exposure_and_catalog() {
        // Registering a descriptor is the only change a new tool needs. No
        // core edit and no frontend allowlist edit.
        let mut descriptor = new_tool();
        descriptor.prompt_summary = "Use when current visual state matters.".into();
        descriptor.effect_kind = ToolEffectKind::Read;
        descriptor.required_capabilities = vec![Capability::Vision];
        descriptor.presentation.label = "查看画面".into();
        descriptor.presentation.group = "device".into();
        descriptor.presentation.group_label = "设备管理".into();

        let registry = DeclaredToolRegistry::new(vec![descriptor]);
        let llm = registry.generate_llm_schema_for(&[Capability::Vision], None);
        let tools = llm["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "inspect_camera");
        assert_eq!(
            tools[0]["description"],
            "Capture a camera frame\n\nUse when current visual state matters."
        );

        let catalog = registry.catalog();
        let group = catalog["groups"]
            .as_array()
            .expect("groups")
            .iter()
            .find(|group| group["id"] == "device")
            .expect("declared group appears without a hardcoded catalog edit");
        assert_eq!(group["tools"][0]["label"], "查看画面");
        assert_eq!(group["tools"][0]["danger"], false);
        assert_eq!(group["tools"][0]["enabled"], true);
    }

    #[test]
    fn missing_capability_removes_the_tool_from_the_exposed_subset() {
        let mut descriptor = new_tool();
        descriptor.required_capabilities = vec![Capability::Video];
        let registry = DeclaredToolRegistry::new(vec![descriptor]);

        assert!(
            registry.generate_llm_schema_for(&[Capability::Vision], None)["tools"]
                .as_array()
                .expect("tools")
                .is_empty()
        );
        assert_eq!(
            registry.missing_capabilities("inspect_camera", &[Capability::Vision]),
            Ok(vec![Capability::Video])
        );
        assert_eq!(
            registry.missing_capabilities("inspect_camera", &[Capability::Vision, Capability::Video]),
            Ok(vec![])
        );
    }

    #[test]
    fn nested_password_token_and_secret_fields_are_redacted() {
        let descriptor = new_tool();
        let summary = descriptor
            .arguments_summary(&json!({
                "thingId": "camera-1",
                "password": "outer-secret",
                "nested": {
                    "access_token": "token-value",
                    "clientSecret": "client-secret",
                    "safe": "visible"
                },
                "frames": [
                    { "token": "array-token", "url": "rtsp://camera/1" },
                    { "credentials": { "password": "deep-secret" } }
                ]
            }))
            .expect("valid arguments");

        for leaked in [
            "outer-secret",
            "token-value",
            "client-secret",
            "array-token",
            "deep-secret",
        ] {
            assert!(!summary.contains(leaked), "redacted summary leaked {leaked}: {summary}");
        }
        assert!(summary.contains("[REDACTED]"), "summary must mark redaction: {summary}");
        assert!(summary.contains("visible"));
        assert!(summary.contains("camera-1"));
    }

    #[test]
    fn unknown_tool_and_invalid_arguments_are_rejected() {
        let registry = DeclaredToolRegistry::new(vec![new_tool()]);

        assert_eq!(
            registry.redacted_arguments("missing_tool", &json!({})),
            Err(ToolRegistryError::UnknownTool {
                name: "missing_tool".into()
            })
        );
        assert!(matches!(
            registry.validate_arguments("inspect_camera", &json!({"thingId": 7})),
            Err(ToolRegistryError::InvalidArguments { .. })
        ));
        assert!(matches!(
            registry.validate_arguments("inspect_camera", &json!({})),
            Err(ToolRegistryError::InvalidArguments { .. })
        ));
        assert_eq!(
            registry.validate_arguments("inspect_camera", &json!({"thingId": "camera-1"})),
            Ok(())
        );
    }

    #[test]
    fn workflow_approval_can_only_tighten_mandatory_policy() {
        let mut descriptor = new_tool();
        descriptor.approval_policy = ApprovalPolicy::mandatory("device safety");
        descriptor.requires_approval = true;

        let workflow_disables = ApprovalPolicy {
            mandatory: false,
            timeout_secs: 5,
            reason: None,
        };
        let effective = descriptor.effective_approval(Some(&workflow_disables));
        assert!(effective.mandatory, "workflow must not disable mandatory approval");
        assert_eq!(
            effective.timeout_secs, 60,
            "workflow must not shorten the approval window"
        );

        let workflow_tightens = ApprovalPolicy {
            mandatory: true,
            timeout_secs: 120,
            reason: Some("workflow requires review".into()),
        };
        let effective = descriptor.effective_approval(Some(&workflow_tightens));
        assert!(effective.mandatory);
        assert_eq!(effective.timeout_secs, 120);
    }

    #[test]
    fn current_policy_must_match_the_pinned_revision() {
        let pinned = ApprovalPolicy {
            mandatory: true,
            timeout_secs: 60,
            reason: Some("safety".into()),
        };

        assert_eq!(verify_revision_policy(&pinned, &pinned), Ok(()));
        assert!(matches!(
            verify_revision_policy(
                &pinned,
                &ApprovalPolicy {
                    mandatory: false,
                    timeout_secs: 60,
                    reason: None,
                }
            ),
            Err(ToolPolicyError::RevisionPolicyMismatch { .. })
        ));
        assert!(matches!(
            verify_revision_policy(
                &pinned,
                &ApprovalPolicy {
                    mandatory: true,
                    timeout_secs: 10,
                    reason: None,
                }
            ),
            Err(ToolPolicyError::RevisionPolicyMismatch { .. })
        ));
    }

    #[test]
    fn workflow_policy_value_cannot_supply_a_weaker_effective_policy() {
        // Template/persistence shape: boolean + timeout. Effective resolution
        // still ORs with the descriptor's mandatory policy.
        let workflow = ApprovalPolicy::from_workflow_value(&json!({
            "requiresApproval": false,
            "timeoutSecs": 1
        }))
        .expect("recognized workflow policy");
        let mut descriptor = new_tool();
        descriptor.approval_policy = ApprovalPolicy::mandatory("irreversible device action");
        let effective = descriptor.effective_approval(Some(&workflow));
        assert!(effective.mandatory);
        assert_eq!(effective.timeout_secs, 60);
    }

    #[test]
    fn video_requires_a_vision_capable_provider() {
        assert!(!Capability::Video.is_satisfied_by(&[Capability::Video]));
        assert!(Capability::Video.is_satisfied_by(&[Capability::Vision, Capability::Video]));
        assert!(!Capability::Vision.is_satisfied_by(&[Capability::Video]));
    }

    #[test]
    fn sensitive_containers_never_leak_scalar_descendants() {
        let descriptor = new_tool();
        for (arguments, leaked) in [
            (
                json!({"thingId": "camera-1", "credentials": {"value": "hunter2"}}),
                "hunter2",
            ),
            (json!({"thingId": "camera-1", "tokens": ["abc", "def"]}), "abc"),
            (json!({"thingId": "camera-1", "secrets": [{"key1": "x"}]}), "x"),
        ] {
            let summary = descriptor.arguments_summary(&arguments).expect("valid argument shape");
            assert!(!summary.contains(leaked), "redacted summary leaked {leaked}: {summary}");
            assert!(summary.contains("[REDACTED]"), "{summary}");
        }
    }

    #[test]
    fn unrecognized_workflow_policy_shapes_are_rejected() {
        assert_eq!(ApprovalPolicy::from_workflow_value(&json!({})), None);
        assert_eq!(
            ApprovalPolicy::from_workflow_value(&json!({"unknownKey": true, "note": "nope"})),
            None
        );
        assert!(ApprovalPolicy::from_workflow_value(&json!({"requiresApproval": false})).is_some());
    }
}
