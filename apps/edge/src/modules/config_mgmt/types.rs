use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub key: String,
    pub cloud_version: Option<String>,
    pub local_version: Option<String>,
    pub updated_at: i64,
}
