//! E4a install / declaration routes, mounted under the existing
//! `/v1/drivers` prefix.
//!
//! `install` and `acceptance` execute native driver code inside the hub
//! process, so both require an admin role. Every route requires a resolved,
//! non-empty workspace: the empty string is the built-in namespaced slot.

use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tinyiothub_core::driver::declaration::AcceptanceReportRecord;
use tinyiothub_runtime::acceptance::runner::run_profiles;
use tinyiothub_runtime::access::AccessError;
use tinyiothub_web::middleware::workspace::{AuthClaims, WorkspaceScope};

use crate::state::AppState;

type ApiResult = (StatusCode, Json<Value>);

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/install", post(install_driver))
        .route("/declarations", get(list_declarations))
        .route("/{driver}/declaration", get(get_declaration))
        .route("/{driver}/acceptance", post(run_acceptance))
}

fn fail(status: StatusCode, message: impl Into<String>) -> ApiResult {
    (status, Json(json!({"success": false, "error": message.into()})))
}

fn access_failure(err: &AccessError) -> ApiResult {
    let status = match err {
        AccessError::NotFound(_, _) => StatusCode::NOT_FOUND,
        AccessError::Store(_) | AccessError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AccessError::Abi(_) => StatusCode::UNPROCESSABLE_ENTITY,
        AccessError::Manifest(_)
        | AccessError::Json(_)
        | AccessError::MissingManifest(_)
        | AccessError::Validation(_) => StatusCode::BAD_REQUEST,
    };
    fail(status, err.to_string())
}

/// Require a concrete workspace. `WorkspaceScope` is `None` or empty when the
/// token carries no workspace; writing then would land in the built-in slot.
fn scoped_workspace(ws: WorkspaceScope) -> Result<String, ApiResult> {
    match ws.0 {
        Some(ws) if !ws.trim().is_empty() => Ok(ws),
        _ => Err(fail(
            StatusCode::FORBIDDEN,
            "workspace id is required for driver declaration operations",
        )),
    }
}

/// Admin gate for native-code execution. Uses the same role source as the
/// agent/admin handler pattern (`roles.is_administrator`), failing closed.
async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), ApiResult> {
    match state.db.count_user_admin_roles(user_id).await {
        Ok(n) if n > 0 => Ok(()),
        Ok(_) => Err(fail(StatusCode::FORBIDDEN, "Access denied: admin role required")),
        Err(e) => {
            tracing::warn!(error = %e, user = %user_id, "admin role check failed");
            Err(fail(StatusCode::INTERNAL_SERVER_ERROR, "Permission check failed"))
        }
    }
}

fn access_missing() -> ApiResult {
    fail(StatusCode::SERVICE_UNAVAILABLE, "access service not wired")
}

/// `POST /v1/drivers/install` — multipart fields `manifest` (required) and
/// optional `binary`. Admin only: the binary is loaded in-process.
async fn install_driver(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    mut multipart: Multipart,
) -> ApiResult {
    if let Err(e) = ensure_admin(&state, &claims.user_id).await {
        return e;
    }
    let ws = match scoped_workspace(WorkspaceScope(ws)) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    let Some(access) = state.access.clone() else {
        return access_missing();
    };
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let mut manifest_seen = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("manifest") => match field.text().await {
                Ok(text) => {
                    if std::fs::write(dir.path().join("driver.manifest.json"), text).is_err() {
                        return fail(StatusCode::INTERNAL_SERVER_ERROR, "write manifest failed");
                    }
                    manifest_seen = true;
                }
                Err(e) => return fail(StatusCode::BAD_REQUEST, e.to_string()),
            },
            Some("binary") => match field.bytes().await {
                Ok(bytes) => {
                    if std::fs::write(dir.path().join("driver.so"), bytes).is_err() {
                        return fail(StatusCode::INTERNAL_SERVER_ERROR, "write binary failed");
                    }
                }
                Err(e) => return fail(StatusCode::BAD_REQUEST, e.to_string()),
            },
            _ => {}
        }
    }
    if !manifest_seen {
        return fail(StatusCode::BAD_REQUEST, "missing manifest field");
    }
    match access.install_dir(&ws, dir.path()).await {
        Ok(pkg) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "driver": pkg.manifest.identity.driver,
                "version": pkg.manifest.identity.version,
                "manifestHash": pkg.manifest_hash,
                "warnings": pkg.warnings,
            })),
        ),
        Err(e) => access_failure(&e),
    }
}

/// `GET /v1/drivers/declarations` — active packages visible to the caller's
/// workspace (installed packages plus built-in fallbacks).
async fn list_declarations(State(state): State<AppState>, WorkspaceScope(ws): WorkspaceScope) -> ApiResult {
    let ws = match scoped_workspace(WorkspaceScope(ws)) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    let Some(access) = state.access.clone() else {
        return access_missing();
    };
    let items: Vec<Value> = access
        .manifests
        .list(&ws)
        .into_iter()
        .map(|p| {
            json!({
                "driver": p.manifest.identity.driver,
                "version": p.manifest.identity.version,
                "protocol": p.manifest.identity.protocol,
                "manifestHash": p.manifest_hash,
                "dynamicCapabilities": p.manifest.identity.dynamic_capabilities,
            })
        })
        .collect();
    (StatusCode::OK, Json(json!({"success": true, "data": items})))
}

/// `GET /v1/drivers/{driver}/declaration` — the active package manifest.
async fn get_declaration(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> ApiResult {
    let ws = match scoped_workspace(WorkspaceScope(ws)) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    let Some(access) = state.access.clone() else {
        return access_missing();
    };
    match access.get_manifest(&ws, &driver) {
        Some(pkg) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "data": serde_json::to_value(&pkg.manifest).unwrap_or_default(),
            })),
        ),
        None => fail(StatusCode::NOT_FOUND, "driver not installed"),
    }
}

/// `POST /v1/drivers/{driver}/acceptance` — run every auto profile against the
/// installed package and persist the aggregate report. Admin only: the runner
/// loads the package binary.
async fn run_acceptance(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> ApiResult {
    if let Err(e) = ensure_admin(&state, &claims.user_id).await {
        return e;
    }
    let ws = match scoped_workspace(WorkspaceScope(ws)) {
        Ok(ws) => ws,
        Err(e) => return e,
    };
    let Some(access) = state.access.clone() else {
        return access_missing();
    };
    let Some(dir) = access.package_dir(&ws, &driver) else {
        return fail(StatusCode::NOT_FOUND, "driver not installed");
    };
    // The runner is synchronous and spins up its own tokio runtime for
    // materialization; keep it off the request's runtime thread.
    let report = match tokio::task::spawn_blocking(move || run_profiles(&dir, &["all"])).await {
        Ok(report) => report,
        Err(e) => {
            return fail(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("acceptance runner failed: {e}"),
            );
        }
    };
    // An unreadable package has no identity; persist the requested driver
    // name rather than the runner's `<unreadable>` placeholder.
    let (driver_name, driver_version) = if report.driver == "<unreadable>" {
        (driver.clone(), String::new())
    } else {
        (report.driver.clone(), report.version.clone())
    };
    let rec = AcceptanceReportRecord {
        workspace_id: ws.clone(),
        driver_name,
        driver_version,
        profile: "all".into(),
        level: if report.passed() { "pass".into() } else { "fail".into() },
        report_json: serde_json::to_string(&report).unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = access.acceptance_store.insert(&rec).await {
        tracing::warn!(error = %e, "acceptance report persist failed");
    }
    (StatusCode::OK, Json(json!({"success": true, "report": report})))
}
