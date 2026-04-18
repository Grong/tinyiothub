pub mod commands;
pub mod dashboard;
pub mod management;
pub mod monitoring;
pub mod profile;
pub mod properties;
pub mod trace;

use crate::dto::entity::device::Device;
use crate::shared::{app_state::AppState, error::Error};

/// Verify a device belongs to the authenticated user's tenant.
/// Returns the device on success, or an appropriate error on failure.
pub async fn verify_device_tenant(
    state: &AppState,
    device_id: &str,
    tenant_id: &str,
) -> Result<Device, Error> {
    match state.device_service.get_device_by_id(device_id).await {
        Ok(Some(device)) => {
            if device.tenant_id.as_ref() == Some(&tenant_id.to_string()) {
                Ok(device)
            } else {
                tracing::warn!(
                    "Access denied: device {} belongs to tenant {:?}, user tenant {}",
                    device_id,
                    device.tenant_id,
                    tenant_id
                );
                Err(Error::NotFound)
            }
        }
        Ok(None) => {
            tracing::warn!("Access denied: device {} not found", device_id);
            Err(Error::NotFound)
        }
        Err(e) => {
            tracing::error!("Failed to verify device {}: {}", device_id, e);
            Err(e)
        }
    }
}

use axum::Router;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .merge(management::create_router())
        .merge(properties::create_router())
        .merge(commands::create_router())
        .merge(dashboard::create_router())
        .merge(profile::create_router())
        .merge(trace::create_router())
        .merge(monitoring::create_router())
}
