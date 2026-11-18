use async_trait::async_trait;
use axum::{
    Extension,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use tinyiothub_core::driver::manifest::DriverManifest;
use tinyiothub_web::{response::ApiResponseBuilder, security::Claims};

use super::{
    MediaState, authorize_workspace, error_response,
    types::{PlaybackDescriptor, PlaybackProtocol},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamPlaybackDescriptor {
    pub protocol: String,
    pub url: Option<String>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackUnavailable;

#[async_trait]
pub trait PlaybackProvider: Send + Sync {
    async fn descriptor(
        &self,
        workspace_id: &str,
        device_id: &str,
        source_id: &str,
    ) -> Result<UpstreamPlaybackDescriptor, PlaybackUnavailable>;
}

pub struct UnavailablePlaybackProvider;

#[async_trait]
impl PlaybackProvider for UnavailablePlaybackProvider {
    async fn descriptor(
        &self,
        _workspace_id: &str,
        _device_id: &str,
        _source_id: &str,
    ) -> Result<UpstreamPlaybackDescriptor, PlaybackUnavailable> {
        Err(PlaybackUnavailable)
    }
}

pub fn descriptor_expiry(now: DateTime<Utc>, upstream: DateTime<Utc>) -> DateTime<Utc> {
    upstream.min(now + Duration::minutes(5))
}

pub fn validate_playback_url(value: &str) -> Result<reqwest::Url, PlaybackUnavailable> {
    let url = reqwest::Url::parse(value).map_err(|_| PlaybackUnavailable)?;
    let secure_or_local = url.scheme() == "https" || (url.scheme() == "http" && url.host_str() == Some("localhost"));
    if !secure_or_local || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(PlaybackUnavailable);
    }
    Ok(url)
}

fn unavailable(workspace_id: &str, device_id: &str, source_id: &str) -> Response {
    tracing::info!(%workspace_id, %device_id, %source_id, "playback unavailable");
    error_response(StatusCode::SERVICE_UNAVAILABLE, "playback_unavailable")
}

pub async fn get_playback(
    State(state): State<MediaState>,
    Extension(claims): Extension<Claims>,
    Path((workspace_id, device_id, source_id)): Path<(String, String, String)>,
) -> Response {
    if let Err(response) = authorize_workspace(&state, &claims, &workspace_id).await {
        return response;
    }
    match state.db.find_thing_by_id(Some(&workspace_id), &device_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "设备不存在"),
        Err(_) => {
            tracing::error!(%workspace_id, %device_id, "playback device lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "设备读取失败");
        }
    }
    let declaration = match state.db.find_device_declaration(&workspace_id, &device_id).await {
        Ok(Some(declaration)) if declaration.status == "active" => declaration,
        Ok(_) => return error_response(StatusCode::NOT_FOUND, "设备声明不存在"),
        Err(_) => {
            tracing::error!(%workspace_id, %device_id, "playback declaration lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "设备声明读取失败");
        }
    };
    let packages = match state.db.list_active_driver_declarations(&workspace_id).await {
        Ok(packages) => packages,
        Err(_) => {
            tracing::error!(%workspace_id, %device_id, "playback package lookup failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "设备声明读取失败");
        }
    };
    let source_is_declared = packages
        .iter()
        .filter(|package| {
            package.driver_name == declaration.driver_name && package.driver_version == declaration.driver_version
        })
        .filter_map(|package| serde_json::from_str::<DriverManifest>(&package.manifest_json).ok())
        .any(|manifest| manifest.sources.iter().any(|source| source.id == source_id));
    if !source_is_declared {
        return error_response(StatusCode::NOT_FOUND, "数据源不存在");
    }

    let upstream = match state
        .playback_provider
        .descriptor(&workspace_id, &device_id, &source_id)
        .await
    {
        Ok(descriptor) => descriptor,
        Err(PlaybackUnavailable) => return unavailable(&workspace_id, &device_id, &source_id),
    };
    let protocol = match upstream.protocol.as_str() {
        "mjpeg" => PlaybackProtocol::Mjpeg,
        "hls" => PlaybackProtocol::Hls,
        _ => return unavailable(&workspace_id, &device_id, &source_id),
    };
    let Some(signed_url) = upstream.url else {
        return unavailable(&workspace_id, &device_id, &source_id);
    };
    let url = match validate_playback_url(&signed_url) {
        Ok(url) => url,
        Err(PlaybackUnavailable) => return unavailable(&workspace_id, &device_id, &source_id),
    };
    let expires_at = descriptor_expiry(Utc::now(), upstream.expires_at);
    let origin = url.origin().ascii_serialization();
    tracing::info!(%workspace_id, %device_id, %source_id, %origin, %expires_at, "playback descriptor issued");
    ApiResponseBuilder::success(PlaybackDescriptor {
        protocol,
        url: signed_url,
        expires_at,
    })
    .into_response()
}
