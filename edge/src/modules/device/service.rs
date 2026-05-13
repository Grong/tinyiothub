use std::sync::Arc;

use tinyiothub_core::models::device::{CreateDeviceRequest, Device};
use tinyiothub_core::repository::device::{DeviceCriteria, DeviceRepository};

pub struct DeviceService {
    repo: Arc<dyn DeviceRepository>,
}

impl DeviceService {
    pub fn new(repo: Arc<dyn DeviceRepository>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn get_device(
        &self,
        id: &str,
    ) -> Result<Device, Box<dyn std::error::Error + Send + Sync>> {
        self.repo
            .find_by_id(id)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .ok_or_else(|| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("device not found: {}", id),
                )) as Box<dyn std::error::Error + Send + Sync>
            })
    }

    pub async fn list_devices(
        &self,
        driver_name: Option<&str>,
    ) -> Result<Vec<Device>, Box<dyn std::error::Error + Send + Sync>> {
        let criteria = if let Some(dn) = driver_name {
            let mut c = DeviceCriteria::default();
            c.driver_name = Some(dn.to_string());
            c
        } else {
            DeviceCriteria::default()
        };
        self.repo
            .find_all(&criteria)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    pub async fn sync_from_cloud(
        &self,
        cloud_devices: &[CreateDeviceRequest],
    ) -> Result<Vec<Device>, Box<dyn std::error::Error + Send + Sync>> {
        if cloud_devices.is_empty() {
            return Ok(Vec::new());
        }
        self.repo
            .create_batch(cloud_devices)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }

    pub async fn get_driver_for_device(
        &self,
        device_id: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let device = self.get_device(device_id).await?;
        device.driver_name.ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "device has no driver configured",
            )) as Box<dyn std::error::Error + Send + Sync>
        })
    }
}
