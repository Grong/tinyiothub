//! E4a declarative driver installation API and the thing-creation hook
//! adapter over `AccessService`.

pub mod handler;

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::driver::schema::validate_params;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::access::service::AccessService;

use crate::domains::thing::hooks::{CreationMode, ThingDeclarationHook};

/// `ThingDeclarationHook` backed by the runtime access service.
///
/// Stateless: every call receives its workspace explicitly, so a single
/// Arc can be shared by per-request `ThingService` instances.
pub struct AccessDeclarationHook {
    pub access: Arc<AccessService>,
}

#[async_trait]
impl ThingDeclarationHook for AccessDeclarationHook {
    async fn prepare_creation(
        &self,
        workspace_id: &str,
        driver_name: &str,
        options: Option<&str>,
    ) -> Result<CreationMode, String> {
        let pkg = self
            .access
            .get_manifest(workspace_id, driver_name)
            .ok_or_else(|| format!("driver '{driver_name}' has no installed manifest"))?;
        // An absent config object means "no options"; an explicit empty
        // string / `null` is still parsed so malformed input is rejected.
        let opts: serde_json::Value = match options {
            Some(raw) if !raw.trim().is_empty() => {
                serde_json::from_str(raw).map_err(|e| format!("invalid driver_options JSON: {e}"))?
            }
            _ => serde_json::json!({}),
        };
        validate_params(&pkg.manifest.config, &opts).map_err(|e| format!("driver_options invalid: {e}"))?;
        Ok(if pkg.manifest.identity.dynamic_capabilities {
            CreationMode::TemplateBootstrap
        } else {
            CreationMode::Declarative
        })
    }

    async fn materialize(&self, workspace_id: &str, thing_id: &str) -> Result<(), String> {
        let thing: Thing = self
            .access
            .things
            .load_thing(thing_id)
            .await?
            .ok_or_else(|| format!("thing '{thing_id}' not found"))?;
        self.access
            .bind_thing(workspace_id, &thing)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
