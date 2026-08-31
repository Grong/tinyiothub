// Open API Module
// Public API for AI platform integration

use crate::domains::admin::AdminState;
use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
};
use sha2::Digest;
use subtle::ConstantTimeEq;
use tinyiothub_web::response::ApiResponseBuilder;

use tinyiothub_web::api_response::ApiResponse;

/// Create open API router (public API, requires API Key)
pub fn create_open_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AdminState: axum::extract::FromRef<S>,
    std::sync::Arc<tinyiothub_authn::jwt::JwtService>: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/open/health", get(open_health))
        .route("/open/things", get(list_things))
        .route("/open/things/{id}", get(get_thing))
        .route("/open/things/{id}/properties", get(get_thing_properties))
        .route("/open/things/{id}/commands", get(list_commands))
        .route("/open/things/{id}/command", post(send_command))
        .route("/open/things/{id}/events", get(list_events))
        .route("/open/events", get(list_all_events))
        .fallback(handle_open_api)
}

/// Validate API Key
async fn validate_api_key(
    state: &AdminState,
    api_key: Option<String>,
) -> Result<
    (
        tinyiothub_storage::tenant::ApiKey,
        tinyiothub_storage::tenant::Tenant,
        String,
    ),
    StatusCode,
> {
    let raw_key = api_key.ok_or(StatusCode::UNAUTHORIZED)?;
    if raw_key.len() < 12 {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Lookup by the stored 12-char prefix (creation stores raw_key[..12]).
    // Pre-fix this passed the full key — equality could never hold and every
    // open-API call 401'd (pre-landing security review).
    let key = state
        .tenant_service
        .find_api_key_by_prefix(&raw_key[..12])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Verify the SECRET: constant-time SHA-256(presented) vs stored key_hash.
    // The prefix is a lookup hint, not a credential.
    let presented_hash = format!("{:x}", sha2::Sha256::digest(raw_key.as_bytes()));
    let hash_matches: bool = presented_hash.as_bytes().ct_eq(key.key_hash.as_bytes()).into();
    if !hash_matches {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if !key.is_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    if key.is_revoked {
        return Err(StatusCode::FORBIDDEN);
    }

    // Expiry: stored as "%Y-%m-%d %H:%M:%S" (creation format); accept RFC3339
    // too. An existing-but-unparseable expiry fails CLOSED.
    if let Some(expires) = &key.expires_at {
        let parsed = chrono::DateTime::parse_from_rfc3339(expires)
            .map(|d| d.with_timezone(&chrono::Utc))
            .or_else(|_| chrono::NaiveDateTime::parse_from_str(expires, "%Y-%m-%d %H:%M:%S").map(|d| d.and_utc()));
        match parsed {
            Ok(exp) if exp >= chrono::Utc::now() => {}
            _ => return Err(StatusCode::FORBIDDEN),
        }
    }

    // Resolve tenant_id from workspace for quota check
    let workspace = state
        .workspace_service
        .find_by_id(&key.workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let tenant = state
        .tenant_service
        .find_tenant_by_id(&workspace.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if tenant.status != "active" {
        return Err(StatusCode::FORBIDDEN);
    }

    let can_proceed = state
        .tenant_service
        .check_quota(&workspace.tenant_id, "api_call")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !can_proceed {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let workspace_id = key.workspace_id.clone();
    Ok((key, tenant, workspace_id))
}

/// Record API usage
async fn record_api_usage(
    state: &AdminState,
    workspace_id: &str,
    api_key_id: Option<&str>,
    method: &str,
    path: &str,
    status_code: StatusCode,
    latency_ms: i32,
) {
    let _ = state
        .tenant_service
        .record_api_usage(
            workspace_id,
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
async fn open_health(State(_state): State<AdminState>) -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "status": "ok",
        "service": "TinyIoTHub Open API",
        "version": "1.0.0",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// List things
async fn list_things(State(state): State<AdminState>, headers: HeaderMap) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let things: Vec<serde_json::Value> = state
        .db
        .list_open_things(&workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "name": row.name,
                "display_name": row.display_name,
                "category": row.category,
                "state": row.state,
                "created_at": row.created_at,
            })
        })
        .collect();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "GET",
        "/open/things",
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&things).unwrap_or_default()))
        .unwrap())
}

/// Get thing details
async fn get_thing(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let row = state
        .db
        .find_open_thing(&id, &workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = row.ok_or(StatusCode::NOT_FOUND)?;
    let thing = serde_json::json!({
        "id": row.id,
        "name": row.name,
        "display_name": row.display_name,
        "category": row.category,
        "address": row.address,
        "state": row.state,
        "protocol_type": row.protocol_type,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    });

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "GET",
        &format!("/open/things/{}", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&thing).unwrap_or_default()))
        .unwrap())
}

