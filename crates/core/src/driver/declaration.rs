//! Device-level declaration (spec §0 #3): package manifest + describe().

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::manifest::{
    DriverManifest, MAX_INSTANCE_STATE_FIELDS, MAX_INSTANCE_TOOLS, ManifestError, ManifestTool, StateFieldDecl,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDeclaration {
    pub manifest_version: String,
    pub driver: String,
    pub driver_version: String,
    #[serde(default)]
    pub state_fields: Vec<StateFieldDecl>,
    #[serde(default)]
    pub tools: Vec<ManifestTool>,
}

pub fn fixed_declaration(m: &DriverManifest) -> DeviceDeclaration {
    DeviceDeclaration {
        manifest_version: m.manifest_version.clone(),
        driver: m.identity.driver.clone(),
        driver_version: m.identity.version.clone(),
        state_fields: m.state_fields.clone(),
        tools: m.tools.clone(),
    }
}

pub fn validate_device_declaration(package: &DriverManifest, decl: &DeviceDeclaration) -> Result<(), ManifestError> {
    if decl.driver != package.identity.driver || decl.driver_version != package.identity.version {
        return Err(ManifestError::IdentityMismatch {
            expected: format!("{}@{}", package.identity.driver, package.identity.version),
            got: format!("{}@{}", decl.driver, decl.driver_version),
        });
    }
    if decl.state_fields.len() > MAX_INSTANCE_STATE_FIELDS {
        return Err(ManifestError::TooMany {
            kind: "state fields",
            count: decl.state_fields.len(),
            max: MAX_INSTANCE_STATE_FIELDS,
        });
    }
    if decl.tools.len() > MAX_INSTANCE_TOOLS {
        return Err(ManifestError::TooMany {
            kind: "tools",
            count: decl.tools.len(),
            max: MAX_INSTANCE_TOOLS,
        });
    }

    let mut fields = HashSet::new();
    for f in &decl.state_fields {
        if !fields.insert(f.name.as_str()) {
            return Err(ManifestError::Duplicate {
                kind: "state field",
                name: f.name.clone(),
            });
        }
    }
    let mut tool_names = HashSet::new();
    for t in &decl.tools {
        if !tool_names.insert(t.name.as_str()) {
            return Err(ManifestError::Duplicate {
                kind: "tool",
                name: t.name.clone(),
            });
        }
        if !t.parameters.is_object() {
            return Err(ManifestError::InvalidToolSchema(t.name.clone()));
        }
        if t.kind == super::manifest::ToolKindDecl::Set {
            let field = t
                .target_field
                .as_ref()
                .ok_or_else(|| ManifestError::MissingTargetField { tool: t.name.clone() })?;
            let f = decl.state_fields.iter().find(|f| &f.name == field).ok_or_else(|| {
                ManifestError::UnknownTargetField {
                    tool: t.name.clone(),
                    field: field.clone(),
                }
            })?;
            if f.read_only {
                return Err(ManifestError::UnknownTargetField {
                    tool: t.name.clone(),
                    field: field.clone(),
                });
            }
        }
    }
    for s in &package.sources {
        for f in &s.fields {
            if !fields.contains(f.as_str()) {
                return Err(ManifestError::UnknownSourceField {
                    source_id: s.id.clone(),
                    field: f.clone(),
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DriverDeclarationRecord {
    pub workspace_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub manifest_version: String,
    pub manifest_json: String,
    pub manifest_hash: String,
    pub binary_hash: Option<String>,
    pub status: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceDeclarationRecord {
    pub workspace_id: String,
    pub thing_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub declaration_json: String,
    pub declaration_hash: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceReportRecord {
    pub workspace_id: String,
    pub driver_name: String,
    pub driver_version: String,
    pub profile: String,
    pub level: String,
    pub report_json: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::manifest::*;
    use serde_json::json;

    fn package(dynamic: bool) -> DriverManifest {
        serde_json::from_value(json!({
            "manifestVersion":"1.0",
            "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":dynamic},
            "config":{"type":"object"},"tools":[],"stateFields":[],
            "sources":[{"id":"telemetry","kind":"poll","fields":[],"intervalMs":1000,"minIntervalMs":200}],
            "resources":[],"placement":{"mode":"runtime"},
            "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
        })).unwrap()
    }

    fn decl() -> DeviceDeclaration {
        DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(),
                data_type: "integer".into(),
                unit: Some("%".into()),
                range: Some(NumericRange {
                    min: Some(0.0),
                    max: Some(100.0),
                }),
                read_only: false,
                required: true,
                source: Some("telemetry".into()),
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(),
                kind: ToolKindDecl::Set,
                target_field: Some("level".into()),
                parameters: json!({"type":"object","properties":{"level":{"type":"integer"}},"required":["level"]}),
                timeout: ToolTimeout::default(),
                safety: ToolSafetyDecl::Control,
                resources: vec![],
                cancellable: true,
            }],
        }
    }

    #[test]
    fn fixed_declaration_copies_package_capabilities() {
        let mut p = package(false);
        p.state_fields = decl().state_fields;
        p.tools = decl().tools;
        let d = fixed_declaration(&p);
        assert_eq!(d.state_fields[0].name, "level");
        validate_device_declaration(&p, &d).unwrap();
    }

    #[test]
    fn dynamic_declaration_may_add_fields_and_tools() {
        validate_device_declaration(&package(true), &decl()).unwrap();
    }

    #[test]
    fn rejects_identity_mismatch() {
        let p = package(true);
        let mut d = decl();
        d.driver_version = "9.9.9".into();
        assert!(matches!(
            validate_device_declaration(&p, &d),
            Err(ManifestError::IdentityMismatch { .. })
        ));
    }

    #[test]
    fn rejects_tool_limit_overflow() {
        let p = package(true);
        let mut d = decl();
        d.tools = (0..=MAX_INSTANCE_TOOLS)
            .map(|i| ManifestTool {
                name: format!("t{i}"),
                ..d.tools[0].clone()
            })
            .collect();
        assert!(matches!(
            validate_device_declaration(&p, &d),
            Err(ManifestError::TooMany { .. })
        ));
    }
}
