use std::sync::Arc;

use crate::shared::error::{EdgeError, EdgeResult};
use tinyiothub_core::models::device::{CreateDeviceRequest, Device};
use tinyiothub_core::repository::device::{DeviceCriteria, DeviceRepository};

pub struct DeviceService {
    repo: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(repo: Arc<dyn DeviceRepository>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn get_device(&self, id: &str) -> EdgeResult<Device> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| EdgeError::Internal(format!("device not found: {}", id)))
    }

    pub async fn list_devices(&self, driver_name: Option<&str>) -> EdgeResult<Vec<Device>> {
        let criteria = if let Some(dn) = driver_name {
            DeviceCriteria { driver_name: Some(dn.to_string()), ..Default::default() }
        } else {
            DeviceCriteria::default()
        };
        Ok(self.repo.find_all(&criteria).await?)
    }

    pub async fn sync_from_cloud(&self, cloud_devices: &[CreateDeviceRequest]) -> EdgeResult<Vec<Device>> {
        if cloud_devices.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.repo.create_batch(cloud_devices).await?)
    }

    pub async fn get_driver_for_device(&self, device_id: &str) -> EdgeResult<String> {
        let device = self.get_device(device_id).await?;
        device
            .driver_name
            .ok_or_else(|| EdgeError::Internal("device has no driver configured".into()))
    }
}
