use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tinyiothub_web::response::ApiResponseBuilder;

use crate::AppState;
use crate::types::{PaginatedList, PaginationParams, Template};

const CACHE_STALE_HEADER: &str = "X-Cache-Stale";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/templates", axum::routing::get(list_templates))
        .route("/templates/{name}", axum::routing::get(get_template))
}

async fn list_templates(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Response, (StatusCode, Json<tinyiothub_web::response::ApiResponse<()>>)> {
    if let Err(e) = params.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            ApiResponseBuilder::error_with_code(400, format!("Invalid pagination: {}", e)),
        ));
    }

    let is_cold = state.cache.is_cold();

    match state.cache.get_templates() {
        Ok(Some(items)) => {
            let filtered = filter_templates(&items, &params);
            let total = filtered.len();
            let offset = params.offset();
            let page_items: Vec<Template> = filtered
                .into_iter()
                .skip(offset)
                .take(params.per_page)
                .filter_map(|v| serde_json::from_value::<Template>(v).ok())
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
                Vec::<Template>::new(),
                0,
                params.page,
                params.per_page,
            ));
            Ok((headers, response).into_response())
        }
        Err(e) => {
            tracing::warn!("Sled read error: {}", e);
            Err((
                StatusCode::BAD_GATEWAY,
                ApiResponseBuilder::error_with_code(502, "Cache unavailable"),
            ))
        }
    }
}

async fn get_template(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, (StatusCode, Json<tinyiothub_web::response::ApiResponse<()>>)> {
    let is_cold = state.cache.is_cold();

    match state.cache.get_templates() {
        Ok(Some(items)) => {
            match items
                .iter()
                .find(|item| item.get("name").and_then(|v| v.as_str()) == Some(&name))
            {
                Some(v) => match serde_json::from_value::<Template>(v.clone()) {
                    Ok(t) => {
                        let mut headers = HeaderMap::new();
                        if is_cold {
                            headers.insert(CACHE_STALE_HEADER, "true".parse().unwrap());
                        }
                        Ok((headers, ApiResponseBuilder::success(t)).into_response())
                    }
                    Err(e) => Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ApiResponseBuilder::error_with_code(500, format!("Data error: {}", e)),
                    )),
                },
                None => Err((
                    StatusCode::NOT_FOUND,
                    ApiResponseBuilder::error_with_code(40401, "template not found"),
                )),
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            ApiResponseBuilder::error_with_code(40401, "template not found"),
        )),
        Err(_) => Err((
            StatusCode::BAD_GATEWAY,
            ApiResponseBuilder::error_with_code(502, "cache unavailable"),
        )),
    }
}

fn filter_templates(items: &[serde_json::Value], params: &PaginationParams) -> Vec<serde_json::Value> {
    let search_lower = params.search.as_ref().map(|s| s.to_lowercase());

    items
        .iter()
        .filter(|item| {
            if let Some(ref cat) = params.category {
                if item.get("category").and_then(|v| v.as_str()) != Some(cat.as_str()) {
                    return false;
                }
            }

            if let Some(ref proto) = params.protocol {
                if item.get("protocol_type").and_then(|v| v.as_str()) != Some(proto.as_str()) {
                    return false;
                }
            }

            if let Some(ref search) = search_lower {
                let name_match = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase().contains(search))
                    .unwrap_or(false);
                let display_match = item
                    .get("display_name")
                    .and_then(|v| v.get("zh").or(v.get("en")).and_then(|s| s.as_str()))
                    .map(|s| s.to_lowercase().contains(search))
                    .unwrap_or(false);
                let tags_match = item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .any(|s| s.to_lowercase().contains(search))
                    })
                    .unwrap_or(false);
                if !name_match && !display_match && !tags_match {
                    return false;
                }
            }

            true
        })
        .cloned()
        .collect()
}
