use tinyiothub_storage::sqlite::{
    database::Database,
    device::SqliteDeviceRepository,
};
use tinyiothub_storage::traits::{
    device::{DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder},
};
use tinyiothub_core::models::device::{Device, DeviceQueryParams};

use crate::modules::tag::SqliteTagRepository;
use crate::modules::tag::TagRepository;

/// Find a device by ID (convenience wrapper for MCP tools compatibility)
pub async fn find_device_by_id(
    db: &Database,
    id: &str,
) -> Result<Option<Device>, sqlx::Error> {
    let repo = SqliteDeviceRepository::new(db.clone());
    repo.find_by_id(id).await.map_err(|_| sqlx::Error::RowNotFound)
}

/// Load tags for a single device
pub async fn load_device_tags(
    device: &mut Device,
    db: &Database,
    tenant_id: &str,
) -> Result<(), sqlx::Error> {
    let tag_repo = SqliteTagRepository::new(db.clone());
    let tags = tag_repo
        .find_by_target_id(&device.id, tenant_id)
        .await
        .map_err(|_| sqlx::Error::RowNotFound)?;
    let tag_values: Vec<serde_json::Value> = tags
        .into_iter()
        .map(|t| serde_json::to_value(t).unwrap_or_default())
        .collect();
    device.tags = Some(tag_values);
    Ok(())
}

/// Load tags for multiple devices
pub async fn load_tags_for_devices(
    db: &Database,
    devices: &mut [Device],
    tenant_id: &str,
) -> Result<(), sqlx::Error> {
    let tag_repo = SqliteTagRepository::new(db.clone());

    for device in devices {
        let tags = tag_repo
            .find_by_target_id(&device.id, tenant_id)
            .await
            .map_err(|_| sqlx::Error::RowNotFound)?;
        let tag_values: Vec<serde_json::Value> = tags
            .into_iter()
            .map(|t| serde_json::to_value(t).unwrap_or_default())
            .collect();
        device.tags = Some(tag_values);
    }

    Ok(())
}

/// Find a device by ID including its tags
pub async fn find_device_by_id_with_tags(
    db: &Database,
    id: &str,
    tenant_id: &str,
) -> Result<Option<Device>, sqlx::Error> {
    if let Some(mut device) = find_device_by_id(db, id).await? {
        load_device_tags(&mut device, db, tenant_id).await?;
        Ok(Some(device))
    } else {
        Ok(None)
    }
}

/// Find all devices matching query params, including tags
pub async fn find_all_devices_with_tags(
    db: &Database,
    params: &DeviceQueryParams,
    tenant_id: Option<String>,
    _workspace_id: Option<String>,
) -> Result<Vec<Device>, sqlx::Error> {
    let criteria = DeviceCriteria {
        name: params.name.clone(),
        display_name: params.display_name.clone(),
        device_type: params.device_type.clone(),
        address: params.address.clone(),
        driver_name: params.driver_name.clone(),
        state: params.state,
        parent_id: params.parent_id.clone(),
        product_id: params.product_id.clone(),
        workspace_id: _workspace_id,
        search_text: None,
        tag_name: None,
        sort_by: DeviceSortBy::CreatedAt,
        sort_order: DeviceSortOrder::Descending,
        limit: params.page_size,
        offset: params.page.map(|p| p.saturating_sub(1) * params.page_size.unwrap_or(0)),
    };
    let repo = SqliteDeviceRepository::new(db.clone());
    let mut devices = repo
        .find_all(&criteria)
        .await
        .map_err(|_| sqlx::Error::RowNotFound)?;
    let tenant_id_for_tags = tenant_id.as_deref().unwrap_or("");
    load_tags_for_devices(db, &mut devices, tenant_id_for_tags).await?;
    Ok(devices)
}
