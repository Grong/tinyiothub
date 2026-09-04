// Thing template import/export — DTDL and WoT Thing Description support.
//
// Supports:
//   - Import DTDL JSON → thing_templates row
//   - Export thing_templates row → DTDL JSON (format_version: 2)
//   - Import WoT Thing Description JSON → thing_templates row
//   - Backwards compat: accept "commands" key and map to "actions"

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_storage::Db;
use tinyiothub_storage::scene_template::SceneTemplateFile;
use tinyiothub_storage::thing_template::SceneTemplateInsert;

pub use tinyiothub_storage::thing_template::{ParsedTemplate, ThingTemplateRow};

// ──────────────────────────────────────────────
// Error
// ──────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Unsupported DTDL @type: {0}")]
    UnsupportedType(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Template not found: {0}")]
    NotFound(String),

    #[error("Name conflict in workspace: {0}")]
    NameConflict(String),
}

impl From<serde_json::Error> for ImportError {
    fn from(e: serde_json::Error) -> Self {
        ImportError::InvalidJson(e.to_string())
    }
}

// ──────────────────────────────────────────────
// DB row (subset of thing_templates)
// ──────────────────────────────────────────────

// ──────────────────────────────────────────────
// Internal schema types
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PropertySchema {
    pub name: String,
    #[allow(dead_code)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub data_type: Option<String>,
    pub writable: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct ActionSchema {
    pub name: String,
    #[allow(dead_code)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub request: Option<Value>,
    pub response: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct EventSchema {
    pub name: String,
    #[allow(dead_code)]
    pub display_name: Option<String>,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub data_type: Option<String>,
}

// ──────────────────────────────────────────────
// Scene pack import（场景包注册闭环）
// ──────────────────────────────────────────────

/// 判定 import JSON 是否场景包：根级含非空 children 数组。
/// entity 模板（DTDL/WoT）无此键，走现有 ParsedTemplate 路径不变。
pub fn is_scene_template_json(json: &Value) -> bool {
    json.get("children")
        .and_then(Value::as_array)
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

/// 场景包 import 结果。
#[derive(Debug)]
pub struct SceneImportOutcome {
    pub id: String,
    /// 冲突重命名后的最终模板名
    pub name: String,
    pub thing_type: String,
}

/// 场景包旁路：`SceneTemplateFile` 校验 → device_info 存原文 → 注册为 workspace 组合模板。
/// 名称冲突沿用现有模板重命名策略（marketplace install 同款：后缀探测取第一个空闲名）。
pub async fn import_scene_template(
    pool: &SqlitePool,
    body: &Value,
    workspace_id: Option<&str>,
) -> Result<SceneImportOutcome, ImportError> {
    let raw = serde_json::to_string(body)?;
    let scene = SceneTemplateFile::from_json(&raw)?;

    if scene.name.trim().is_empty() {
        return Err(ImportError::MissingField("name".to_string()));
    }
    if scene.children.is_empty() {
        return Err(ImportError::MissingField("children".to_string()));
    }

    let db = Db::new(pool.clone());
    let ws_key = workspace_id.unwrap_or("");

    // 附录 A 缺省映射：building→building，其余空间→space
    let thing_type = if scene.thing_category.as_deref() == Some("building") {
        "building"
    } else {
        "space"
    };
    let category = if scene.category.is_empty() {
        "scenes".to_string()
    } else {
        scene.category.clone()
    };

    // 名称候选 + 竞态兜底：探测（SELECT）与 INSERT 之间被并发抢占时
    // （idx_thing_templates_name_workspace 唯一冲突）继续尝试下一个后缀，
    // 而不是 500；探测查询本身出错则错误透传（不当作"名称被占"）
    for candidate in import_name_candidates(&scene.name) {
        if !name_is_free(&db, ws_key, &candidate).await? {
            continue;
        }
        let insert = SceneTemplateInsert {
            name: candidate.clone(),
            display_name: serde_json::to_string(&scene.display_name)?,
            description: scene.description.as_ref().map(serde_json::to_string).transpose()?,
            version: scene.version.clone(),
            category: category.clone(),
            thing_type: thing_type.to_string(),
            device_info: raw.clone(),
            workspace_id: workspace_id.map(|s| s.to_string()),
        };
        match db.insert_scene_thing_template(&insert).await {
            Ok(id) => {
                return Ok(SceneImportOutcome {
                    id,
                    name: candidate,
                    thing_type: thing_type.to_string(),
                });
            }
            Err(e) if is_template_name_unique_violation(&e) => continue, // 竞态被抢占，试下一个后缀
            Err(e) => return Err(ImportError::Database(e)),
        }
    }
    Err(ImportError::NameConflict(format!(
        "Unable to resolve name conflict for '{}' in workspace",
        scene.name
    )))
}

/// thing_templates 名称索引唯一冲突判定（竞态重试信号）。
fn is_template_name_unique_violation(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(dbe) if dbe.is_unique_violation() => {
            let msg = dbe.message();
            msg.contains("idx_thing_templates_name_workspace") || msg.contains("thing_templates.name")
        }
        _ => false,
    }
}

/// 名称候选序列：原名 → "{name} (import)" → "{name} (import {N})"（N=2..100）。
fn import_name_candidates(name: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(100);
    out.push(name.to_string());
    out.push(format!("{name} (import)"));
    for i in 2..100 {
        out.push(format!("{name} (import {i})"));
    }
    out
}

async fn name_is_free(db: &Db, workspace_key: &str, name: &str) -> Result<bool, ImportError> {
    // 探测查询出错透传（不当作"名称被占"安全侧处理——那会掩盖 DB 故障并把
    // 用户导向无意义的后缀重命名）
    let count = db.count_thing_template_name_conflicts(workspace_key, name).await?;
    Ok(count == 0)
}

// ──────────────────────────────────────────────
// DTDL Import
// ──────────────────────────────────────────────

/// Parse DTDL JSON into our template fields.
///
/// Mapping:
///   displayName  → name
///   description  → description
///   @type        → thing_type ("Interface" → "device")
///   contents[].@type="Property"  → properties
///   contents[].@type="Telemetry" → events
///   contents[].@type="Command"   → actions
pub fn parse_dtdl(json: &Value) -> Result<ParsedTemplate, ImportError> {
    // Validate @type
    let dtdl_type = json["@type"].as_str().unwrap_or("Interface");

    if dtdl_type != "Interface" {
        return Err(ImportError::UnsupportedType(dtdl_type.to_string()));
    }

    let name = json["displayName"].as_str().unwrap_or("Untitled").to_string();

    let description = json["description"].as_str().map(|s| s.to_string());

    let thing_type = "device".to_string(); // DTDL interfaces are things

    let mut properties: Vec<Value> = Vec::new();
    let mut actions: Vec<Value> = Vec::new();
    let mut events: Vec<Value> = Vec::new();

    if let Some(contents) = json["contents"].as_array() {
        for item in contents {
            let content_type = item["@type"].as_str().unwrap_or("");

            match content_type {
                "Property" => {
                    let schema = resolve_schema_value(&item["schema"]);
                    properties.push(json!({
                        "name": item["name"].as_str().unwrap_or(""),
                        "displayName": item["displayName"],
                        "description": item["description"],
                        "schema": schema,
                        "writable": item["writable"].as_bool().unwrap_or(false),
                    }));
                }
                "Telemetry" => {
                    let schema = resolve_schema_value(&item["schema"]);
                    events.push(json!({
                        "name": item["name"].as_str().unwrap_or(""),
                        "displayName": item["displayName"],
                        "description": item["description"],
                        "schema": schema,
                    }));
                }
                "Command" => {
                    let mut cmd = json!({
                        "name": item["name"].as_str().unwrap_or(""),
                        "displayName": item["displayName"],
                        "description": item["description"],
                    });

                    // Request schema
                    if let Some(req) = item["request"].as_object() {
                        let req_schema = req.get("schema").cloned();
                        if let Some(s) = req_schema {
                            cmd["request"] = json!({
                                "name": req.get("name").and_then(|v| v.as_str()).unwrap_or("input"),
                                "schema": s,
                            });
                        }
                    }

                    // Response schema
                    if let Some(resp) = item["response"].as_object() {
                        let resp_schema = resp.get("schema").cloned();
                        if let Some(s) = resp_schema {
                            cmd["response"] = json!({
                                "name": resp.get("name").and_then(|v| v.as_str()).unwrap_or("output"),
                                "schema": s,
                            });
                        }
                    }

                    actions.push(cmd);
                }
                "Relationship" | "Component" => {
                    // Skip — not directly mappable to our simple schema
                    tracing::debug!(
                        "Skipping DTDL content type '{}' (name={})",
                        content_type,
                        item["name"].as_str().unwrap_or("?")
                    );
                }
                _ => {
                    tracing::debug!("Unknown DTDL content type '{}' — skipping", content_type);
                }
            }
        }
    }

    Ok(ParsedTemplate {
        name,
        display_name: json["displayName"].as_str().unwrap_or("Untitled").to_string(),
        description,
        thing_type,
        device_type: "generic".to_string(),
        properties: serde_json::to_string(&properties)?,
        actions: serde_json::to_string(&actions)?,
        events: serde_json::to_string(&events)?,
    })
}

/// Resolve a DTDL schema field to a simple string or keep full object.
fn resolve_schema_value(schema: &Value) -> Value {
    match schema {
        Value::String(s) => json!(s),
        Value::Object(obj) => {
            // If it's a complex schema with @type, try to extract the primitive
            if let Some(schema_type) = obj.get("@type").and_then(|v| v.as_str()) {
                match schema_type {
                    "Enum" => {
                        // Return enum with enumValues
                        json!({
                            "@type": "Enum",
                            "valueSchema": obj.get("valueSchema").unwrap_or(&json!("string")),
                            "enumValues": obj.get("enumValues").unwrap_or(&json!([])),
                        })
                    }
                    "Object" | "Map" | "Array" => schema.clone(),
                    _ => json!(schema_type),
                }
            } else {
                schema.clone()
            }
        }
        _ => schema.clone(),
    }
}

// ──────────────────────────────────────────────
// WoT Thing Description Import
// ──────────────────────────────────────────────

/// Parse WoT Thing Description JSON into our template fields.
///
/// Mapping:
///   title       → name
///   description → description
///   properties  → our property schema
///   actions     → our action schema
///   events      → our event schema
pub fn parse_wot_td(json: &Value) -> Result<ParsedTemplate, ImportError> {
    let name = json["title"]
        .as_str()
        .or_else(|| json["id"].as_str())
        .unwrap_or("Untitled")
        .to_string();

    let description = json["description"].as_str().map(|s| s.to_string());
    let thing_type = "device".to_string();

    let mut properties: Vec<Value> = Vec::new();
    if let Some(props_obj) = json["properties"].as_object() {
        for (prop_name, prop_def) in props_obj {
            properties.push(json!({
                "name": prop_name,
                "displayName": prop_def["title"],
                "description": prop_def["description"],
                "schema": resolve_wot_schema(prop_def),
                "writable": !prop_def["readOnly"].as_bool().unwrap_or(false),
            }));
        }
    }

    let mut actions: Vec<Value> = Vec::new();
    if let Some(actions_obj) = json["actions"].as_object() {
        for (action_name, action_def) in actions_obj {
            let mut cmd = json!({
                "name": action_name,
                "displayName": action_def["title"],
                "description": action_def["description"],
            });

            if let Some(input) = action_def.get("input") {
                cmd["request"] = json!({
                    "name": "input",
                    "schema": resolve_wot_schema(input),
                });
            }
            if let Some(output) = action_def.get("output") {
                cmd["response"] = json!({
                    "name": "output",
                    "schema": resolve_wot_schema(output),
                });
            }

            actions.push(cmd);
        }
    }

    let mut events: Vec<Value> = Vec::new();
    if let Some(events_obj) = json["events"].as_object() {
        for (event_name, event_def) in events_obj {
            let mut event = json!({
                "name": event_name,
                "displayName": event_def["title"],
                "description": event_def["description"],
            });

            if let Some(data) = event_def.get("data") {
                event["schema"] = resolve_wot_schema(data);
            }

            events.push(event);
        }
    }

    Ok(ParsedTemplate {
        name: name.clone(),
        display_name: name,
        description,
        thing_type,
        device_type: "generic".to_string(),
        properties: serde_json::to_string(&properties)?,
        actions: serde_json::to_string(&actions)?,
        events: serde_json::to_string(&events)?,
    })
}

/// Resolve WoT data schema to a simplified representation.
fn resolve_wot_schema(value: &Value) -> Value {
    match value.get("type") {
        Some(t) => t.clone(),
        None => {
            if let Some(props) = value.get("properties") {
                // Object schema
                json!({ "type": "object", "properties": props })
            } else if value.get("items").is_some() {
                // Array schema
                json!({ "type": "array", "items": value["items"] })
            } else {
                json!("string")
            }
        }
    }
}

// ──────────────────────────────────────────────
// Backwards compat: "commands" → "actions"
// ──────────────────────────────────────────────

// ──────────────────────────────────────────────
// DTDL Export
// ──────────────────────────────────────────────

/// Convert a ThingTemplateRow to DTDL JSON.
pub fn export_to_dtdl(row: &ThingTemplateRow) -> Result<Value, ImportError> {
    let mut contents: Vec<Value> = Vec::new();

    // Properties → DTDL Property
    let properties: Vec<Value> = serde_json::from_str(&row.properties).unwrap_or_default();
    for prop in &properties {
        let schema = prop.get("schema").cloned().unwrap_or_else(|| json!("string"));

        contents.push(json!({
            "@type": "Property",
            "name": prop["name"],
            "displayName": prop.get("displayName"),
            "description": prop.get("description"),
            "schema": dtdl_primitive_schema(&schema),
            "writable": prop.get("writable").and_then(|v| v.as_bool()).unwrap_or(false),
        }));
    }

    // Events → DTDL Telemetry
    let events: Vec<Value> = serde_json::from_str(&row.events).unwrap_or_default();
    for event in &events {
        let schema = event.get("schema").cloned().unwrap_or_else(|| json!("string"));

        contents.push(json!({
            "@type": "Telemetry",
            "name": event["name"],
            "displayName": event.get("displayName"),
            "description": event.get("description"),
            "schema": dtdl_primitive_schema(&schema),
        }));
    }

    // Actions → DTDL Command
    let actions: Vec<Value> = serde_json::from_str(&row.actions).unwrap_or_default();
    for action in &actions {
        let mut cmd = json!({
            "@type": "Command",
            "name": action["name"],
            "displayName": action.get("displayName"),
            "description": action.get("description"),
        });

        if let Some(req) = action.get("request") {
            let req_schema = req.get("schema").cloned().unwrap_or_else(|| json!("string"));
            cmd["request"] = json!({
                "name": req.get("name").and_then(Value::as_str).unwrap_or("input"),
                "schema": dtdl_primitive_schema(&req_schema),
            });
        }

        if let Some(resp) = action.get("response") {
            let resp_schema = resp.get("schema").cloned().unwrap_or_else(|| json!("object"));
            cmd["response"] = json!({
                "name": resp.get("name").and_then(Value::as_str).unwrap_or("output"),
                "schema": dtdl_primitive_schema(&resp_schema),
            });
        }

        contents.push(cmd);
    }

    Ok(json!({
        "@context": "dtmi:dtdl:context;2",
        "@id": format!("dtmi:tinyiothub:{};1", row.id),
        "@type": "Interface",
        "displayName": row.name,
        "description": row.description,
        "format_version": 2,
        "contents": contents,
    }))
}

/// Map a schema value to a DTDL primitive when possible.
fn dtdl_primitive_schema(schema: &Value) -> Value {
    match schema {
        Value::String(s) => {
            match s.as_str() {
                "boolean" => json!("boolean"),
                "double" | "float" | "number" => json!("double"),
                "integer" | "int" | "long" | "int64" | "int32" => json!("integer"),
                "string" | "str" | "text" => json!("string"),
                "dateTime" | "datetime" | "date-time" => json!("dateTime"),
                "duration" => json!("duration"),
                "time" => json!("time"),
                "date" => json!("date"),
                _ => json!(s), // Unknown — passthrough
            }
        }
        Value::Object(obj) => {
            // If it's already a DTDL schema object, keep it
            if obj.contains_key("@type") {
                schema.clone()
            } else if let Some(t) = obj.get("type").and_then(|v| v.as_str()) {
                // WoT-style type → DTDL primitive
                match t {
                    "boolean" => json!("boolean"),
                    "number" | "float" => json!("double"),
                    "integer" => json!("integer"),
                    "string" => json!("string"),
                    _ => json!(t),
                }
            } else {
                schema.clone()
            }
        }
        _ => schema.clone(),
    }
}

// ──────────────────────────────────────────────
// ParsedTemplate (internal representation)
// ──────────────────────────────────────────────

// ──────────────────────────────────────────────
// Database operations
// ──────────────────────────────────────────────

/// Insert a parsed template into the thing_templates table.
pub async fn save_template(
    pool: &SqlitePool,
    template: &ParsedTemplate,
    workspace_id: Option<&str>,
) -> Result<String, ImportError> {
    // Check name conflict
    let ws_key = workspace_id.unwrap_or("");
    let db = Db::new(pool.clone());
    let existing = db.count_thing_template_name_conflicts(ws_key, &template.name).await?;

    if existing > 0 {
        return Err(ImportError::NameConflict(format!(
            "Template name '{}' already exists in workspace",
            template.name
        )));
    }

    db.insert_parsed_thing_template(template, workspace_id, "{}")
        .await
        .map_err(ImportError::from)
}

/// Load a template from thing_templates by ID.
pub async fn load_template(pool: &SqlitePool, id: &str) -> Result<ThingTemplateRow, ImportError> {
    Db::new(pool.clone())
        .find_thing_template_row(id)
        .await?
        .ok_or_else(|| ImportError::NotFound(id.to_string()))
}

// ──────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Scene pack detection ──

    #[test]
    fn test_is_scene_template_json() {
        assert!(is_scene_template_json(
            &json!({"name": "x", "children": [{"key": "room"}]})
        ));
        // 空 children 不算组合模板
        assert!(!is_scene_template_json(&json!({"name": "x", "children": []})));
        // 无 children 键 → entity 路径
        assert!(!is_scene_template_json(&json!({"name": "x"})));
        // children 非数组 → entity 路径
        assert!(!is_scene_template_json(&json!({"children": "not-array"})));
    }

    // ── DTDL import helpers ──

    fn sample_dtdl_json() -> Value {
        json!({
            "@context": "dtmi:dtdl:context;2",
            "@id": "dtmi:com:example:Thermostat;1",
            "@type": "Interface",
            "displayName": "Thermostat",
            "description": "A thermostat device model",
            "contents": [
                {
                    "@type": "Property",
                    "name": "targetTemperature",
                    "displayName": "Target Temperature",
                    "description": "Desired temperature setpoint",
                    "schema": "double",
                    "writable": true
                },
                {
                    "@type": "Property",
                    "name": "mode",
                    "schema": {
                        "@type": "Enum",
                        "valueSchema": "integer",
                        "enumValues": [
                            {"name": "off", "displayName": "Off", "enumValue": 0},
                            {"name": "heat", "displayName": "Heat", "enumValue": 1},
                            {"name": "cool", "displayName": "Cool", "enumValue": 2}
                        ]
                    },
                    "writable": true
                },
                {
                    "@type": "Telemetry",
                    "name": "temperature",
                    "displayName": "Temperature",
                    "schema": "double"
                },
                {
                    "@type": "Command",
                    "name": "setMode",
                    "displayName": "Set Mode",
                    "request": {
                        "name": "mode",
                        "schema": "integer"
                    },
                    "response": {
                        "name": "statusCode",
                        "schema": "integer"
                    }
                }
            ]
        })
    }

    #[test]
    fn test_parse_dtdl_basic() {
        let result = parse_dtdl(&sample_dtdl_json()).unwrap();
        assert_eq!(result.name, "Thermostat");
        assert_eq!(result.thing_type, "device");

        let properties: Vec<Value> = serde_json::from_str(&result.properties).unwrap();
        assert_eq!(properties.len(), 2);
        assert_eq!(properties[0]["name"], "targetTemperature");
        assert_eq!(properties[0]["schema"], "double");
        assert!(properties[0]["writable"].as_bool().unwrap());

        let events: Vec<Value> = serde_json::from_str(&result.events).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["name"], "temperature");
        assert_eq!(events[0]["schema"], "double");

        let actions: Vec<Value> = serde_json::from_str(&result.actions).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["name"], "setMode");
    }

    #[test]
    fn test_parse_dtdl_minimal() {
        let json = json!({
            "@type": "Interface",
            "displayName": "MinimalSensor",
            "contents": [
                { "@type": "Telemetry", "name": "value", "schema": "float" }
            ]
        });
        let result = parse_dtdl(&json).unwrap();
        assert_eq!(result.name, "MinimalSensor");

        let events: Vec<Value> = serde_json::from_str(&result.events).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["name"], "value");

        let properties: Vec<Value> = serde_json::from_str(&result.properties).unwrap();
        assert!(properties.is_empty());
    }

    #[test]
    fn test_parse_dtdl_empty_contents() {
        let json = json!({
            "@type": "Interface",
            "displayName": "EmptyInterface"
        });
        let result = parse_dtdl(&json).unwrap();
        assert_eq!(result.name, "EmptyInterface");
        assert!(
            serde_json::from_str::<Vec<Value>>(&result.properties)
                .unwrap()
                .is_empty()
        );
        assert!(serde_json::from_str::<Vec<Value>>(&result.actions).unwrap().is_empty());
        assert!(serde_json::from_str::<Vec<Value>>(&result.events).unwrap().is_empty());
    }

    #[test]
    fn test_parse_dtdl_unsupported_type() {
        let json = json!({
            "@type": "Component",
            "displayName": "BadType"
        });
        let err = parse_dtdl(&json).unwrap_err();
        assert!(matches!(err, ImportError::UnsupportedType(_)));
    }

    #[test]
    fn test_parse_dtdl_no_display_name() {
        let json = json!({
            "@type": "Interface"
        });
        let result = parse_dtdl(&json).unwrap();
        assert_eq!(result.name, "Untitled");
    }

    #[test]
    fn test_parse_dtdl_schema_enum() {
        let json = json!({
            "@type": "Interface",
            "displayName": "EnumDevice",
            "contents": [
                {
                    "@type": "Property",
                    "name": "color",
                    "schema": {
                        "@type": "Enum",
                        "valueSchema": "string",
                        "enumValues": [
                            {"name": "red", "enumValue": "red"},
                            {"name": "blue", "enumValue": "blue"}
                        ]
                    }
                }
            ]
        });
        let result = parse_dtdl(&json).unwrap();
        let properties: Vec<Value> = serde_json::from_str(&result.properties).unwrap();
        let schema = &properties[0]["schema"];
        assert_eq!(schema["@type"], "Enum");
        assert_eq!(schema["valueSchema"], "string");
    }

    // ── WoT TD import ──

    fn sample_wot_json() -> Value {
        json!({
            "@context": "https://www.w3.org/2022/wot/td/v1.1",
            "title": "MyTemperatureSensor",
            "description": "A temperature sensor thing",
            "properties": {
                "temperature": {
                    "type": "number",
                    "unit": "°C",
                    "readOnly": true,
                    "description": "Current temperature"
                },
                "threshold": {
                    "type": "number",
                    "readOnly": false,
                    "description": "Alert threshold"
                }
            },
            "actions": {
                "reboot": {
                    "description": "Reboot the device",
                    "input": { "type": "object" },
                    "output": { "type": "string" }
                }
            },
            "events": {
                "overheat": {
                    "description": "Overheat alert",
                    "data": { "type": "number" }
                }
            }
        })
    }

    #[test]
    fn test_parse_wot_td_basic() {
        let result = parse_wot_td(&sample_wot_json()).unwrap();
        assert_eq!(result.name, "MyTemperatureSensor");
        assert_eq!(result.thing_type, "device");

        let properties: Vec<Value> = serde_json::from_str(&result.properties).unwrap();
        assert_eq!(properties.len(), 2);
        assert_eq!(properties[0]["name"], "temperature");
        assert_eq!(properties[0]["schema"], "number");

        let actions: Vec<Value> = serde_json::from_str(&result.actions).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["name"], "reboot");

        let events: Vec<Value> = serde_json::from_str(&result.events).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["name"], "overheat");
    }

    #[test]
    fn test_parse_wot_td_minimal() {
        let json = json!({
            "title": "MinimalSensor"
        });
        let result = parse_wot_td(&json).unwrap();
        assert_eq!(result.name, "MinimalSensor");
        assert!(
            serde_json::from_str::<Vec<Value>>(&result.properties)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn test_parse_wot_td_no_title_fallback_to_id() {
        let json = json!({
            "id": "urn:example:sensor123"
        });
        let result = parse_wot_td(&json).unwrap();
        assert_eq!(result.name, "urn:example:sensor123");
    }

    // ── DTDL Export ──

    fn make_template_row(name: &str, properties_json: &str, actions_json: &str, events_json: &str) -> ThingTemplateRow {
        ThingTemplateRow {
            id: "tmpl-test-001".to_string(),
            name: name.to_string(),
            display_name: name.to_string(),
            description: Some("A test template".to_string()),
            version: "1.0.0".to_string(),
            category: "test".to_string(),
            thing_type: "device".to_string(),
            properties: properties_json.to_string(),
            actions: actions_json.to_string(),
            events: events_json.to_string(),
            default_knowledge: None,
            workspace_id: None,
        }
    }

    #[test]
    fn test_export_to_dtdl() {
        let properties = json!([
            {"name": "temperature", "schema": "double", "writable": false}
        ])
        .to_string();
        let events = json!([
            {"name": "ambientTemp", "schema": "double"}
        ])
        .to_string();
        let actions = json!([
            {"name": "setTemp", "request": {"name": "target", "schema": "double"}}
        ])
        .to_string();

        let row = make_template_row("Thermostat", &properties, &actions, &events);
        let dtdl = export_to_dtdl(&row).unwrap();

        assert_eq!(dtdl["@context"], "dtmi:dtdl:context;2");
        assert_eq!(dtdl["@type"], "Interface");
        assert_eq!(dtdl["displayName"], "Thermostat");
        assert_eq!(dtdl["format_version"], 2);
        assert!(dtdl["@id"].as_str().unwrap().starts_with("dtmi:tinyiothub:"));

        let contents = dtdl["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);

        // Find the Property
        let prop = contents.iter().find(|c| c["@type"] == "Property").unwrap();
        assert_eq!(prop["name"], "temperature");
        assert_eq!(prop["schema"], "double");

        // Find the Telemetry
        let telem = contents.iter().find(|c| c["@type"] == "Telemetry").unwrap();
        assert_eq!(telem["name"], "ambientTemp");

        // Find the Command
        let cmd = contents.iter().find(|c| c["@type"] == "Command").unwrap();
        assert_eq!(cmd["name"], "setTemp");
        assert_eq!(cmd["request"]["name"], "target");
        assert_eq!(cmd["request"]["schema"], "double");
    }

    #[test]
    fn test_export_to_dtdl_empty_template() {
        let row = make_template_row("Empty", "[]", "[]", "[]");
        let dtdl = export_to_dtdl(&row).unwrap();
        let contents = dtdl["contents"].as_array().unwrap();
        assert!(contents.is_empty());
    }

    #[test]
    fn test_export_schema_normalization() {
        let properties = json!([
            {"name": "count", "schema": "int64"},
            {"name": "flag", "schema": "boolean"}
        ])
        .to_string();
        let row = make_template_row("SchemaTest", &properties, "[]", "[]");
        let dtdl = export_to_dtdl(&row).unwrap();
        let contents = dtdl["contents"].as_array().unwrap();
        let count_prop = contents.iter().find(|c| c["name"] == "count").unwrap();
        assert_eq!(count_prop["schema"], "integer");
        let flag_prop = contents.iter().find(|c| c["name"] == "flag").unwrap();
        assert_eq!(flag_prop["schema"], "boolean");
    }

    // ── Round-trip test ──

    #[test]
    fn test_dtdl_round_trip() {
        // Import DTDL
        let original = sample_dtdl_json();
        let parsed = parse_dtdl(&original).unwrap();

        // Convert to a DB row
        let row = ThingTemplateRow {
            id: "tmpl-roundtrip".to_string(),
            name: parsed.name.clone(),
            display_name: parsed.display_name.clone(),
            description: parsed.description.clone(),
            version: "1.0.0".to_string(),
            category: "test".to_string(),
            thing_type: parsed.thing_type.clone(),
            properties: parsed.properties.clone(),
            actions: parsed.actions.clone(),
            events: parsed.events.clone(),
            default_knowledge: None,
            workspace_id: None,
        };

        // Export to DTDL
        let exported = export_to_dtdl(&row).unwrap();

        // Re-import and verify equivalence
        let reimported = parse_dtdl(&exported).unwrap();
        assert_eq!(reimported.name, parsed.name);

        let orig_props: Vec<Value> = serde_json::from_str(&parsed.properties).unwrap();
        let reim_props: Vec<Value> = serde_json::from_str(&reimported.properties).unwrap();
        assert_eq!(orig_props.len(), reim_props.len());

        // Property names should match
        for (orig, reim) in orig_props.iter().zip(reim_props.iter()) {
            assert_eq!(orig["name"], reim["name"]);
        }
    }
}
