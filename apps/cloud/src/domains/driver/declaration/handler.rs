//! E4a install / declaration routes, mounted under the existing
//! `/v1/drivers` prefix.

use axum::extract::{Multipart, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tinyiothub_core::driver::declaration::AcceptanceReportRecord;
use tinyiothub_runtime::acceptance::runner::run_profiles;
use tinyiothub_web::middleware::workspace::WorkspaceScope;

use crate::state::AppState;

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/install", post(install_driver))
        .route("/declarations", get(list_declarations))
        .route("/{driver}/declaration", get(get_declaration))
        .route("/{driver}/acceptance", post(run_acceptance))
}

/// `POST /v1/drivers/install` — multipart fields `manifest` (required) and
/// optional `binary`. The package is copied out of the request tempdir into
/// the workspace package store before returning.
async fn install_driver(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    mut multipart: Multipart,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
    };
    let mut manifest_seen = false;
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("manifest") => match field.text().await {
                Ok(text) => {
                    if std::fs::write(dir.path().join("driver.manifest.json"), text).is_err() {
                        return Json(json!({"success": false, "error": "write manifest failed"}));
                    }
                    manifest_seen = true;
                }
                Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
            },
            Some("binary") => match field.bytes().await {
                Ok(bytes) => {
                    if std::fs::write(dir.path().join("driver.so"), bytes).is_err() {
                        return Json(json!({"success": false, "error": "write binary failed"}));
                    }
                }
                Err(e) => return Json(json!({"success": false, "error": e.to_string()})),
            },
            _ => {}
        }
    }
    if !manifest_seen {
        return Json(json!({"success": false, "error": "missing manifest field"}));
    }
    match access.install_dir(&ws, dir.path()).await {
        Ok(pkg) => Json(json!({
            "success": true,
            "driver": pkg.manifest.identity.driver,
            "version": pkg.manifest.identity.version,
            "manifestHash": pkg.manifest_hash,
        })),
        Err(e) => Json(json!({"success": false, "error": e.to_string()})),
    }
}

/// `GET /v1/drivers/declarations` — active packages visible to the caller's
/// workspace (installed packages plus built-in fallbacks).
async fn list_declarations(State(state): State<AppState>, WorkspaceScope(ws): WorkspaceScope) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
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
    Json(json!({"success": true, "data": items}))
}

/// `GET /v1/drivers/{driver}/declaration` — the active package manifest.
async fn get_declaration(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    match access.get_manifest(&ws, &driver) {
        Some(pkg) => Json(json!({
            "success": true,
            "data": serde_json::to_value(&pkg.manifest).unwrap_or_default(),
        })),
        None => Json(json!({"success": false, "error": "driver not installed"})),
    }
}

/// `POST /v1/drivers/{driver}/acceptance` — run every auto profile against the
/// installed package and persist the aggregate report.
async fn run_acceptance(
    State(state): State<AppState>,
    WorkspaceScope(ws): WorkspaceScope,
    Path(driver): Path<String>,
) -> Json<Value> {
    let ws = ws.unwrap_or_default();
    let Some(access) = state.access.clone() else {
        return Json(json!({"success": false, "error": "access service not wired"}));
    };
    let Some(dir) = access.package_dir(&ws, &driver) else {
        return Json(json!({"success": false, "error": "driver not installed"}));
    };
    // The runner is synchronous and spins up its own tokio runtime for
    // materialization; keep it off the request's runtime thread.
    let report = match tokio::task::spawn_blocking(move || run_profiles(&dir, &["all"])).await {
        Ok(report) => report,
        Err(e) => return Json(json!({"success": false, "error": format!("acceptance runner failed: {e}")})),
    };
    let rec = AcceptanceReportRecord {
        workspace_id: ws.clone(),
        driver_name: report.driver.clone(),
        driver_version: report.version.clone(),
        profile: "all".into(),
        level: if report.passed() { "pass".into() } else { "fail".into() },
        report_json: serde_json::to_string(&report).unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = access.acceptance_store.insert(&rec).await {
        tracing::warn!(error = %e, "acceptance report persist failed");
    }
    Json(json!({"success": true, "report": report}))
}
