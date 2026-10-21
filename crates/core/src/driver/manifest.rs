//! Driver manifest v1 — declarative driver access contract.
//!
//! Pure types + validation only (no I/O). Runtime registries, persistence and
//! the reference driver are built on these shapes by later tasks.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MANIFEST_MAJOR: u32 = 1;
pub const MANIFEST_MINOR: u32 = 0;
pub const MANIFEST_FILE: &str = "driver.manifest.json";
pub const DRIVER_ABI_V2: u32 = 2;
pub const MAX_INSTANCE_STATE_FIELDS: usize = 256;
pub const MAX_INSTANCE_TOOLS: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DriverManifest {
    pub manifest_version: String,
    pub identity: ManifestIdentity,
    #[serde(default = "default_config_schema")]
    pub config: serde_json::Value,
    #[serde(default)]
    pub capabilities: ManifestCapabilities,
    #[serde(default)]
    pub tools: Vec<ManifestTool>,
    #[serde(default)]
    pub state_fields: Vec<StateFieldDecl>,
    #[serde(default)]
    pub sources: Vec<SourceDecl>,
    #[serde(default)]
    pub resources: Vec<ResourceDecl>,
    #[serde(default)]
    pub placement: PlacementDecl,
    #[serde(default)]
    pub limits: ManifestLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestIdentity {
    pub driver: String,
    pub version: String,
    pub protocol: String,
    pub display_name: Option<String>,
    pub abi_version: u32,
    #[serde(default)]
    pub dynamic_capabilities: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCapabilities {
    #[serde(default)]
    pub cancel: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolKindDecl {
    #[default]
    Action,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ToolSafetyDecl {
    Safe,
    #[default]
    Control,
    Dangerous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolTimeout {
    #[serde(default = "default_timeout_ms")]
    pub default_ms: u64,
    #[serde(default = "default_max_timeout_ms")]
    pub max_ms: u64,
}

fn default_timeout_ms() -> u64 {
    3000
}
fn default_max_timeout_ms() -> u64 {
    5000
}
fn default_config_schema() -> serde_json::Value {
    serde_json::json!({"type": "object"})
}
fn default_true() -> bool {
    true
}
fn default_interval_ms() -> u64 {
    1000
}
fn default_min_interval_ms() -> u64 {
    200
}
fn default_capacity() -> u32 {
    1
}
fn default_concurrent() -> u32 {
    1
}
fn default_calls_per_min() -> u32 {
    60
}

impl Default for ToolTimeout {
    fn default() -> Self {
        Self {
            default_ms: default_timeout_ms(),
            max_ms: default_max_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestTool {
    pub name: String,
    #[serde(default)]
    pub kind: ToolKindDecl,
    #[serde(default)]
    pub target_field: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub timeout: ToolTimeout,
    #[serde(default)]
    pub safety: ToolSafetyDecl,
    #[serde(default)]
    pub resources: Vec<String>,
    #[serde(default = "default_true")]
    pub cancellable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NumericRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StateFieldDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub range: Option<NumericRange>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceDecl {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub fields: Vec<String>,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_min_interval_ms")]
    pub min_interval_ms: u64,
    #[serde(default)]
    pub demand_only: bool,
    #[serde(default)]
    pub health: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResourceMode {
    #[default]
    Exclusive,
    Shared,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDecl {
    pub id: String,
    #[serde(default)]
    pub mode: ResourceMode,
    #[serde(default = "default_capacity")]
    pub capacity: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PlacementMode {
    Fixed,
    #[default]
    Runtime,
    Declared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum LocationTrustDecl {
    #[default]
    Trusted,
    Uncertain,
    Lost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlacementDecl {
    #[serde(default)]
    pub mode: PlacementMode,
    #[serde(default)]
    pub from_source: Option<String>,
    #[serde(default)]
    pub map_ref: Option<String>,
    #[serde(default)]
    pub zone_ref: Option<String>,
    #[serde(default)]
    pub position: Option<serde_json::Value>,
    #[serde(default)]
    pub trust: LocationTrustDecl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManifestLimits {
    #[serde(default = "default_concurrent")]
    pub max_concurrent_jobs: u32,
    #[serde(default = "default_calls_per_min")]
    pub max_calls_per_min: u32,
}

impl Default for ManifestLimits {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: default_concurrent(),
            max_calls_per_min: default_calls_per_min(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest version '{0}' must be '<major>.<minor>'")]
    InvalidVersion(String),
    #[error("unsupported manifest major version {0} (hub supports {MANIFEST_MAJOR})")]
    UnsupportedMajor(u32),
    #[error("unsupported ABI version {0} (requires {DRIVER_ABI_V2})")]
    UnsupportedAbi(u32),
    #[error("identity.driver must not be empty")]
    EmptyDriver,
    #[error("identity.version '{0}' is invalid (expected 1-64 chars from [A-Za-z0-9._+-], not '.' or '..')")]
    InvalidDriverVersion(String),
    #[error("set tool '{tool}' schema does not match its target field: {detail}")]
    SetToolSchemaMismatch { tool: String, detail: String },
    #[error("duplicate {kind}: {name}")]
    Duplicate { kind: &'static str, name: String },
    #[error("tool '{0}' parameters must be an object schema")]
    InvalidToolSchema(String),
    #[error("set tool '{tool}' must declare targetField")]
    MissingTargetField { tool: String },
    #[error("set tool '{tool}' references unknown target field '{field}'")]
    UnknownTargetField { tool: String, field: String },
    #[error("tool '{tool}' references unknown resource '{resource}'")]
    UnknownResource { tool: String, resource: String },
    #[error("source '{source_id}' references unknown state field '{field}'")]
    UnknownSourceField { source_id: String, field: String },
    #[error("limits must be greater than zero")]
    InvalidLimits,
    #[error("declaration identity mismatch: expected {expected}, got {got}")]
    IdentityMismatch { expected: String, got: String },
    #[error("declaration manifest version mismatch: expected {expected}, got {got}")]
    ManifestVersionMismatch { expected: String, got: String },
    #[error("declaration must match the fixed package declaration")]
    FixedDeclarationMismatch,
    #[error("too many {kind}: {count} (max {max})")]
    TooMany {
        kind: &'static str,
        count: usize,
        max: usize,
    },
}

pub fn negotiate_manifest_version(v: &str) -> Result<(u32, u32), ManifestError> {
    let (major, minor) = v
        .split_once('.')
        .ok_or_else(|| ManifestError::InvalidVersion(v.to_string()))?;
    let major: u32 = major
        .parse()
        .map_err(|_| ManifestError::InvalidVersion(v.to_string()))?;
    let minor: u32 = minor
        .parse()
        .map_err(|_| ManifestError::InvalidVersion(v.to_string()))?;
    if major != MANIFEST_MAJOR {
        return Err(ManifestError::UnsupportedMajor(major));
    }
    Ok((major, minor))
}

/// Validate `identity.version` as a path-safe, printable version token.
///
/// The version becomes a directory name under the workspace package store,
/// so path separators and `.`/`..` must never be accepted.
pub fn validate_driver_version(version: &str) -> Result<(), ManifestError> {
    let valid = !version.is_empty()
        && version.len() <= 64
        && version != "."
        && version != ".."
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(ManifestError::InvalidDriverVersion(version.to_string()))
    }
}

/// Enforce the spec §2.1 `set` tool contract: `parameters` declares exactly
/// the `targetField`, with the same type, range and required-ness as the
/// corresponding state field.
pub(crate) fn validate_set_tool_schema(t: &ManifestTool, field: &StateFieldDecl) -> Result<(), ManifestError> {
    let mismatch = |detail: String| ManifestError::SetToolSchemaMismatch {
        tool: t.name.clone(),
        detail,
    };
    let obj = t
        .parameters
        .as_object()
        .ok_or_else(|| ManifestError::InvalidToolSchema(t.name.clone()))?;
    let props = obj
        .get("properties")
        .and_then(|v| v.as_object())
        .ok_or_else(|| mismatch("parameters.properties must be an object".into()))?;
    if props.len() != 1 || !props.contains_key(&field.name) {
        return Err(mismatch(format!(
            "parameters.properties must contain exactly '{}'",
            field.name
        )));
    }
    let prop = props
        .get(&field.name)
        .and_then(|v| v.as_object())
        .ok_or_else(|| mismatch(format!("parameter '{}' must be an object schema", field.name)))?;
    let data_type = prop
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| mismatch(format!("parameter '{}' must declare a type", field.name)))?;
    if data_type != field.data_type {
        return Err(mismatch(format!(
            "parameter type '{}' does not match state field type '{}'",
            data_type, field.data_type
        )));
    }

    let expected_min = field.range.as_ref().and_then(|r| r.min);
    let expected_max = field.range.as_ref().and_then(|r| r.max);
    let bound = |key: &str| -> Result<Option<f64>, ManifestError> {
        match prop.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| mismatch(format!("{key} must be a number"))),
        }
    };
    let (actual_min, actual_max) = (bound("minimum")?, bound("maximum")?);
    if actual_min != expected_min || actual_max != expected_max {
        return Err(mismatch(format!(
            "parameter range [{actual_min:?}, {actual_max:?}] does not match state field range [{expected_min:?}, {expected_max:?}]"
        )));
    }

    let required = obj
        .get("required")
        .and_then(|v| v.as_array())
        .is_some_and(|r| r.iter().any(|v| v.as_str() == Some(field.name.as_str())));
    if !required {
        return Err(mismatch(format!("required must include '{}'", field.name)));
    }
    Ok(())
}

pub fn validate_manifest(m: &DriverManifest) -> Result<(), ManifestError> {
    negotiate_manifest_version(&m.manifest_version)?;
    if m.identity.abi_version != DRIVER_ABI_V2 {
        return Err(ManifestError::UnsupportedAbi(m.identity.abi_version));
    }
    if m.identity.driver.trim().is_empty() {
        return Err(ManifestError::EmptyDriver);
    }
    validate_driver_version(&m.identity.version)?;
    if m.limits.max_concurrent_jobs == 0 || m.limits.max_calls_per_min == 0 {
        return Err(ManifestError::InvalidLimits);
    }

    let mut names = HashSet::new();
    for f in &m.state_fields {
        if !names.insert(f.name.clone()) {
            return Err(ManifestError::Duplicate {
                kind: "state field",
                name: f.name.clone(),
            });
        }
    }
    let mut tool_names = HashSet::new();
    let resource_ids: HashSet<&str> = m.resources.iter().map(|r| r.id.as_str()).collect();
    for t in &m.tools {
        if !tool_names.insert(t.name.clone()) {
            return Err(ManifestError::Duplicate {
                kind: "tool",
                name: t.name.clone(),
            });
        }
        if !t.parameters.is_object() {
            return Err(ManifestError::InvalidToolSchema(t.name.clone()));
        }
        for r in &t.resources {
            if !resource_ids.contains(r.as_str()) {
                return Err(ManifestError::UnknownResource {
                    tool: t.name.clone(),
                    resource: r.clone(),
                });
            }
        }
        if t.kind == ToolKindDecl::Set {
            let field = t
                .target_field
                .as_ref()
                .ok_or_else(|| ManifestError::MissingTargetField { tool: t.name.clone() })?;
            let decl =
                m.state_fields
                    .iter()
                    .find(|f| &f.name == field)
                    .ok_or_else(|| ManifestError::UnknownTargetField {
                        tool: t.name.clone(),
                        field: field.clone(),
                    })?;
            if decl.read_only {
                return Err(ManifestError::UnknownTargetField {
                    tool: t.name.clone(),
                    field: field.clone(),
                });
            }
            validate_set_tool_schema(t, decl)?;
        }
    }
    for s in &m.sources {
        for f in &s.fields {
            if !names.contains(f) {
                return Err(ManifestError::UnknownSourceField {
                    source_id: s.id.clone(),
                    field: f.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_manifest_json() -> serde_json::Value {
        json!({
            "manifestVersion": "1.0",
            "identity": {
                "driver": "demo", "version": "1.0.0", "protocol": "demo",
                "displayName": "Demo", "abiVersion": 2, "dynamicCapabilities": false
            },
            "config": { "type": "object", "additionalProperties": true },
            "tools": [{
                "name": "set_level", "kind": "set", "targetField": "level",
                "parameters": {
                    "type": "object",
                    "properties": { "level": { "type": "integer", "minimum": 0, "maximum": 100 } },
                    "required": ["level"], "additionalProperties": false
                },
                "timeout": { "defaultMs": 3000, "maxMs": 5000 },
                "safety": "control", "resources": ["actuator"], "cancellable": true
            }],
            "stateFields": [{
                "name": "level", "type": "integer", "unit": "%",
                "range": { "min": 0, "max": 100 }, "readOnly": false, "required": true,
                "source": "telemetry"
            }],
            "sources": [{
                "id": "telemetry", "kind": "poll", "fields": ["level"],
                "intervalMs": 1000, "minIntervalMs": 200, "demandOnly": false, "health": true
            }],
            "resources": [{ "id": "actuator", "mode": "exclusive", "capacity": 1 }],
            "placement": { "mode": "runtime", "fromSource": "position", "trust": "trusted" },
            "limits": { "maxConcurrentJobs": 1, "maxCallsPerMin": 60 }
        })
    }

    #[test]
    fn parses_valid_manifest() {
        let m: DriverManifest = serde_json::from_value(valid_manifest_json()).unwrap();
        validate_manifest(&m).unwrap();
        assert_eq!(m.tools[0].target_field.as_deref(), Some("level"));
    }

    #[test]
    fn rejects_unsupported_major_version() {
        assert!(matches!(
            negotiate_manifest_version("2.0"),
            Err(ManifestError::UnsupportedMajor(2))
        ));
        assert!(negotiate_manifest_version("1.7").is_ok());
        assert!(negotiate_manifest_version("bad").is_err());
    }

    #[test]
    fn unknown_top_level_fields_are_ignored() {
        let mut v = valid_manifest_json();
        v["futureField"] = json!({"anything": true});
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        validate_manifest(&m).unwrap();
    }

    #[test]
    fn rejects_unknown_resource_reference() {
        let mut v = valid_manifest_json();
        v["tools"][0]["resources"] = json!(["missing"]);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::UnknownResource { .. })
        ));
    }

    #[test]
    fn rejects_set_tool_whose_target_field_does_not_exist() {
        let mut v = valid_manifest_json();
        v["tools"][0]["targetField"] = json!("nope");
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::UnknownTargetField { .. })
        ));
    }

    #[test]
    fn rejects_invalid_identity_version_tokens() {
        for bad in ["", ".", "..", "../x", "1/2", &"1".repeat(65)] {
            let mut v = valid_manifest_json();
            v["identity"]["version"] = json!(bad);
            let m: DriverManifest = serde_json::from_value(v).unwrap();
            assert!(
                matches!(validate_manifest(&m), Err(ManifestError::InvalidDriverVersion(_))),
                "version {bad:?} must be rejected"
            );
        }
        assert!(validate_driver_version("1.0.0+build_2-a").is_ok());
    }

    #[test]
    fn rejects_set_tool_with_extra_parameter() {
        let mut v = valid_manifest_json();
        v["tools"][0]["parameters"]["properties"]["extra"] = json!({"type": "string"});
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::SetToolSchemaMismatch { .. })
        ));
    }

    #[test]
    fn rejects_set_tool_with_mismatched_type() {
        let mut v = valid_manifest_json();
        v["tools"][0]["parameters"]["properties"]["level"]["type"] = json!("string");
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::SetToolSchemaMismatch { .. })
        ));
    }

    #[test]
    fn rejects_set_tool_with_mismatched_range() {
        let mut v = valid_manifest_json();
        v["tools"][0]["parameters"]["properties"]["level"]["maximum"] = json!(200);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::SetToolSchemaMismatch { .. })
        ));
    }

    #[test]
    fn rejects_set_tool_missing_required_field() {
        let mut v = valid_manifest_json();
        v["tools"][0]["parameters"]["required"] = json!([]);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::SetToolSchemaMismatch { .. })
        ));
    }

    #[test]
    fn rejects_source_field_without_state_field() {
        let mut v = valid_manifest_json();
        v["sources"][0]["fields"] = json!(["missing"]);
        let m: DriverManifest = serde_json::from_value(v).unwrap();
        assert!(matches!(
            validate_manifest(&m),
            Err(ManifestError::UnknownSourceField { .. })
        ));
    }

    #[test]
    fn schema_file_matches_types() {
        let schema = schemars::schema_for!(DriverManifest);
        let generated = serde_json::to_string_pretty(&schema).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/driver-manifest.schema.json");
        let checked_in = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            checked_in.trim(),
            generated.trim(),
            "run `cargo test -p core generate_manifest_schema -- --ignored` and commit the file"
        );
    }

    #[test]
    #[ignore]
    fn generate_manifest_schema() {
        let schema = schemars::schema_for!(DriverManifest);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/driver-manifest.schema.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut json = serde_json::to_string_pretty(&schema).unwrap();
        json.push('\n');
        std::fs::write(path, json).unwrap();
    }
}
