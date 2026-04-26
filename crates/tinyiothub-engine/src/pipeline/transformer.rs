//! Data transformer — applies mappings and unit conversions.
//!
//! TODO: Migrate transformation logic from `cloud/src/domain/device/`.

use std::collections::HashMap;

/// Transforms decoded telemetry data.
#[derive(Debug, Default)]
pub struct DataTransformer {
    mappings: HashMap<String, String>,
}

impl DataTransformer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field mapping (source_field -> target_field).
    pub fn add_mapping(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.mappings.insert(source.into(), target.into());
    }

    /// Apply all mappings to the input data.
    ///
    /// Returns a new JSON object with mapped field names.
    /// Unmapped fields are preserved as-is.
    pub fn transform(&self, input: &serde_json::Value) -> Result<serde_json::Value, String> {
        let input_obj = match input.as_object() {
            Some(obj) => obj.clone(),
            None => return Err("input must be a JSON object".to_string()),
        };

        let mut output = serde_json::Map::new();

        for (key, value) in input_obj {
            let target_key = self.mappings.get(&key).cloned().unwrap_or(key);
            output.insert(target_key, value);
        }

        Ok(serde_json::Value::Object(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_no_mappings() {
        let transformer = DataTransformer::new();
        let input = serde_json::json!({"temp": 25.0, "humidity": 60});
        let result = transformer.transform(&input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn test_transform_with_mappings() {
        let mut transformer = DataTransformer::new();
        transformer.add_mapping("temp", "temperature");
        let input = serde_json::json!({"temp": 25.0, "humidity": 60});
        let result = transformer.transform(&input).unwrap();
        let expected = serde_json::json!({"temperature": 25.0, "humidity": 60});
        assert_eq!(result, expected);
    }

    #[test]
    fn test_transform_non_object_fails() {
        let transformer = DataTransformer::new();
        let input = serde_json::json!(42);
        assert!(transformer.transform(&input).is_err());
    }
}
