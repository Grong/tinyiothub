use serde::{Deserialize, Serialize};

pub mod _entities;
pub mod apps;
pub mod device;
pub mod device_event;
pub mod device_property;
pub mod device_service_call;
pub mod device_template;
pub mod prelude;
pub mod tags;
pub mod users;

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
