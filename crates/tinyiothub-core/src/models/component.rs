use serde::{Deserialize, Serialize};

/// Component entity - 组件实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Component {
    pub id: String,
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub device_num: u32,
    pub description: Option<String>,
    pub options_descriptors: String, // JSON string
    pub location: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Component option entity - 组件选项实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComponentOption {
    pub label: String,
    pub name: String,
    pub default_value: String,
    pub option_type: String, // "string", "number", "boolean", "select"
    pub required: bool,
    pub description: Option<String>,
}

impl ComponentOption {
    pub fn new(
        label: String,
        name: String,
        default_value: String,
        option_type: String,
        required: bool,
    ) -> Self {
        Self {
            label,
            name,
            default_value,
            option_type,
            required,
            description: None,
        }
    }
}

/// Query parameters for component search
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ComponentQuery {
    pub name: Option<String>,
    pub version: Option<String>,
    pub class_name: Option<String>,
    pub location: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

impl Component {
    pub fn new(request: CreateComponentRequest) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: request.name,
            version: request.version,
            class_name: request.class_name,
            device_num: request.device_num.unwrap_or(0),
            description: request.description,
            options_descriptors: serde_json::to_string(&request.options_descriptors).unwrap_or_default(),
            location: request.location,
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// Request for creating a new component
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateComponentRequest {
    pub name: String,
    pub version: String,
    pub class_name: String,
    pub device_num: Option<u32>,
    pub description: Option<String>,
    pub options_descriptors: Vec<ComponentOption>,
    pub location: Option<String>,
}

/// Request for updating a component
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateComponentRequest {
    pub name: Option<String>,
    pub version: Option<String>,
    pub class_name: Option<String>,
    pub device_num: Option<u32>,
    pub description: Option<String>,
    pub options_descriptors: Option<Vec<ComponentOption>>,
    pub location: Option<String>,
}
