//! Versioned built-in Workflow templates (X1) and their instantiation
//! validation.
//!
//! Templates are data only: each file is a [`WorkflowDefinition`] plus stable
//! template metadata. Instantiation reuses the same Workflow revision store
//! and the normal trigger scheduler — there is no scenario-specific execution
//! loop. DR4=B is the only difference from a blank create: a template that
//! passes all reference checks is persisted directly enabled.

use std::collections::HashSet;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::harness::{WorkflowDefinition, WorkflowTrigger};
use tinyiothub_skills::registry::{ApprovalPolicy, Capability};
use tinyiothub_skills::trust::{ToolSafety, classify_tool_safety};
use tinyiothub_storage::{Db, DbError};

use crate::domains::agent::prompt::paths;
use crate::domains::agent::tools::build_tools_catalog_json;

use super::normalize_cron;

const INSPECTION_JSON: &str = include_str!("../../../../templates/workflows/inspection.json");
const VIDEO_JSON: &str = include_str!("../../../../templates/workflows/video.json");
const ENERGY_JSON: &str = include_str!("../../../../templates/workflows/energy.json");

/// Stable built-in Workflow tool surface. Shipped templates may only
/// reference this surface: a versioned template must stay portable instead of
/// depending on whichever driver tools happen to be installed in one
/// workspace. Workspace device manifests are still consulted for source and
/// mandatory-approval validation.
const BUILTIN_DEVICE_TOOLS: &[&str] = &[
    "list_devices",
    "read_device",
    "call_device",
    "set_device",
    "stop",
    "watch",
    "unwatch",
    "list_composites",
    "run_composite",
    "search_knowledge",
    "read_document",
    "query_events",
    "read_property",
    "invoke_action",
    "get_thing_profile",
    "get_thing_tree",
    "search_workspace_resources",
    "get_skill",
];

/// Evidence kinds a Workflow template may request. Dispatchers and the
/// evidence UI consume these stable identifiers.
const EVIDENCE_KINDS: &[&str] = &[
    "property_snapshot",
    "event",
    "media",
    "tool_result",
    "observation",
    "device_state",
];

/// One versioned built-in template. The embedded definition is the same
/// unified schema edited by hand in the Workflow editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTemplate {
    pub id: String,
    pub version: i64,
    pub title: String,
    pub description: String,
    pub definition: WorkflowDefinition,
}

/// User-supplied instantiation parameters. `workflow_id` is optional; an
/// empty value lets the server generate a stable ID. Selector/policy fields
/// override the template defaults, with approval merging restricted to
/// tightening.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TemplateInput {
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub device_selector: Value,
    #[serde(default)]
    pub source_selector: Value,
    #[serde(default)]
    pub approval_policy: Value,
}

/// The shipped template catalog, in stable display order.
pub fn builtin_templates() -> Vec<WorkflowTemplate> {
    [INSPECTION_JSON, VIDEO_JSON, ENERGY_JSON]
        .into_iter()
        .map(|raw| serde_json::from_str(raw).expect("built-in Workflow template JSON must parse"))
        .collect()
}

pub fn find_template(template_id: &str) -> Option<WorkflowTemplate> {
    builtin_templates()
        .into_iter()
        .find(|template| template.id == template_id)
}

/// Static template contract checks. This runs before any workspace lookup, so
/// an invalid template can never create a Workflow or a scheduling artifact.
pub fn validate_template(template: &WorkflowTemplate) -> Result<(), DbError> {
    if template.id.trim().is_empty() {
        return Err(validation("workflow template ID must not be blank"));
    }
    if template.version < 1 {
        return Err(validation("workflow template version must be positive"));
    }
    if template.title.trim().is_empty() {
        return Err(validation("workflow template title must not be blank"));
    }
    let definition = &template.definition;
    if definition.objective.trim().is_empty() {
        return Err(validation("workflow objective must not be blank"));
    }
    if definition.trigger.trigger_id().trim().is_empty() {
        return Err(validation("workflow trigger ID must not be blank"));
    }
    if let WorkflowTrigger::Cron { expression, .. } = &definition.trigger {
        Schedule::from_str(&normalize_cron(expression))
            .map_err(|error| validation(format!("invalid workflow cron: {error}")))?;
    }
    validate_selectors(definition)?;
    validate_tools(definition)?;
    validate_skills(definition)?;
    validate_capabilities(&template.id, definition)?;
    validate_approval_policy(definition)?;
    validate_evidence_policy(&definition.evidence_policy)?;
    Ok(())
}

fn validate_selectors(definition: &WorkflowDefinition) -> Result<(), DbError> {
    for (field, selector) in [
        ("device selector", &definition.device_selector),
        ("source selector", &definition.source_selector),
    ] {
        if !selector.is_object() {
            return Err(validation(format!("workflow {field} must be a JSON object")));
        }
    }
    Ok(())
}

