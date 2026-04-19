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
    pub fn transform(&self, _input: &serde_json::Value) -> Result<serde_json::Value, String> {
        // TODO: implement transformation logic
        Err("not yet implemented".into())
    }
}
