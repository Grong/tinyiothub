pub mod config;
pub mod constants;
pub mod error;
pub mod models;
pub mod types;
pub mod version;

/// Generate a unique ID using UUID v4
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Generate current timestamp as "%Y-%m-%d %H:%M:%S" string (UTC)
pub fn now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
