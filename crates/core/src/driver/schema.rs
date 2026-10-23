//! Minimal JSON Schema validator for declared tool parameters (v1 subset:
//! type / properties / required / additionalProperties / enum / min / max).

use serde_json::Value;

pub fn validate_params(schema: &Value, params: &Value) -> Result<(), String> {
    let obj = schema.as_object().ok_or("schema must be a JSON object")?;
    match obj.get("type").and_then(Value::as_str) {
        Some("object") => validate_object(obj, params),
        Some(other) => validate_value(other, obj, params),
        None => Ok(()),
    }
}

fn validate_object(schema: &serde_json::Map<String, Value>, params: &Value) -> Result<(), String> {
    let params = params.as_object().ok_or("expected object")?;
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for r in required {
            let name = r.as_str().ok_or("required entries must be strings")?;
            if !params.contains_key(name) {
                return Err(format!("missing required field: {name}"));
            }
        }
    }
    let props = schema.get("properties").and_then(Value::as_object);
    let deny_unknown = schema.get("additionalProperties").and_then(Value::as_bool) == Some(false);
    for (key, value) in params {
        match props.and_then(|p| p.get(key)) {
            Some(prop_schema) => {
                let pobj = prop_schema.as_object().ok_or("property schema must be an object")?;
                let ty = pobj.get("type").and_then(Value::as_str).unwrap_or("");
                validate_value(ty, pobj, value).map_err(|e| format!("{key}: {e}"))?;
            }
            None if deny_unknown => return Err(format!("unknown field: {key}")),
            None => {}
        }
    }
    Ok(())
}

fn validate_value(ty: &str, schema: &serde_json::Map<String, Value>, value: &Value) -> Result<(), String> {
    match ty {
        "string" if !value.is_string() => return Err("expected string".into()),
        "boolean" if !value.is_boolean() => return Err("expected boolean".into()),
        "integer" if !value.is_i64() && !value.is_u64() => return Err("expected integer".into()),
        "number" if !value.is_number() => return Err("expected number".into()),
        "array" if !value.is_array() => return Err("expected array".into()),
        "object" if !value.is_object() => return Err("expected object".into()),
        _ => {}
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err("value not in enum".into());
    }
    if let Some(n) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && n < min
        {
            return Err(format!("below minimum {min}"));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && n > max
        {
            return Err(format!("above maximum {max}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_valid_object() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer","minimum":0,"maximum":100}},"required":["level"],"additionalProperties":false});
        validate_params(&schema, &json!({"level": 50})).unwrap();
    }

    #[test]
    fn rejects_missing_required() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer"}},"required":["level"]});
        assert!(
            validate_params(&schema, &json!({}))
                .unwrap_err()
                .contains("missing required")
        );
    }

    #[test]
    fn rejects_range_violation() {
        let schema = json!({"type":"object","properties":{"level":{"type":"integer","minimum":0,"maximum":100}}});
        assert!(
            validate_params(&schema, &json!({"level": 101}))
                .unwrap_err()
                .contains("above maximum")
        );
    }

    #[test]
    fn rejects_unknown_field_when_additional_properties_false() {
        let schema = json!({"type":"object","properties":{},"additionalProperties":false});
        assert!(
            validate_params(&schema, &json!({"x":1}))
                .unwrap_err()
                .contains("unknown field")
        );
    }
}
