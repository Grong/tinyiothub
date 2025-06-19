use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TagResponse {
    pub id: i32,
    pub name: String,
    pub r#type: String,
}