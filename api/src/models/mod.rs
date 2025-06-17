use serde::{Deserialize, Serialize};

pub mod _entities;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
    pub name: Option<String>,
    pub is_created_by_me: Option<bool>,
}

fn default_page() -> u64 {
    1
}
fn default_limit() -> u64 {
    30
}

#[derive(Debug, Serialize)]
pub struct PaginatedResult<T> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub pages: u64,
    pub has_more: bool,
}

pub mod apps;
pub mod prelude;
pub mod tags;
pub mod users;

pub mod device_events;
pub mod device_properties;
pub mod device_service_calls;
pub mod device_templates;
pub mod devices;
pub mod tag_bindings;
