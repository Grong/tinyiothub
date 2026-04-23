use crate::shared::security::jwt::Claims;
use tinyiothub_web::response::ApiResponseBuilder;
use crate::dto::entity::product::{CreateProductRequest, Product, UpdateProductRequest};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router
};
use serde::Deserialize;

use crate::{
    dto::{
        request::pagination::PaginationQuery,
        response::ApiResponse
    },
    shared::{app_state::AppState}
};

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(rename_all = "snake_case")]
pub struct ProductQuery {
    pub device_type: Option<String>,
    pub manufacturer: Option<String>,
    pub protocol: Option<String>,
    pub search: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_products).post(create_product))
        .route("/{id}", get(get_product).put(update_product).delete(delete_product))
}

/// 获取产品列表
async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<Product>>> {
    match state.product_service.find_with_filters(
        query.search,
        None, // manufacturer
        None, // device_type
        Some(query.pagination.page.unwrap_or(1)),
        Some(query.pagination.page_size.unwrap_or(20)),
    )
    .await
    {
        Ok(products) => {
            tracing::debug!("Retrieved {} products", products.len());
            ApiResponseBuilder::success(products)
        }
        Err(e) => {
            tracing::error!("Failed to list products: {}", e);
            ApiResponseBuilder::error("获取产品列表失败".to_string())
        }
    }
}

/// 创建产品
async fn create_product(
    State(state): State<AppState>,
    _claims: Claims,
    Json(request): Json<CreateProductRequest>,
) -> Json<ApiResponse<Product>> {
    // 验证输入
    if request.name.trim().is_empty() {
        return ApiResponseBuilder::error("产品名称不能为空".to_string());
    }

    // 检查产品名称是否已存在
    match state.product_service.exists_by_name(&request.name).await {
        Ok(true) => {
            return ApiResponseBuilder::error("产品名称已存在".to_string());
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check product name existence: {}", e);
            return ApiResponseBuilder::error("创建产品失败".to_string());
        }
    }

    // 创建产品
    match state.product_service.create(&request).await {
        Ok(product) => {
            tracing::info!("Product created: {}", product.name);
            ApiResponseBuilder::success(product)
        }
        Err(e) => {
            tracing::error!("Failed to create product: {}", e);
            ApiResponseBuilder::error("创建产品失败".to_string())
        }
    }
}

/// 获取产品详情
async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Product>> {
    match state.product_service.find_by_id(&id).await {
        Ok(Some(product)) => {
            tracing::debug!("Retrieved product: {}", product.name);
            ApiResponseBuilder::success(product)
        }
        Ok(None) => ApiResponseBuilder::error("产品不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to get product {}: {}", id, e);
            ApiResponseBuilder::error("获取产品信息失败".to_string())
        }
    }
}

/// 更新产品
async fn update_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
    Json(request): Json<UpdateProductRequest>,
) -> Json<ApiResponse<Product>> {
    // 验证输入
    if let Some(name) = &request.name {
        if name.trim().is_empty() {
            return ApiResponseBuilder::error("产品名称不能为空".to_string());
        }

        // 检查产品名称是否已被其他产品使用
        match state.product_service.exists_by_name_excluding_id(name, &id).await {
            Ok(true) => {
                return ApiResponseBuilder::error("产品名称已存在".to_string());
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Failed to check product name uniqueness: {}", e);
                return ApiResponseBuilder::error("更新产品失败".to_string());
            }
        }
    }

    match state.product_service.update(&id, &request).await {
        Ok(product) => {
            tracing::info!("Product updated: {}", product.name);
            ApiResponseBuilder::success(product)
        }
        Err(crate::shared::error::Error::NotFound) => ApiResponseBuilder::error("产品不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to update product {}: {}", id, e);
            ApiResponseBuilder::error("更新产品失败".to_string())
        }
    }
}

/// 删除产品
async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match state.product_service.delete(&id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                tracing::info!("Product deleted: {}", id);
                ApiResponseBuilder::success(true)
            } else {
                ApiResponseBuilder::error("产品不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete product {}: {}", id, e);
            ApiResponseBuilder::error("删除产品失败".to_string())
        }
    }
}
