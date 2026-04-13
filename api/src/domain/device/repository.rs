use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::dto::entity::device::{
    CreateDeviceRequest, Device, DeviceStats, DeviceStatusUpdate, UpdateDeviceRequest,
};
use crate::shared::error::Result;

/// Repository interface for device persistence (defined in domain layer)
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Find a device by its ID
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>>;

    /// Find a device by its name
    async fn find_by_name(&self, name: &str) -> Result<Option<Device>>;

    /// Find devices matching the given criteria
    async fn find_all(&self, criteria: &DeviceCriteria) -> Result<Vec<Device>>;

    /// Count devices matching the given criteria
    async fn count(&self, criteria: &DeviceCriteria) -> Result<i64>;

    /// Create a new device
    async fn create(&self, request: &CreateDeviceRequest) -> Result<Device>;

    /// Update an existing device
    async fn update(&self, id: &str, request: &UpdateDeviceRequest) -> Result<Device>;

    /// Delete a device by its ID
    async fn delete(&self, id: &str) -> Result<u64>;

    /// Delete multiple devices by their IDs
    async fn delete_by_ids(&self, ids: &[String]) -> Result<u64>;

    /// Create multiple devices in batch
    async fn create_batch(&self, requests: &[CreateDeviceRequest]) -> Result<Vec<Device>>;

    /// Update the state of a single device
    async fn update_state(&self, id: &str, state: i32) -> Result<()>;

    /// Update states for multiple devices in batch
    async fn update_states_batch(&self, updates: &[(String, i32)]) -> Result<u64>;

    /// Update the enabled status of a device
    async fn update_enabled_status(&self, id: &str, enabled: bool) -> Result<bool>;

    /// Find child devices by parent ID
    async fn find_children(&self, parent_id: &str) -> Result<Vec<Device>>;

    /// Find devices by product ID
    async fn find_by_product_id(&self, product_id: &str) -> Result<Vec<Device>>;

    /// Find devices by driver name
    async fn find_by_driver_name(&self, driver_name: &str) -> Result<Vec<Device>>;

    /// Check if a device with the given name exists
    async fn exists_by_name(&self, name: &str) -> Result<bool>;

    /// Find devices by a list of IDs
    async fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Device>>;

    /// Search devices by keyword across multiple fields
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>>;

    /// Get overall device statistics
    async fn get_stats(&self) -> Result<DeviceStats>;

    /// Get device statistics grouped by device type
    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>>;

    /// Get device statistics grouped by driver name
    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>>;

    /// Get the device tree starting from an optional root ID
    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>>;

    /// Find devices with enabled/search filters and pagination
    async fn find_with_filters(
        &self,
        enabled: Option<bool>,
        search: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<Device>>;

    /// Update detailed status for multiple devices in batch
    async fn update_status_batch(&self, updates: &[DeviceStatusUpdate]) -> Result<u64>;
}

/// Criteria for querying devices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCriteria {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub device_type: Option<String>,
    pub address: Option<String>,
    pub driver_name: Option<String>,
    pub state: Option<i32>,
    pub parent_id: Option<String>,
    pub product_id: Option<String>,
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    pub search_text: Option<String>,
    pub sort_by: DeviceSortBy,
    pub sort_order: DeviceSortOrder,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Sorting options for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DeviceSortBy {
    Name,
    CreatedAt,
    UpdatedAt,
    DeviceType,
    DriverName,
    State,
}

/// Sort order for devices
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DeviceSortOrder {
    Ascending,
    Descending,
}

impl Default for DeviceSortBy {
    fn default() -> Self {
        Self::CreatedAt
    }
}

impl Default for DeviceSortOrder {
    fn default() -> Self {
        Self::Descending
    }
}

impl Default for DeviceCriteria {
    fn default() -> Self {
        Self {
            name: None,
            display_name: None,
            device_type: None,
            address: None,
            driver_name: None,
            state: None,
            parent_id: None,
            product_id: None,
            tenant_id: None,
            workspace_id: None,
            search_text: None,
            sort_by: DeviceSortBy::default(),
            sort_order: DeviceSortOrder::default(),
            limit: None,
            offset: None,
        }
    }
}

impl DeviceCriteria {
    /// Create a new criteria builder
    pub fn builder() -> DeviceCriteriaBuilder {
        DeviceCriteriaBuilder::new()
    }

    /// Filter by name
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// Filter by display name
    pub fn with_display_name(mut self, display_name: String) -> Self {
        self.display_name = Some(display_name);
        self
    }

    /// Filter by device type
    pub fn with_device_type(mut self, device_type: String) -> Self {
        self.device_type = Some(device_type);
        self
    }

    /// Filter by address
    pub fn with_address(mut self, address: String) -> Self {
        self.address = Some(address);
        self
    }

