use serde::{Deserialize, Serialize};

// ── Domain models ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Driver {
    pub id: String,
    pub name: String,
    pub version: String,
    pub protocol: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub author_name: String,
    #[serde(default)]
    pub author_email: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_zero")]
    pub downloads: i64,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub reviews: Option<i32>,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub platforms: Option<serde_json::Value>,
    #[serde(default)]
    pub requirements: Option<serde_json::Value>,
    #[serde(default)]
    pub created_at: String,
    pub updated_at: String,
}

fn default_zero() -> i64 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalizedString {
    #[serde(default)]
    pub zh: Option<String>,
    #[serde(default)]
    pub en: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub name: String,
    pub display_name: LocalizedString,
    pub description: LocalizedString,
    pub data_type: String,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub min_value: Option<f64>,
    #[serde(default)]
    pub max_value: Option<f64>,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub is_read_only: bool,
    #[serde(default)]
    pub is_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub display_name: LocalizedString,
    pub description: LocalizedString,
    #[serde(default)]
    pub parameters: Option<String>,
    #[serde(default)]
    pub parameter_schema: Option<String>,
    #[serde(default)]
    pub is_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceInfo {
    #[serde(default)]
    pub default_name_pattern: Option<String>,
    #[serde(default)]
    pub default_display_name_pattern: Option<LocalizedString>,
    #[serde(default)]
    pub default_description: Option<LocalizedString>,
    #[serde(default)]
    pub required_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub display_name: LocalizedString,
    pub description: LocalizedString,
    pub version: String,
    pub author: String,
    pub category: String,
    #[serde(default)]
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub protocol_type: String,
    pub driver_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub device_info: DeviceInfo,
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_zero")]
    pub downloads: i64,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub reviews: Option<i32>,
    #[serde(default = "default_mit_license")]
    pub license: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_mit_license() -> String {
    "MIT".to_string()
}

// ── Request types ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_per_page")]
    pub per_page: usize,
    pub search: Option<String>,
    pub category: Option<String>,
    pub protocol: Option<String>,
}

fn default_page() -> usize {
    1
}

fn default_per_page() -> usize {
    20
}

impl PaginationParams {
    pub const MAX_PER_PAGE: usize = 100;
    const MAX_SEARCH_LEN: usize = 200;

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.page == 0 {
            return Err("page must be >= 1");
        }
        if self.per_page > Self::MAX_PER_PAGE {
            return Err("per_page must be <= 100");
        }
        if self.per_page == 0 {
            return Err("per_page must be >= 1");
        }
        if self.search.as_ref().map_or(0, |s| s.len()) > Self::MAX_SEARCH_LEN {
            return Err("search must be <= 200 characters");
        }
        Ok(())
    }

    pub fn offset(&self) -> usize {
        (self.page - 1) * self.per_page
    }
}

// ── Response types ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedList<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

impl<T> PaginatedList<T> {
    pub fn new(items: Vec<T>, total: usize, page: usize, per_page: usize) -> Self {
        Self {
            items,
            total,
            page,
            per_page,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub last_sync: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_params() {
        let params = PaginationParams {
            page: 1,
            per_page: 20,
            search: None,
            category: None,
            protocol: None,
        };
        params.validate().unwrap();
        assert_eq!(params.offset(), 0);
    }

    #[test]
    fn page_zero_invalid() {
        let params = PaginationParams {
            page: 0,
            per_page: 20,
            search: None,
            category: None,
            protocol: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn per_page_over_100_invalid() {
        let params = PaginationParams {
            page: 1,
            per_page: 101,
            search: None,
            category: None,
            protocol: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn per_page_zero_invalid() {
        let params = PaginationParams {
            page: 1,
            per_page: 0,
            search: None,
            category: None,
            protocol: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn offset_calculation() {
        let params = PaginationParams {
            page: 3,
            per_page: 10,
            search: None,
            category: None,
            protocol: None,
        };
        assert_eq!(params.offset(), 20);
    }
}
