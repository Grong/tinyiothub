use std::sync::Arc;
use tinyiothub_storage::sqlite::Database;

pub struct DeviceService {
    _db: Arc<Database>,
}

impl DeviceService {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Arc::new(Self { _db: db })
    }
    pub async fn list_devices(
        &self,
        _driver_name: Option<&str>,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(Vec::new())
    }
    pub async fn get_driver_for_device(
        &self,
        _device_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok("default".into())
    }
}