    /// Filter by driver name
    pub fn with_driver_name(mut self, driver_name: String) -> Self {
        self.driver_name = Some(driver_name);
        self
    }

    /// Filter by state
    pub fn with_state(mut self, state: i32) -> Self {
        self.state = Some(state);
        self
    }

    /// Filter by parent ID
    pub fn with_parent_id(mut self, parent_id: String) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Filter by product ID
    pub fn with_product_id(mut self, product_id: String) -> Self {
        self.product_id = Some(product_id);
        self
    }

    /// Filter by tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Filter by workspace ID
    pub fn with_workspace_id(mut self, workspace_id: String) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    /// Filter by search text
    pub fn with_search_text(mut self, text: String) -> Self {
        self.search_text = Some(text);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, sort_by: DeviceSortBy, sort_order: DeviceSortOrder) -> Self {
        self.sort_by = sort_by;
        self.sort_order = sort_order;
        self
    }

    /// Set pagination
    pub fn with_pagination(mut self, limit: u32, offset: u32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Builder for DeviceCriteria
pub struct DeviceCriteriaBuilder {
    criteria: DeviceCriteria,
}

impl DeviceCriteriaBuilder {
    pub fn new() -> Self {
        Self {
            criteria: DeviceCriteria::default(),
        }
    }

    pub fn name(mut self, name: String) -> Self {
        self.criteria.name = Some(name);
        self
    }

    pub fn display_name(mut self, display_name: String) -> Self {
        self.criteria.display_name = Some(display_name);
        self
    }

    pub fn device_type(mut self, device_type: String) -> Self {
        self.criteria.device_type = Some(device_type);
        self
    }

    pub fn address(mut self, address: String) -> Self {
        self.criteria.address = Some(address);
        self
    }

    pub fn driver_name(mut self, driver_name: String) -> Self {
        self.criteria.driver_name = Some(driver_name);
        self
    }

    pub fn state(mut self, state: i32) -> Self {
        self.criteria.state = Some(state);
        self
    }

    pub fn parent_id(mut self, parent_id: String) -> Self {
        self.criteria.parent_id = Some(parent_id);
        self
    }

    pub fn product_id(mut self, product_id: String) -> Self {
        self.criteria.product_id = Some(product_id);
        self
    }

    pub fn tenant_id(mut self, tenant_id: String) -> Self {
        self.criteria.tenant_id = Some(tenant_id);
        self
    }

    pub fn workspace_id(mut self, workspace_id: String) -> Self {
        self.criteria.workspace_id = Some(workspace_id);
        self
    }

    pub fn search_text(mut self, text: String) -> Self {
        self.criteria.search_text = Some(text);
        self
    }

    pub fn sort_by(mut self, sort_by: DeviceSortBy) -> Self {
        self.criteria.sort_by = sort_by;
        self
    }

    pub fn sort_order(mut self, sort_order: DeviceSortOrder) -> Self {
        self.criteria.sort_order = sort_order;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.criteria.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.criteria.offset = Some(offset);
        self
    }

    pub fn build(self) -> DeviceCriteria {
        self.criteria
    }
}

impl Default for DeviceCriteriaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criteria_builder() {
        let criteria = DeviceCriteria::builder()
            .name("sensor-01".to_string())
            .device_type("temperature".to_string())
            .driver_name("modbus".to_string())
            .state(1)
            .workspace_id("ws-1".to_string())
            .sort_by(DeviceSortBy::Name)
            .sort_order(DeviceSortOrder::Ascending)
            .limit(100)
            .offset(0)
            .build();

        assert_eq!(criteria.name, Some("sensor-01".to_string()));
        assert_eq!(criteria.device_type, Some("temperature".to_string()));
        assert_eq!(criteria.driver_name, Some("modbus".to_string()));
        assert_eq!(criteria.state, Some(1));
        assert_eq!(criteria.workspace_id, Some("ws-1".to_string()));
        assert!(matches!(criteria.sort_by, DeviceSortBy::Name));
        assert!(matches!(criteria.sort_order, DeviceSortOrder::Ascending));
        assert_eq!(criteria.limit, Some(100));
        assert_eq!(criteria.offset, Some(0));
    }

    #[test]
    fn test_criteria_fluent_interface() {
        let criteria = DeviceCriteria::default()
            .with_name("sensor-02".to_string())
            .with_state(0)
            .with_sort(DeviceSortBy::State, DeviceSortOrder::Descending)
            .with_pagination(50, 10);

        assert_eq!(criteria.name, Some("sensor-02".to_string()));
        assert_eq!(criteria.state, Some(0));
        assert!(matches!(criteria.sort_by, DeviceSortBy::State));
        assert!(matches!(criteria.sort_order, DeviceSortOrder::Descending));
        assert_eq!(criteria.limit, Some(50));
        assert_eq!(criteria.offset, Some(10));
    }
}