fn validate_tools(definition: &WorkflowDefinition) -> Result<(), DbError> {
    if definition.tool_ids.is_empty() {
        return Err(validation("workflow templates must reference at least one tool"));
    }
    let known = static_tool_ids();
    let mut seen = HashSet::new();
    for tool in &definition.tool_ids {
        if tool.trim().is_empty() {
            return Err(validation("workflow tool IDs must not be blank"));
        }
        if !known.contains(tool.as_str()) {
            return Err(validation(format!("workflow template references unknown tool: {tool}")));
        }
        if !seen.insert(tool.as_str()) {
            return Err(validation(format!("workflow template repeats tool: {tool}")));
        }
    }
    Ok(())
}

fn validate_skills(definition: &WorkflowDefinition) -> Result<(), DbError> {
    for skill in &definition.skill_refs {
        if !skill_available(skill, None) {
            return Err(validation(format!(
                "workflow template references unavailable skill: {skill}"
            )));
        }
    }
    Ok(())
}

fn validate_capabilities(template_id: &str, definition: &WorkflowDefinition) -> Result<(), DbError> {
    let mut capabilities = Vec::new();
    for raw in &definition.required_capabilities {
        let capability = Capability::all()
            .into_iter()
            .find(|candidate| candidate.as_str() == raw)
            .ok_or_else(|| validation(format!("unknown workflow capability: {raw}")))?;
        capabilities.push(capability);
    }
    if capabilities.contains(&Capability::Video) && !capabilities.contains(&Capability::Vision) {
        return Err(validation("video workflows must require the vision capability"));
    }
    if template_id == "video" && !capabilities.contains(&Capability::Vision) {
        return Err(validation(
            "the video workflow template must require the vision capability",
        ));
    }
    Ok(())
}

fn validate_approval_policy(definition: &WorkflowDefinition) -> Result<(), DbError> {
    if !definition.approval_policy.is_object() {
        return Err(validation("workflow approval policy must be a JSON object"));
    }
    if policy_declares_no_approval(&definition.approval_policy)
        && definition
            .tool_ids
            .iter()
            .any(|tool| classify_tool_safety(tool) == ToolSafety::Destructive)
    {
        return Err(validation(
            "workflow template must not disable mandatory approval for a destructive tool",
        ));
    }
    Ok(())
}

fn validate_evidence_policy(value: &Value) -> Result<(), DbError> {
    let Some(object) = value.as_object() else {
        return Err(validation("workflow evidence policy must be a JSON object"));
    };
    for key in object.keys() {
        if key != "required" && key != "kinds" {
            return Err(validation(format!("unknown evidence policy key: {key}")));
        }
    }
    let required = match object.get("required") {
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err(validation("evidence policy required must be a boolean")),
        None => false,
    };
    let mut kinds = Vec::new();
    match object.get("kinds") {
        Some(Value::Array(items)) => {
            for item in items {
                let Some(kind) = item.as_str() else {
                    return Err(validation("evidence policy kinds must be strings"));
                };
                if !EVIDENCE_KINDS.contains(&kind) {
                    return Err(validation(format!("unknown evidence kind: {kind}")));
                }
                kinds.push(kind);
            }
        }
        Some(_) => return Err(validation("evidence policy kinds must be an array")),
        None => {}
    }
    if required && kinds.is_empty() {
        return Err(validation("evidence policy requires at least one evidence kind"));
    }
    Ok(())
}

/// Workspace-specific reference checks. All reads happen before the first
/// write, so any failure leaves zero Workflows and zero scheduling rows.
pub(crate) async fn validate_workspace_references(
    db: &Db,
    workspace_id: &str,
    template_id: &str,
    definition: &WorkflowDefinition,
) -> Result<(), DbError> {
    let device_ids = selected_device_ids(&definition.device_selector)?;
    let source_ids = selected_source_ids(&definition.source_selector)?;
    if source_ids.is_empty() {
        return Err(validation("workflow source selector must name at least one source"));
    }

    let mut dangerous_tools = HashSet::new();
    for device_id in &device_ids {
        let reference = db.workflow_device_reference(workspace_id, device_id).await?;
        if !reference.found {
            return Err(validation(format!("device not found in workspace: {device_id}")));
        }
        for source_id in &source_ids {
            let Some(declared) = reference.sources.iter().find(|candidate| &candidate.id == source_id) else {
                return Err(validation(format!(
                    "device {device_id} does not declare source: {source_id}"
                )));
            };
            if template_id == "video" && !matches!(declared.data_kind.as_str(), "image" | "video") {
                return Err(validation(format!(
                    "video template requires an image or video source: {source_id}"
                )));
            }
        }
        dangerous_tools.extend(reference.dangerous_tool_ids.iter().cloned());
    }

    if policy_declares_no_approval(&definition.approval_policy)
        && definition
            .tool_ids
            .iter()
            .any(|tool| dangerous_tools.contains(tool) || classify_tool_safety(tool) == ToolSafety::Destructive)
    {
        return Err(validation(
            "workflow template must not disable mandatory approval for a device control tool",
        ));
    }

    // Skills resolve through the workspace layer first (workspace → global →
    // shipped template bundle). The static check above already rejects refs
    // that are unknown everywhere; this pass makes the workspace the authority.
    for skill in &definition.skill_refs {
        if !skill_available(skill, Some(workspace_id)) {
            return Err(validation(format!(
                "workflow template references unavailable skill: {skill}"
            )));
        }
    }
    Ok(())
}

