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
    // 场景包的属性（如占地面积/容积率）可以没有描述——与 cloud 运行时类型对齐
    #[serde(default)]
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
    // 场景包（category = "scenes"）无设备字段，缺省为空字符串
    #[serde(default)]
    pub device_type: String,
    #[serde(default)]
    pub protocol_type: String,
    #[serde(default)]
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
    /// 透传未建模字段（场景包的 parameters/children/thing_category 等），
    /// 保证 get_template 返回完整 JSON 而不丢字段。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_mit_license() -> String {
    "MIT".to_string()
}

// ── Request types ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: usize,
    // 前端经 cloud proxy 透传 camelCase→snake_case 的 page_size，两种拼写都接受
    #[serde(default = "default_per_page", alias = "page_size")]
    pub per_page: usize,
    pub search: Option<String>,
    pub category: Option<String>,
    // 前端发 protocol_type；本服务驱动字段名为 protocol，两种拼写都接受
    #[serde(alias = "protocol_type")]
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
        // checked：page 无上界（如 u64::MAX 经 query 反序列化进来），
        // 乘法溢出在 debug 下 panic、release 下回绕成任意 offset
        (self.page - 1).checked_mul(self.per_page).unwrap_or(usize::MAX)
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

    #[test]
    fn offset_overflow_saturates() {
        // page 无 validate 上界：极大 page 不得 panic（debug）或回绕（release）
        let params = PaginationParams {
            page: usize::MAX,
            per_page: 100,
            search: None,
            category: None,
            protocol: None,
        };
        assert_eq!(params.offset(), usize::MAX);
    }
}
