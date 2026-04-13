use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    dto::{
        entity::product::{CreateProductRequest, Product, UpdateProductRequest},
        request::pagination::PaginationQuery,
        response::ApiResponse,
    },
    shared::{app_state::AppState, security::jwt::Claims},
};

#[derive(Deserialize)]
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
        .route("/:id", get(get_product).put(update_product).delete(delete_product))
}

/// 获取产品列表
async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductQuery>,
    _claims: Claims,
) -> Json<ApiResponse<Vec<Product>>> {
    match Product::find_with_filters(
        state.database(),
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
            ApiResponse::success(products)
        }
        Err(e) => {
            tracing::error!("Failed to list products: {}", e);
            ApiResponse::error("获取产品列表失败".to_string())
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
        return ApiResponse::error("产品名称不能为空".to_string());
    }

    // 检查产品名称是否已存在
    match Product::exists_by_name(state.database(), &request.name).await {
        Ok(true) => {
            return ApiResponse::error("产品名称已存在".to_string());
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check product name existence: {}", e);
            return ApiResponse::error("创建产品失败".to_string());
        }
    }

    // 创建产品
    match Product::create(state.database(), &request).await {
        Ok(product) => {
            tracing::info!("Product created: {}", product.name);
            ApiResponse::success(product)
        }
        Err(e) => {
            tracing::error!("Failed to create product: {}", e);
            ApiResponse::error("创建产品失败".to_string())
        }
    }
}

/// 获取产品详情
async fn get_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<Product>> {
    match Product::find_by_id(state.database(), &id).await {
        Ok(Some(product)) => {
            tracing::debug!("Retrieved product: {}", product.name);
            ApiResponse::success(product)
        }
        Ok(None) => ApiResponse::error("产品不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to get product {}: {}", id, e);
            ApiResponse::error("获取产品信息失败".to_string())
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
            return ApiResponse::error("产品名称不能为空".to_string());
        }

        // 检查产品名称是否已被其他产品使用
        match Product::exists_by_name_excluding_id(state.database(), name, &id).await {
            Ok(true) => {
                return ApiResponse::error("产品名称已存在".to_string());
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!("Failed to check product name uniqueness: {}", e);
                return ApiResponse::error("更新产品失败".to_string());
            }
        }
    }

    match Product::update(state.database(), &id, &request).await {
        Ok(product) => {
            tracing::info!("Product updated: {}", product.name);
            ApiResponse::success(product)
        }
        Err(sqlx::Error::RowNotFound) => ApiResponse::error("产品不存在".to_string()),
        Err(e) => {
            tracing::error!("Failed to update product {}: {}", id, e);
            ApiResponse::error("更新产品失败".to_string())
        }
    }
}

/// 删除产品
async fn delete_product(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _claims: Claims,
) -> Json<ApiResponse<bool>> {
    match Product::delete(state.database(), &id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                tracing::info!("Product deleted: {}", id);
                ApiResponse::success(true)
            } else {
                ApiResponse::error("产品不存在".to_string())
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete product {}: {}", id, e);
            ApiResponse::error("删除产品失败".to_string())
        }
    }
}
