// Validate invoke params against the action's parameter schema.
//
// Schema shape: `[{"name": "interval", "type": "number", "required": true}]`.
// Rules: required params present, no unknown params, primitive type match.
// Returns a Chinese error message on mismatch (design 六: 校验明细定位字段).

use serde_json::Value;

pub fn validate_action_params(schema_json: &str, params: Option<&Value>) -> Result<(), String> {
    let schema: Vec<Value> =
        serde_json::from_str(schema_json).map_err(|e| format!("操作参数 schema 解析失败: {}", e))?;
    if schema.is_empty() {
        return Ok(());
    }
    let provided = params.and_then(|p| p.as_object());

    for spec in &schema {
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
