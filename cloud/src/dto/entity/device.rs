pub use tinyiothub_core::models::device::*;

use crate::infrastructure::persistence::database::Database;

/// 根据 ID 查找设备（临时兼容包装，MCP tools 仍在调用）
pub async fn find_device_by_id(db: &Database, id: &str) -> Result<Option<Device>, sqlx::Error> {
    use crate::domain::device::repository::DeviceRepository;
    let repo = crate::infrastructure::persistence::repositories::SqliteDeviceRepository::new(db.clone());
    repo.find_by_id(id).await.map_err(|e| match e {
        crate::shared::error::Error::NotFound => sqlx::Error::RowNotFound,
        _ => sqlx::Error::RowNotFound,
    })
}

/// 加载设备的标签
pub async fn load_device_tags(device: &mut Device, db: &Database) -> Result<(), sqlx::Error> {
    use crate::domain::tag::repository::TagRepository;
    use crate::infrastructure::persistence::repositories::SqliteTagRepository;
    let tag_repo = SqliteTagRepository::new(db.clone());
    let tenant_id = device.tenant_id.as_deref().unwrap_or("");
    let tags = tag_repo.find_by_target_id(&device.id, tenant_id).await.map_err(|_| sqlx::Error::RowNotFound)?;
    device.tags = Some(tags);
    Ok(())
}

/// 为设备列表批量加载标签
pub async fn load_tags_for_devices(
    db: &Database,
    devices: &mut [Device],
) -> Result<(), sqlx::Error> {
    use crate::domain::tag::repository::TagRepository;
    use crate::infrastructure::persistence::repositories::SqliteTagRepository;
    let tag_repo = SqliteTagRepository::new(db.clone());

    for device in devices {
        let tenant_id = device.tenant_id.as_deref().unwrap_or("");
        let tags = tag_repo.find_by_target_id(&device.id, tenant_id).await.map_err(|_| sqlx::Error::RowNotFound)?;
        device.tags = Some(tags);
    }

    Ok(())
}

/// 根据ID查找设备并包含标签信息（临时兼容包装，MCP tools 仍在调用）
pub async fn find_device_by_id_with_tags(
    db: &Database,
    id: &str,
) -> Result<Option<Device>, sqlx::Error> {
    if let Some(mut device) = find_device_by_id(db, id).await? {
        load_device_tags(&mut device, db).await?;
        Ok(Some(device))
    } else {
        Ok(None)
    }
}

/// 查询设备列表并包含标签信息（临时兼容包装，MCP tools 仍在调用）
pub async fn find_all_devices_with_tags(
    db: &Database,
    params: &DeviceQueryParams,
) -> Result<Vec<Device>, sqlx::Error> {
    use crate::domain::device::repository::{DeviceCriteria, DeviceRepository, DeviceSortBy, DeviceSortOrder};
    let criteria = DeviceCriteria {
        name: params.name.clone(),
        display_name: params.display_name.clone(),
        device_type: params.device_type.clone(),
        address: params.address.clone(),
        driver_name: params.driver_name.clone(),
        state: params.state,
        parent_id: params.parent_id.clone(),
        product_id: params.product_id.clone(),
        tenant_id: params.tenant_id.clone(),
        workspace_id: params.workspace_id.clone(),
        search_text: None,
        sort_by: DeviceSortBy::CreatedAt,
        sort_order: DeviceSortOrder::Descending,
        limit: params.page_size,
        offset: params.page.map(|p| p.saturating_sub(1) * params.page_size.unwrap_or(0)),
    };
    let repo = crate::infrastructure::persistence::repositories::SqliteDeviceRepository::new(db.clone());
    let mut devices = repo.find_all(&criteria).await.map_err(|e| match e {
        crate::shared::error::Error::NotFound => sqlx::Error::RowNotFound,
        _ => sqlx::Error::RowNotFound,
    })?;
    load_tags_for_devices(db, &mut devices).await?;
    Ok(devices)
}
