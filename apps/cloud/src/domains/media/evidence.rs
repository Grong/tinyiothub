use axum::{
    Extension,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tinyiothub_web::security::Claims;

use super::{MediaState, authorize_workspace, error_response};

pub async fn get_evidence(
    State(state): State<MediaState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, evidence_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = authorize_workspace(&state, &claims, &workspace_id).await {
        return response;
    }
    let row = match state
        .db
        .get_media_evidence_in_workspace(&workspace_id, &evidence_id)
        .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "证据不存在"),
        Err(_) => {
            tracing::error!(%workspace_id, %evidence_id, "evidence metadata lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "证据读取失败");
        }
    };
    if row.expires_at <= chrono::Utc::now() {
        return error_response(StatusCode::GONE, "证据已过期");
    }
    let content_type = match row.mime.as_str() {
        "image/jpeg" => "image/jpeg",
        "image/png" => "image/png",
        _ => {
            tracing::error!(%workspace_id, %evidence_id, "unsupported stored evidence MIME");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "证据读取失败");
        }
    };
    let Some(evidence) = state.evidence else {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "证据服务不可用");
    };
    match evidence.get_image(&evidence_id).await {
        Ok(Some(image)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            Body::from(image.bytes),
        )
            .into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "证据不存在"),
        Err(_) => {
            tracing::error!(%workspace_id, %evidence_id, "evidence byte read failed");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "证据读取失败")
        }
    }
}
