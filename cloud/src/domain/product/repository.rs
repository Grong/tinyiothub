use async_trait::async_trait;

use crate::dto::entity::product::{CreateProductRequest, Product, ProductQueryParams, UpdateProductRequest};
use tinyiothub_core::error::Result;

#[async_trait]
pub trait ProductRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Product>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Product>>;
    async fn create(&self, request: &CreateProductRequest) -> Result<Product>;
    async fn update(&self, id: &str, request: &UpdateProductRequest) -> Result<Product>;
    async fn delete(&self, id: &str) -> Result<u64>;
    async fn find_all(&self, params: &ProductQueryParams) -> Result<Vec<Product>>;
    async fn count(&self, params: &ProductQueryParams) -> Result<i64>;
    async fn exists_by_name(&self, name: &str) -> Result<bool>;
    async fn find_by_device_type(&self, device_type: &str) -> Result<Vec<Product>>;
    async fn find_by_manufacturer(&self, manufacturer: &str) -> Result<Vec<Product>>;
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Product>>;
    async fn get_stats_by_device_type(&self) -> Result<Vec<(String, i64)>>;
    async fn get_stats_by_manufacturer(&self) -> Result<Vec<(String, i64)>>;
    async fn find_with_filters(
        &self,
        name: Option<String>,
        manufacturer: Option<String>,
        device_type: Option<String>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<Product>>;
    async fn exists_by_name_excluding_id(&self, name: &str, exclude_id: &str) -> Result<bool>;
}
