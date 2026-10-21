//! In-memory hub used by the materialization profile: the projection store
//! records what was written so the profile can compare counts with the
//! declaration.

use std::sync::Mutex;

use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
use tinyiothub_core::models::thing::{Thing, ThingStatus};
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;

use crate::ports::DeclarationProjectionStore;

#[derive(Default)]
struct InMemoryProjection {
    properties: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl DeclarationProjectionStore for InMemoryProjection {
    async fn upsert_properties(&self, _thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
        *self.properties.lock().unwrap() = fields.iter().map(|f| f.name.clone()).collect();
        Ok(())
    }

    async fn upsert_commands(&self, _thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
        *self.commands.lock().unwrap() = tools.iter().map(|t| t.name.clone()).collect();
        Ok(())
    }

    async fn mark_stale(
        &self,
        _thing_id: &str,
        _keep_properties: &[String],
        _keep_commands: &[String],
    ) -> Result<(), String> {
        Ok(())
    }
}

pub struct TestHub {
    projection: InMemoryProjection,
}

impl Default for TestHub {
    fn default() -> Self {
        Self::new()
    }
}

impl TestHub {
    pub fn new() -> Self {
        Self {
            projection: InMemoryProjection::default(),
        }
    }

    pub fn projection(&self) -> &dyn DeclarationProjectionStore {
        &self.projection
    }

    /// (properties, commands) written by the last materialization.
    pub fn projection_counts(&self) -> (usize, usize) {
        (
            self.projection.properties.lock().unwrap().len(),
            self.projection.commands.lock().unwrap().len(),
        )
    }
}

/// Fixture with one property and one command (Task 8 shape).
pub fn fixture_thing() -> Thing {
    Thing {
        id: "t1".into(),
        name: "t1".into(),
        status: ThingStatus::Online,
        driver_name: Some("simulator".into()),
        properties: Some(vec![ThingProperty {
            id: "p1".into(),
            thing_id: "t1".into(),
            name: "temperature".into(),
            display_name: None,
            description: None,
            data_type: Some("float".into()),
            unit: Some("C".into()),
            min_value: Some(-40.0),
            max_value: Some(125.0),
            default_value: None,
            is_read_only: 1,
            source: Some("template".into()),
            expression: None,
            refs: None,
            created_at: None,
            updated_at: None,
            current_value: None,
            alarm_status: None,
            derived_status: None,
        }]),
        commands: Some(vec![ThingCommand {
            id: "c1".into(),
            thing_id: "t1".into(),
            name: "restart".into(),
            display_name: None,
            description: None,
            parameters: Some("{\"type\":\"object\"}".into()),
            created_at: "2026-10-21".into(),
        }]),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::materialize::materialize_declaration;
    use tinyiothub_core::driver::manifest::ToolKindDecl;

    fn declaration() -> tinyiothub_core::driver::declaration::DeviceDeclaration {
        tinyiothub_core::driver::declaration::DeviceDeclaration {
            manifest_version: "1.0".into(),
            driver: "demo".into(),
            driver_version: "1.0.0".into(),
            state_fields: vec![StateFieldDecl {
                name: "level".into(),
                data_type: "integer".into(),
                unit: None,
                range: None,
                read_only: false,
                required: true,
                source: None,
            }],
            tools: vec![ManifestTool {
                name: "set_level".into(),
                kind: ToolKindDecl::Action,
                target_field: None,
                parameters: serde_json::json!({"type": "object"}),
                timeout: Default::default(),
                safety: Default::default(),
                resources: vec![],
                cancellable: true,
            }],
        }
    }

    #[tokio::test]
    async fn counts_materialized_properties_and_commands() {
        let hub = TestHub::new();
        materialize_declaration(hub.projection(), "test-thing", &declaration())
            .await
            .unwrap();
        assert_eq!(hub.projection_counts(), (1, 1));
    }
}