/// Cron-backed definitions expose their next planned wake. Watch/event
/// triggers are not time based, so they have no planned timestamp.
pub fn next_wake_at(definition: &WorkflowDefinition, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let WorkflowTrigger::Cron { expression, .. } = &definition.trigger else {
        return None;
    };
    Schedule::from_str(&normalize_cron(expression)).ok()?.after(&now).next()
}

pub(crate) fn selector_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(object) => object.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::String(value) => value.trim().is_empty(),
        _ => false,
    }
}

fn selected_device_ids(value: &Value) -> Result<Vec<String>, DbError> {
    let raw = match value {
        Value::Array(items) => Some(items.clone()),
        Value::Object(object) => ["device_ids", "deviceIds"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_array).cloned()),
        _ => None,
    }
    .unwrap_or_default();
    let mut ids = Vec::new();
    for item in raw {
        if let Some(id) = item.as_str() {
            let id = id.trim();
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    if ids.is_empty() {
        return Err(validation("workflow device selector must name at least one device"));
    }
    Ok(ids)
}

fn selected_source_ids(value: &Value) -> Result<Vec<String>, DbError> {
    let raw: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        Value::String(source) => vec![Value::String(source.clone())],
        Value::Object(object) => ["sources", "source_ids", "sourceIds"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_array).cloned())
            .or_else(|| object.get("source").map(|source| vec![source.clone()]))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut ids = Vec::new();
    for item in raw {
        if let Some(id) = item.as_str() {
            let id = id.trim();
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    Ok(ids)
}

fn policy_declares_no_approval(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    ["mandatory", "requiresApproval", "requires_approval", "approvalRequired"]
        .iter()
        .any(|key| matches!(object.get(*key).and_then(Value::as_bool), Some(false)))
}

fn static_tool_ids() -> HashSet<String> {
    let mut ids: HashSet<String> = BUILTIN_DEVICE_TOOLS.iter().map(|tool| (*tool).to_string()).collect();
    if let Some(groups) = build_tools_catalog_json().get("groups").and_then(Value::as_array) {
        for group in groups {
            for tool in group.get("tools").and_then(Value::as_array).into_iter().flatten() {
                if let Some(id) = tool.get("id").and_then(Value::as_str) {
                    ids.insert(id.to_string());
                }
            }
        }
    }
    ids
}

fn skill_available(raw: &str, workspace_id: Option<&str>) -> bool {
    let name = raw.split('@').next().unwrap_or(raw).trim();
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
    {
        return false;
    }
    let mut dirs = vec![
        paths::global_skills_dir(),
        paths::project_root().join("templates/skills"),
        // Repository layout: apps/cloud/templates/skills. Production layout:
        // <project_root>/templates/skills (the Docker image copies the same
        // directory).
        paths::project_root().join("apps/cloud/templates/skills"),
    ];
    if let Some(workspace_id) = workspace_id {
        dirs.insert(0, paths::workspace_skills_dir(workspace_id));
    }
    dirs.iter().any(|dir| dir.join(format!("{name}.md")).is_file())
}

fn validation(message: impl Into<String>) -> DbError {
    DbError::Validation {
        message: message.into(),
    }
}

/// Effective approval policy: template defaults may be tightened by the
/// user's input, never loosened.
pub(crate) fn effective_approval_policy(template: &WorkflowDefinition, input: &Value) -> Result<Value, DbError> {
    let template_policy =
        ApprovalPolicy::from_workflow_value(&template.approval_policy).unwrap_or_else(ApprovalPolicy::none);
    let input_policy = ApprovalPolicy::from_workflow_value(input);
    Ok(serde_json::to_value(template_policy.tighten(input_policy.as_ref()))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_catalog_parses_and_validates() {
        let templates = builtin_templates();
        let ids: Vec<&str> = templates.iter().map(|template| template.id.as_str()).collect();
        assert_eq!(ids, tinyiothub_storage::seed::BUILTIN_WORKFLOW_TEMPLATE_IDS);
        for template in templates {
            validate_template(&template).expect("built-in template validates");
        }
    }
}
