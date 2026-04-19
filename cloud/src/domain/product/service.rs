use std::sync::Arc;

use tinyiothub_core::models::product::{CreateProductRequest, Product, ProductQueryParams, UpdateProductRequest};
use crate::shared::error::Result;

use super::repository::ProductRepository;

pub struct ProductService {
    product_repository: Arc<dyn ProductRepository>,
}

impl ProductService {
    pub fn new(product_repository: Arc<dyn ProductRepository>) -> Self {
        Self { product_repository }
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Product>> {
        self.product_repository.find_by_id(id).await
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<Product>> {
        self.product_repository.find_by_name(name).await
    }

    pub async fn create(&self, request: &CreateProductRequest) -> Result<Product> {
        self.product_repository.create(request).await
    }

    pub async fn update(&self, id: &str, request: &UpdateProductRequest) -> Result<Product> {
        self.product_repository.update(id, request).await
    }

    pub async fn delete(&self, id: &str) -> Result<u64> {
        self.product_repository.delete(id).await
    }

    pub async fn find_all(&self, params: &ProductQueryParams) -> Result<Vec<Product>> {
        self.product_repository.find_all(params).await
    }

    pub async fn count(&self, params: &ProductQueryParams) -> Result<i64> {
        self.product_repository.count(params).await
    }

    pub async fn exists_by_name(&self, name: &str) -> Result<bool> {
        self.product_repository.exists_by_name(name).await
    }

    pub async fn find_by_device_type(&self, device_type: &str) -> Result<Vec<Product>> {
        self.product_repository.find_by_device_type(device_type).await
    }

    pub async fn find_by_manufacturer(&self, manufacturer: &str) -> Result<Vec<Product>> {
        self.product_repository.find_by_manufacturer(manufacturer).await
    }

    pub async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Product>> {
        self.product_repository.search(keyword, limit).await
    }

    pub async fn get_stats_by_device_type(&self) -> Result<Vec<(String, i64)>> {
        self.product_repository.get_stats_by_device_type().await
    }

    pub async fn get_stats_by_manufacturer(&self) -> Result<Vec<(String, i64)>> {
        self.product_repository.get_stats_by_manufacturer().await
    }

    pub async fn find_with_filters(
        &self,
        name: Option<String>,
        manufacturer: Option<String>,
        device_type: Option<String>,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<Vec<Product>> {
        self.product_repository
            .find_with_filters(name, manufacturer, device_type, page, page_size)
            .await
    }

    pub async fn exists_by_name_excluding_id(&self, name: &str, exclude_id: &str) -> Result<bool> {
        self.product_repository
            .exists_by_name_excluding_id(name, exclude_id)
            .await
    }
}