/// Get thing properties
async fn get_thing_properties(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    // Workspace-scoped like every sibling endpoint (pre-landing security
    // review: this was the one open-API endpoint missing the tenant guard —
    // any API key could read any workspace's live property values)
    let properties: Vec<_> = state
        .db
        .list_open_thing_properties(&id, &workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "name": row.name,
                "display_name": row.display_name,
                "data_type": row.data_type,
                "value": row.value,
                "unit": row.unit,
                "updated_at": row.updated_at,
            })
        })
        .collect::<Vec<_>>();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "GET",
        &format!("/open/things/{}/properties", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&properties).unwrap_or_default()))
        .unwrap())
}

/// List thing commands
async fn list_commands(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let commands: Vec<_> = state
        .db
        .list_open_thing_commands(&id, &workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "name": row.name,
                "display_name": row.display_name,
                "description": row.description,
                "parameters": row.parameters,
            })
        })
        .collect::<Vec<_>>();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "GET",
        &format!("/open/things/{}/commands", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&commands).unwrap_or_default()))
        .unwrap())
}

/// Send thing command
async fn send_command(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let command_name = payload
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let command_params = payload.get("params").cloned();

    // Verify the thing exists IN THIS WORKSPACE and is a device (T1)
    let thing: Option<(String, String)> = state
        .db
        .find_open_thing_type(&id, &workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((_, thing_type)) = thing else {
        return Err(StatusCode::NOT_FOUND);
    };
    if thing_type != "device" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify the action is registered on the thing
    let registered: bool = state
        .db
        .count_thing_action_by_name(&id, command_name)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);
    if !registered {
        return Err(StatusCode::NOT_FOUND);
    }

    // Dispatch through the real command channel (same path as the
    // invoke_action confirm flow), not a definitions-table INSERT.
    let cmd_id = uuid::Uuid::new_v4().to_string();
    let dispatched = state.data_server().cloned().map(|data_server| {
        let cmd = tinyiothub_core::models::device_command::DeviceCommand {
            id: cmd_id.clone(),
            thing_id: id.clone(),
            name: command_name.to_string(),
            display_name: None,
            description: None,
            parameters: command_params.as_ref().map(|p| p.to_string()),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        data_server.execute_command(cmd)
    });
    let status_str = match dispatched {
        Some(Ok(())) => "executed",
        Some(Err(_)) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        None => "simulated",
    };

    let result = serde_json::json!({
        "command_id": cmd_id,
        "status": status_str,
        "message": "Command dispatched"
    });

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "POST",
        &format!("/open/things/{}/command", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&result).unwrap_or_default()))
        .unwrap())
}

/// Get thing events
async fn list_events(
    State(state): State<AdminState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let events: Vec<_> = state
        .db
        .list_open_thing_events(&id, &workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "event_type": row.event_type,
                "event_level": row.event_level,
                "message": row.title.unwrap_or_default(),
                "created_at": row.created_at,
            })
        })
        .collect::<Vec<_>>();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
        Some(&key.id),
        "GET",
        &format!("/open/things/{}/events", id),
        StatusCode::OK,
        latency_ms,
    )
    .await;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&events).unwrap_or_default()))
        .unwrap())
}

/// Get all events
async fn list_all_events(State(state): State<AdminState>, headers: HeaderMap) -> Result<Response<Body>, StatusCode> {
    let start = std::time::Instant::now();

    let api_key = extract_api_key_header(&headers);
    let (key, _tenant, workspace_id) = validate_api_key(&state, api_key).await?;

    let events: Vec<_> = state
        .db
        .list_open_events(&workspace_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "event_type": row.event_type,
                "event_level": row.event_level,
                "message": row.title.unwrap_or_default(),
                "thing_id": row.thing_id,
                "created_at": row.created_at,
            })
        })
        .collect::<Vec<_>>();

    let latency_ms = start.elapsed().as_millis() as i32;
    record_api_usage(
        &state,
        &workspace_id,
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
        .body(Body::from(serde_json::to_string(&events).unwrap_or_default()))
        .unwrap())
}

/// Extract API key from request headers
fn extract_api_key_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-API-Key")
        .or_else(|| headers.get("x-api-key"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Fallback handler
async fn handle_open_api() -> Json<ApiResponse<()>> {
    ApiResponseBuilder::error("API endpoint not found")
}
