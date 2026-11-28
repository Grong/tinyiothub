// Validate invoke params against the action's parameter schema.
//
// Two shapes are accepted because the same `thing_actions.parameters` column is
// written by two sources:
// - legacy array: `[{"name": "interval", "type": "number", "required": true}]`
// - JSON Schema object: `{"type":"object","properties":{...},"required":[...]}`
//   (the demo seed and scene templates store this shape).
// Rules: required params present, no unknown params, primitive type match.
// Returns a Chinese error message on mismatch (design 六: 校验明细定位字段).

use serde_json::Value;

pub fn validate_action_params(schema_json: &str, params: Option<&Value>) -> Result<(), String> {
    let parsed: Value = serde_json::from_str(schema_json).map_err(|e| format!("操作参数 schema 解析失败: {}", e))?;

    if let Some(schema) = parsed.as_array() {
        return validate_legacy(schema, params);
    }
    if parsed.is_object() {
        let empty = Value::Object(Default::default());
        let provided = params.unwrap_or(&empty);
        return tinyiothub_core::driver::schema::validate_params(&parsed, provided);
    }
    Err("操作参数 schema 必须是数组或对象".to_string())
}

fn validate_legacy(schema: &[Value], params: Option<&Value>) -> Result<(), String> {
    if schema.is_empty() {
        return Ok(());
    }
    let provided = params.and_then(|p| p.as_object());

    for spec in schema {
        let name = spec.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let required = spec.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
        let expected = spec.get("type").and_then(|t| t.as_str()).unwrap_or("string");
        let value = provided.and_then(|obj| obj.get(name));
        match value {
            None if required => return Err(format!("缺少必填参数 '{}'", name)),
            None => continue,
            Some(v) => {
                let ok = match expected {
                    "string" => v.is_string(),
                    "number" | "float" | "integer" => v.is_number(),
                    "boolean" | "bool" => v.is_boolean(),
                    "object" => v.is_object(),
                    "array" => v.is_array(),
                    _ => true,
                };
                if !ok {
                    return Err(format!("参数 '{}' 类型不符: 期望 {}, 实际 {}", name, expected, v));
                }
            }
        }
    }

    if let Some(obj) = provided {
        let known: Vec<&str> = schema
            .iter()
            .filter_map(|sp| sp.get("name").and_then(|n| n.as_str()))
            .collect();
        for key in obj.keys() {
            if !known.contains(&key.as_str()) {
                return Err(format!("未知参数 '{}', 可用参数: {}", key, known.join(", ")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_array_schema_still_validates() {
        let schema = json!([
            {"name": "interval", "type": "number", "required": true},
            {"name": "mode", "type": "string", "required": false}
        ])
        .to_string();
        assert!(validate_action_params(&schema, Some(&json!({"interval": 5}))).is_ok());
        assert!(validate_action_params(&schema, Some(&json!({"interval": 5, "extra": 1}))).is_err());
        assert!(validate_action_params(&schema, Some(&json!({}))).is_err());
    }

    #[test]
    fn json_schema_object_validates_demo_actions() {
        let empty = json!({}).to_string();
        assert!(validate_action_params(&empty, Some(&json!({}))).is_ok());

        let schema = json!({
            "type": "object",
            "properties": {"brightness": {"type": "integer", "minimum": 0, "maximum": 100}},
            "required": ["brightness"],
            "additionalProperties": false
        })
        .to_string();
        assert!(validate_action_params(&schema, Some(&json!({"brightness": 80}))).is_ok());
        assert!(validate_action_params(&schema, Some(&json!({"brightness": "80"}))).is_err());
        assert!(validate_action_params(&schema, Some(&json!({}))).is_err());
        assert!(validate_action_params(&schema, Some(&json!({"brightness": 80, "x": 1}))).is_err());
    }

    #[test]
    fn malformed_schema_is_reported() {
        assert!(validate_action_params("not json", Some(&json!({}))).is_err());
        assert!(validate_action_params("42", Some(&json!({}))).is_err());
    }
}
