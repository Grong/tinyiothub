// Open API Module
// Public API for AI platform integration

use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use sqlx::Row;

use crate::dto::entity::tenant::{ApiKey, Tenant};
use crate::dto::response::{api_response::ApiResponse, builder::ApiResponseBuilder};
use crate::shared::app_state::AppState;

/// Create open API router (public API, requires API Key)
pub fn create_open_router() -> Router<AppState> {
    Router::new()
        .route("/open/health", get(open_health))
        .route("/open/devices", get(list_devices))
        .route("/open/devices/:id", get(get_device))
        .route("/open/devices/:id/properties", get(get_device_properties))
        .route("/open/devices/:id/commands", get(list_commands))
        .route("/open/devices/:id/command", post(send_command))
        .route("/open/devices/:id/events", get(list_events))
        .route("/open/events", get(list_all_events))
        .fallback(handle_open_api)
}

/// Validate API Key
async fn validate_api_key(
    state: &AppState,
    api_key: Option<String>,
) -> Result<(ApiKey, Tenant), StatusCode> {
    let key = api_key.ok_or(StatusCode::UNAUTHORIZED)?;

    let key = ApiKey::find_by_prefix(&state.database, &key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if !key.is_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    if key.is_revoked {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Some(expires) = &key.expires_at {
        if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires) {
            if exp < chrono::Utc::now() {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    let tenant = Tenant::find_by_id(&state.database, &key.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if tenant.status != "active" {
        return Err(StatusCode::FORBIDDEN);
    }

    let can_proceed = Tenant::check_quota(&state.database, &key.tenant_id, "api_call")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !can_proceed {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok((key, tenant))
}

/// Record API usage
async fn record_api_usage(
    state: &AppState,
    tenant_id: &str,
    api_key_id: Option<&str>,
    method: &str,
    path: &str,
    status_code: StatusCode,
    latency_ms: i32,
) {
    let _ = ApiKey::record_usage(
        &state.database,
        tenant_id,
        api_key_id,
        method,
        path,
        status_code.as_u16() as i32,
        latency_ms,
        None,
    )
    .await;
}

/// Open API health check
async fn open_health(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "service": "TinyIoTHub Open API",
        "version": "1.0.0",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// List devices
async fn list_devices(State(state): State<AppState>) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = format!(
        "SELECT id, name, display_name, device_type, state, created_at FROM devices ORDER BY created_at DESC LIMIT 100"
    );

    let rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "id": row.try_get::<String, _>("id")?,
                "name": row.try_get::<String, _>("name")?,
                "display_name": row.try_get::<Option<String>, _>("display_name")?,
                "device_type": row.try_get::<Option<String>, _>("device_type")?,
                "state": row.try_get::<i32, _>("state")?,
                "created_at": row.try_get::<String, _>("created_at")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let devices: Vec<_> = rows.into_iter().map(|r| r).collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        "/open/devices",
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&devices).unwrap_or_default(),
        ))
        .unwrap())
}

/// Get device details
async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = format!(
        "SELECT id, name, display_name, device_type, address, state, protocol_type, created_at, updated_at FROM devices WHERE id = '{}' LIMIT 1",
        id
    );

    let mut rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "id": row.try_get::<String, _>("id")?,
                "name": row.try_get::<String, _>("name")?,
                "display_name": row.try_get::<Option<String>, _>("display_name")?,
                "device_type": row.try_get::<Option<String>, _>("device_type")?,
                "address": row.try_get::<Option<String>, _>("address")?,
                "state": row.try_get::<i32, _>("state")?,
                "protocol_type": row.try_get::<Option<String>, _>("protocol_type")?,
                "created_at": row.try_get::<String, _>("created_at")?,
                "updated_at": row.try_get::<String, _>("updated_at")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let device = rows.pop().ok_or(StatusCode::NOT_FOUND)?;

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        &format!("/open/devices/{}", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&device).unwrap_or_default(),
        ))
        .unwrap())
}

