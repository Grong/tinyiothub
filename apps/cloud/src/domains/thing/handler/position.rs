//! E4d 物坐标 API：`GET /v1/things/{id}/position`（工作区读取）与
//! `PUT /v1/things/{id}/position`（admin 人工校正）。
//!
//! 语义（spec §2.3 / 裁定 #6）：
//! - body `{x, y, level?, heading?, trust?}`；坐标必须有限且落在物所属图
//!   （无 `map_id` 时取工作区默认图）的 bounds 内，越界/无图 → 400；
//! - 写坐标列 + `position_updated_at=now`，保留物既有 `map_id` 与
//!   `zone_ref`/`position_ingest`（API 校正不受驱动摄取门限制）；
//! - 物不属于调用方工作区 → 404（不可见，而非 403）。

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tinyiothub_core::models::spatial::{LocationTrust, MapRecord, ThingPosition};
use tinyiothub_runtime::maps::zone::within_bounds;
use tinyiothub_web::middleware::workspace::{AuthClaims, WorkspaceScope};
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::state::AppState;

type ApiSuccess = (StatusCode, Json<ApiResponse<Value>>);
type ApiError = (StatusCode, Json<ApiResponse<Value>>);
type ApiResult = Result<ApiSuccess, ApiError>;

fn ok(data: impl Serialize) -> ApiResult {
    Ok((
        StatusCode::OK,
        ApiResponseBuilder::success(serde_json::to_value(data).unwrap_or(Value::Null)),
    ))
}

fn fail(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        ApiResponseBuilder::error_with_code(status.as_u16() as i32, message),
    )
}

fn db_error(context: &str, error: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %error, context, "position API storage error");
    fail(StatusCode::INTERNAL_SERVER_ERROR, "storage error")
}

async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), ApiError> {
    match state.db.count_user_admin_roles(user_id).await {
        Ok(n) if n > 0 => Ok(()),
        Ok(_) => Err(fail(StatusCode::FORBIDDEN, "Access denied: admin role required")),
        Err(e) => {
            tracing::warn!(error = %e, user = %user_id, "admin role check failed");
            Err(fail(StatusCode::INTERNAL_SERVER_ERROR, "Permission check failed"))
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionView {
    map_id: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    level: Option<i64>,
    heading: Option<f64>,
    trust: LocationTrust,
    updated_at: Option<String>,
}

impl From<ThingPosition> for PositionView {
    fn from(position: ThingPosition) -> Self {
        Self {
            map_id: position.map_id,
            x: position.x,
            y: position.y,
            level: position.level,
            heading: position.heading,
            trust: position.trust,
            updated_at: position.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutPositionRequest {
    pub x: f64,
    pub y: f64,
    pub level: Option<i64>,
    pub heading: Option<f64>,
    pub trust: Option<LocationTrust>,
}

/// 物所属图：既有 `map_id`，否则工作区默认图。
async fn position_map(state: &AppState, ws: &str, map_id: Option<&str>) -> Result<Option<MapRecord>, ApiError> {
    let result = match map_id {
        Some(id) => state.db.find_map(ws, id).await,
        None => state.db.find_default_map(ws).await,
    };
    result.map_err(|e| db_error("load position map", e))
}

pub async fn get_position(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
) -> ApiResult {
    let ws = ws.unwrap_or_default();
    match state.db.get_thing_position(&ws, &id).await {
        Ok(Some(position)) => ok(PositionView::from(position)),
        Ok(None) => Err(fail(StatusCode::NOT_FOUND, "thing not found")),
        Err(e) => Err(db_error("get thing position", e)),
    }
}

pub async fn put_position(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path(id): Path<String>,
    Json(req): Json<PutPositionRequest>,
) -> ApiResult {
    ensure_admin(&state, &claims.user_id).await?;
    let ws = match ws {
        Some(ws) if !ws.trim().is_empty() => ws,
        _ => {
            return Err(fail(
                StatusCode::FORBIDDEN,
                "workspace id is required for position management",
            ));
        }
    };

    if !req.x.is_finite() || !req.y.is_finite() {
        return Err(fail(StatusCode::BAD_REQUEST, "position coordinates must be finite"));
    }
    if req.heading.is_some_and(|h| !h.is_finite()) {
        return Err(fail(StatusCode::BAD_REQUEST, "heading must be finite"));
    }

    // 行存在 = 物存在且属于该工作区（否则 None → 404）。
    let Some(current) = state
        .db
        .get_thing_position(&ws, &id)
        .await
        .map_err(|e| db_error("load thing position", e))?
    else {
        return Err(fail(StatusCode::NOT_FOUND, "thing not found"));
    };

    let Some(map) = position_map(&state, &ws, current.map_id.as_deref()).await? else {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "no map available for position (thing has no map and workspace has no default map)",
        ));
    };
    if !within_bounds(&map.bounds, req.x, req.y) {
        return Err(fail(StatusCode::BAD_REQUEST, "position is outside map bounds"));
    }

    let position = ThingPosition {
        // 保留既有地图归属（默认图为 None 时读侧按默认图解析）。
        map_id: current.map_id,
        x: Some(req.x),
        y: Some(req.y),
        level: req.level,
        heading: req.heading,
        // 人工校正默认可信；显式 trust 可降级。
        trust: req.trust.unwrap_or(LocationTrust::Trusted),
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
    };

    if let Some(zones) = &state.zones {
        // 同一实例写穿缓存（DataServer/watch 立即看到校正后的坐标）。
        zones
            .set_position(&ws, &id, &position)
            .await
            .map_err(|e| db_error("set thing position", e))?;
    } else {
        match state.db.set_thing_position(&ws, &id, &position).await {
            Ok(true) => {}
            Ok(false) => return Err(fail(StatusCode::NOT_FOUND, "thing not found")),
            Err(e) => return Err(db_error("set thing position", e)),
        }
    }

    ok(PositionView::from(position))
}
