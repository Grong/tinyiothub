use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::AppState;
use crate::types::{Driver, PaginatedList, PaginationParams};

const CACHE_STALE_HEADER: &str = "X-Cache-Stale";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/drivers", axum::routing::get(list_drivers))
        .route("/drivers/{id}", axum::routing::get(get_driver))
}

async fn list_drivers(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Response, (StatusCode, Json<tinyiothub_web::response::ApiResponse<()>>)> {
    if let Err(e) = params.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            ApiResponseBuilder::error_with_code(400, format!("Invalid pagination: {}", e)),
        ));
    }

    // 按资源自身判断冷缓存：None = 该类数据未加载（比全局 is_cold 更精确）
    let cached = state.cache.get_drivers();
    let is_cold = matches!(cached, Ok(None));

    match cached {
        Ok(Some(items)) => {
            let filtered = filter_drivers(&items, &params);
            let total = filtered.len();
            let offset = params.offset();
            let page_items: Vec<Driver> = filtered
                .into_iter()
                .skip(offset)
                .take(params.per_page)
                .filter_map(|v| serde_json::from_value::<Driver>(v).ok())
                .collect();

            let mut headers = HeaderMap::new();
            if is_cold {
                headers.insert(CACHE_STALE_HEADER, "true".parse().unwrap());
            }
            headers.insert("X-Total-Count", total.to_string().parse().unwrap());
            headers.insert("X-Page", params.page.to_string().parse().unwrap());
            headers.insert("X-Per-Page", params.per_page.to_string().parse().unwrap());

            let response =
                ApiResponseBuilder::success(PaginatedList::new(page_items, total, params.page, params.per_page));
            Ok((headers, response).into_response())
        }
        Ok(None) => {
            let mut headers = HeaderMap::new();
            headers.insert(CACHE_STALE_HEADER, "true".parse().unwrap());
            let response = ApiResponseBuilder::success(PaginatedList::new(
                Vec::<Driver>::new(),
                0,
                params.page,
                params.per_page,
            ));
            Ok((headers, response).into_response())
        }
        Err(e) => {
            tracing::warn!("Sled read error for drivers: {}", e);
            Err((
                StatusCode::BAD_GATEWAY,
                ApiResponseBuilder::error_with_code(502, "Cache unavailable"),
            ))
        }
    }
}

async fn get_driver(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, (StatusCode, Json<tinyiothub_web::response::ApiResponse<()>>)> {
    // 按资源自身判断冷缓存：None = 该类数据未加载（比全局 is_cold 更精确）
    let cached = state.cache.get_drivers();
    let is_cold = matches!(cached, Ok(None));

    match cached {
        Ok(Some(items)) => {
            match items
                .iter()
                .find(|item| item.get("id").and_then(|v| v.as_str()) == Some(&id))
            {
                Some(v) => match serde_json::from_value::<Driver>(v.clone()) {
                    Ok(d) => {
                        let mut headers = HeaderMap::new();
                        if is_cold {
                            headers.insert(CACHE_STALE_HEADER, "true".parse().unwrap());
                        }
                        Ok((headers, ApiResponseBuilder::success(d)).into_response())
                    }
                    Err(e) => Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ApiResponseBuilder::error_with_code(500, format!("Data error: {}", e)),
                    )),
                },
                None => Err((
                    StatusCode::NOT_FOUND,
                    ApiResponseBuilder::error_with_code(40401, "driver not found"),
                )),
            }
        }
        // 缓存未加载 ≠ 驱动不存在：返回 503 + X-Cache-Stale，与真正的 404 区分
        // （经 Ok 返回是因为本函数 Err 变体声明为 (StatusCode, Json) 二元组）
        Ok(None) => {
            let mut headers = HeaderMap::new();
            headers.insert(CACHE_STALE_HEADER, "true".parse().unwrap());
            Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                headers,
                ApiResponseBuilder::error_with_code::<()>(50301, "driver cache not loaded, retry later"),
            )
                .into_response())
        }
        Err(_) => Err((
            StatusCode::BAD_GATEWAY,
            ApiResponseBuilder::error_with_code(502, "cache unavailable"),
        )),
    }
}

fn filter_drivers(items: &[serde_json::Value], params: &PaginationParams) -> Vec<serde_json::Value> {
    let search_lower = params.search.as_ref().map(|s| s.to_lowercase());

    items
        .iter()
        .filter(|item| {
            if let Some(ref proto) = params.protocol
                && item.get("protocol").and_then(|v| v.as_str()) != Some(proto.as_str())
            {
                return false;
            }

            if let Some(ref search) = search_lower {
                let matches = [
                    item.get("name").and_then(|v| v.as_str()),
                    item.get("description").and_then(|v| v.as_str()),
                ]
                .into_iter()
                .flatten()
                .any(|s| s.to_lowercase().contains(search));
                if !matches {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}
