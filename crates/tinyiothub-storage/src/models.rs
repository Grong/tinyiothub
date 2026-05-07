//! DAO (Data Access Object) models for the storage layer.
//!
//! These types are used internally by repository implementations.
//! They are thin wrappers or re-exports from `tinyiothub_core::models`
//! with optional storage-specific metadata.

use serde::{Deserialize, Serialize};

/// Row-level metadata tracked by the storage layer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RowMetadata {
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub version: Option<i32>,
}

/// Pagination parameters for repository queries.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { page: 1, page_size: 20 }
    }
}

impl Pagination {
    pub fn offset(&self) -> u32 {
        self.page.saturating_sub(1) * self.page_size
    }

    pub fn limit(&self) -> u32 {
        self.page_size
    }
}

/// Sort order for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    Ascending,
    Descending,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self::Descending
    }
}

/// Filter operator for query conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In,
    IsNull,
}

/// A single filter condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub field: String,
    pub op: FilterOp,
    pub value: Option<serde_json::Value>,
}