/// Get device properties
async fn get_device_properties(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = format!(
        "SELECT name, display_name, data_type, value, unit, updated_at FROM device_properties WHERE device_id = '{}' ORDER BY created_at DESC",
        id
    );

    let rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "name": row.try_get::<String, _>("name")?,
                "display_name": row.try_get::<Option<String>, _>("display_name")?,
                "data_type": row.try_get::<String, _>("data_type")?,
                "value": row.try_get::<Option<String>, _>("value")?,
                "unit": row.try_get::<Option<String>, _>("unit")?,
                "updated_at": row.try_get::<String, _>("updated_at")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let properties: Vec<_> = rows.into_iter().map(|r| r).collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        &format!("/open/devices/{}/properties", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&properties).unwrap_or_default(),
        ))
        .unwrap())
}

/// List device commands
async fn list_commands(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = format!(
        "SELECT id, name, display_name, description, command_type FROM device_commands WHERE device_id = '{}' ORDER BY created_at DESC",
        id
    );

    let rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "id": row.try_get::<String, _>("id")?,
                "name": row.try_get::<String, _>("name")?,
                "display_name": row.try_get::<Option<String>, _>("display_name")?,
                "description": row.try_get::<Option<String>, _>("description")?,
                "command_type": row.try_get::<String, _>("command_type")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let commands: Vec<_> = rows.into_iter().map(|r| r).collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        &format!("/open/devices/{}/commands", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&commands).unwrap_or_default(),
        ))
        .unwrap())
}

/// Send device command
async fn send_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let command_name = payload
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let command_params = payload
        .get("params")
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    let cmd_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let sql = format!(
        r#"
        INSERT INTO device_commands (id, device_id, name, command_type, parameters, status, created_at, updated_at)
        VALUES ('{}', '{}', '{}', 'custom', '{}', 'pending', '{}', '{}')
    "#,
        cmd_id,
        id,
        command_name,
        command_params.unwrap_or_default(),
        now,
        now
    );

    state
        .database
        .execute(&sql)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let result = serde_json::json!({
        "command_id": cmd_id,
        "status": "pending",
        "message": "Command sent successfully"
    });

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "POST",
        &format!("/open/devices/{}/command", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&result).unwrap_or_default(),
        ))
        .unwrap())
}

/// Get device events
async fn list_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = format!(
        "SELECT id, event_type, event_level, message, created_at FROM events WHERE device_id = '{}' ORDER BY created_at DESC LIMIT 100",
        id
    );

    let rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "id": row.try_get::<String, _>("id")?,
                "event_type": row.try_get::<String, _>("event_type")?,
                "event_level": row.try_get::<String, _>("event_level")?,
                "message": row.try_get::<String, _>("message")?,
                "created_at": row.try_get::<String, _>("created_at")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let events: Vec<_> = rows.into_iter().map(|r| r).collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        &format!("/open/devices/{}/events", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&events).unwrap_or_default(),
        ))
        .unwrap())
}

/// Get all events
async fn list_all_events(State(state): State<AppState>) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let (key, tenant) = validate_api_key(&state, None).await?;

    let sql = "SELECT id, event_type, event_level, message, device_id, created_at FROM events ORDER BY created_at DESC LIMIT 100".to_string();

    let rows = state
        .database
        .query(&sql, |row| {
            Ok(serde_json::json!({
                "id": row.try_get::<String, _>("id")?,
                "event_type": row.try_get::<String, _>("event_type")?,
                "event_level": row.try_get::<String, _>("event_level")?,
                "message": row.try_get::<String, _>("message")?,
                "device_id": row.try_get::<Option<String>, _>("device_id")?,
                "created_at": row.try_get::<String, _>("created_at")?,
            }))
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let events: Vec<_> = rows.into_iter().map(|r| r).collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &tenant.id,
        Some(&key.id),
        "GET",
        "/open/events",
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&events).unwrap_or_default(),
        ))
        .unwrap())
}

/// Fallback handler
async fn handle_open_api() -> Json<ApiResponse<()>> {
    ApiResponseBuilder::error("API endpoint not found")
}
