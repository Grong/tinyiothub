pub mod evidence;
pub mod playback;
pub mod types;

use std::sync::Arc;

use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use tinyiothub_runtime::media::evidence::EvidenceService;
use tinyiothub_web::{api_response::ApiResponse, security::Claims};

#[derive(Clone)]
pub struct MediaState {
    pub db: Arc<tinyiothub_storage::Db>,
    pub workspace_access: Arc<crate::state::TenantWorkspaceAccess>,
    pub evidence: Option<Arc<EvidenceService>>,
    pub playback_provider: Arc<dyn playback::PlaybackProvider>,
}

pub fn create_router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    MediaState: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/{id}/evidence/{evidence_id}", get(evidence::get_evidence))
        .route(
            "/{id}/devices/{device_id}/sources/{source_id}/playback",
            get(playback::get_playback),
        )
}

pub(crate) fn error_response(status: StatusCode, message: &'static str) -> Response {
    (
        status,
        Json(ApiResponse::<serde_json::Value> {
            msg: message.into(),
            code: status.as_u16().into(),
            result: None,
        }),
    )
        .into_response()
}

pub(crate) async fn authorize_workspace(
    state: &MediaState,
    claims: &Claims,
    workspace_id: &str,
) -> Result<(), Response> {
    match state.workspace_access.workspace_tenant_id(workspace_id).await {
        Ok(Some(tenant_id)) if tenant_id == claims.tenant_id => Ok(()),
        Ok(Some(_)) => Err(error_response(StatusCode::FORBIDDEN, "无权访问此工作空间")),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "工作空间不存在")),
        Err(_) => {
            tracing::error!(%workspace_id, "media workspace lookup failed");
            Err(error_response(StatusCode::INTERNAL_SERVER_ERROR, "获取工作空间失败"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode, header},
    };
    use chrono::{DateTime, Utc};
    use http_body_util::BodyExt;
    use sha2::Digest;
    use tempfile::TempDir;
    use tinyiothub_core::{
        driver::{
            declaration::{DeviceDeclaration, DeviceDeclarationRecord, DriverDeclarationRecord, fixed_declaration},
            manifest::DriverManifest,
        },
        media::MediaEvidenceRecord,
    };
    use tinyiothub_runtime::media::evidence::{EvidenceService, LocalMediaStore, MediaStore};
    use tower::ServiceExt;

    use super::playback::{PlaybackProvider, PlaybackUnavailable, UpstreamPlaybackDescriptor};
    use crate::{
        state::AppState,
        test_utils::{auth_header, create_test_token_with_workspace, seed_test_workspace, setup_test_app_with_pool},
    };

    const TENANT: &str = "tenant-media";
    const WORKSPACE: &str = "ws_1";
    const OTHER_WORKSPACE: &str = "ws_other";
    const DEVICE: &str = "device_1";
    const SOURCE: &str = "front";

    struct FakePlayback {
        response: tokio::sync::Mutex<Result<UpstreamPlaybackDescriptor, PlaybackUnavailable>>,
        calls: tokio::sync::Mutex<Vec<(String, String, String)>>,
    }

    impl FakePlayback {
        fn new(response: Result<UpstreamPlaybackDescriptor, PlaybackUnavailable>) -> Self {
            Self {
                response: tokio::sync::Mutex::new(response),
                calls: tokio::sync::Mutex::new(Vec::new()),
            }
        }

        async fn set(&self, response: Result<UpstreamPlaybackDescriptor, PlaybackUnavailable>) {
            *self.response.lock().await = response;
        }
    }

    #[async_trait]
    impl PlaybackProvider for FakePlayback {
        async fn descriptor(
            &self,
            workspace_id: &str,
            device_id: &str,
            source_id: &str,
        ) -> Result<UpstreamPlaybackDescriptor, PlaybackUnavailable> {
            self.calls
                .lock()
                .await
                .push((workspace_id.to_string(), device_id.to_string(), source_id.to_string()));
            self.response.lock().await.clone()
        }
    }

    struct MediaApiFixture {
        app: Router,
        state: AppState,
        provider: Arc<FakePlayback>,
        store: Arc<LocalMediaStore>,
        _media_dir: TempDir,
        token: String,
    }

    impl MediaApiFixture {
        async fn new() -> Self {
            let (mut state, pool) = setup_test_app_with_pool().await;
            seed_test_workspace(&pool, TENANT, WORKSPACE).await;
            seed_test_workspace(&pool, TENANT, OTHER_WORKSPACE).await;
            let now = Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(DEVICE)
                .bind("Front camera")
                .bind(WORKSPACE)
                .bind(&now)
                .bind(&now)
                .execute(&pool)
                .await
                .expect("device");

            let manifest: DriverManifest = serde_json::from_value(serde_json::json!({
                "manifestVersion": "1.1",
                "identity": {
                    "driver": "camera-fixture",
                    "version": "1.0.0",
                    "protocol": "fixture",
                    "displayName": null,
                    "abiVersion": 2,
                    "dynamicCapabilities": false
                },
                "sources": [{
                    "id": SOURCE,
                    "kind": "callback",
                    "dataKind": "video"
                }]
            }))
            .expect("manifest");
            let db = tinyiothub_storage::Db::new(pool.clone());
            db.upsert_driver_declaration(&DriverDeclarationRecord {
                workspace_id: WORKSPACE.into(),
                driver_name: manifest.identity.driver.clone(),
                driver_version: manifest.identity.version.clone(),
                manifest_version: manifest.manifest_version.clone(),
                manifest_json: serde_json::to_string(&manifest).expect("manifest"),
                manifest_hash: "synthetic-manifest-hash".into(),
                binary_hash: None,
                status: "active".into(),
                installed_at: now.clone(),
            })
            .await
            .expect("driver declaration");
            let declaration: DeviceDeclaration = fixed_declaration(&manifest);
            db.upsert_device_declaration(&DeviceDeclarationRecord {
                workspace_id: WORKSPACE.into(),
                thing_id: DEVICE.into(),
                driver_name: manifest.identity.driver.clone(),
                driver_version: manifest.identity.version.clone(),
                declaration_json: serde_json::to_string(&declaration).expect("declaration"),
                declaration_hash: "synthetic-declaration-hash".into(),
                status: "active".into(),
                updated_at: now,
            })
            .await
            .expect("device declaration");

            let media_dir = tempfile::tempdir().expect("media directory");
            let store = Arc::new(LocalMediaStore::new(media_dir.path()).await.expect("media store"));
            let evidence = Arc::new(EvidenceService::new(
                store.clone(),
                Arc::new(crate::shared::runtime_ports::DbMediaEvidenceStore::new(db)),
            ));
            state.evidence = Some(evidence);
            let provider = Arc::new(FakePlayback::new(Ok(upstream(
                "hls",
                Utc::now() + chrono::Duration::hours(1),
            ))));
            state.playback_provider = provider.clone();
            let app = Router::new()
                .nest("/api", crate::api::create_router(&state))
                .with_state(state.clone());
            let token = create_test_token_with_workspace("user-media", TENANT, WORKSPACE);
            Self {
                app,
                state,
                provider,
                store,
                _media_dir: media_dir,
                token,
            }
        }

        async fn enable_media(&self) {
            sqlx::query("UPDATE workspaces SET heartbeat_config = ? WHERE id = ?")
                .bind(
                    serde_json::json!({
                        "enabled": false,
                        "interval_minutes": 5,
                        "runtime_media_enabled": true
                    })
                    .to_string(),
                )
                .bind(WORKSPACE)
                .execute(self.state.db.pool())
                .await
                .expect("enable media");
        }

        async fn put_evidence(&self, id: &str, workspace: &str, mime: &str, bytes: &[u8]) {
            let sha256 = hex::encode(sha2::Sha256::digest(bytes));
            let storage_path = self.store.put(workspace, &sha256, bytes).await.expect("media bytes");
            let now = Utc::now();
            self.state
                .db
                .put_media_evidence(&MediaEvidenceRecord {
                    id: id.into(),
                    workspace_id: workspace.into(),
                    task_id: None,
                    run_id: None,
                    tool_call_id: None,
                    job_id: None,
                    device_id: DEVICE.into(),
                    source_id: SOURCE.into(),
                    source_seq: 1,
                    media_time: now,
                    mime: mime.into(),
                    size_bytes: bytes.len() as u64,
                    sha256,
                    storage_path,
                    dedup_key: format!("dedup-{id}"),
                    created_at: now,
                    expires_at: now + chrono::Duration::days(1),
                })
                .await
                .expect("evidence row");
        }

        async fn expire(&self, evidence_id: &str) {
            sqlx::query("UPDATE media_evidence SET expires_at = ? WHERE id = ?")
                .bind(Utc::now() - chrono::Duration::seconds(1))
                .bind(evidence_id)
                .execute(self.state.db.pool())
                .await
                .expect("expire evidence");
        }

        async fn get_evidence(&self, workspace: &str, evidence_id: &str) -> axum::response::Response {
            self.request(
                &format!("/api/v1/workspaces/{workspace}/evidence/{evidence_id}"),
                Some(&self.token),
            )
            .await
        }

        async fn get_playback(&self, workspace: &str, device: &str, source: &str) -> axum::response::Response {
            self.request(
                &format!("/api/v1/workspaces/{workspace}/devices/{device}/sources/{source}/playback"),
                Some(&self.token),
            )
            .await
        }

        async fn request(&self, path: &str, token: Option<&str>) -> axum::response::Response {
            let mut request = Request::builder().uri(path).method("GET");
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, auth_header(token));
            }
            self.app
                .clone()
                .oneshot(request.body(Body::empty()).expect("request"))
                .await
                .expect("response")
        }
    }

    fn upstream(protocol: &str, expires_at: DateTime<Utc>) -> UpstreamPlaybackDescriptor {
        UpstreamPlaybackDescriptor {
            protocol: protocol.into(),
            url: Some("https://camera.example/live.m3u8?sig=synthetic".into()),
            expires_at,
        }
    }

    async fn json(response: axum::response::Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.expect("body").to_bytes();
        serde_json::from_slice(&bytes).expect("JSON response")
    }

    #[tokio::test]
    async fn evidence_requires_authentication_and_is_workspace_scoped() {
        let fixture = MediaApiFixture::new().await;
        fixture
            .put_evidence("evidence_1", WORKSPACE, "image/png", b"synthetic-png")
            .await;

        assert_eq!(
            fixture
                .request("/api/v1/workspaces/ws_1/evidence/evidence_1", None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            fixture.get_evidence(OTHER_WORKSPACE, "evidence_1").await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            fixture.get_evidence(WORKSPACE, "absent").await.status(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn evidence_returns_stored_content_type_and_explicit_expiry() {
        let fixture = MediaApiFixture::new().await;
        fixture
            .put_evidence("evidence_png", WORKSPACE, "image/png", b"synthetic-png")
            .await;
        fixture
            .put_evidence("evidence_jpeg", WORKSPACE, "image/jpeg", b"synthetic-jpeg")
            .await;

        for (id, content_type, expected) in [
            ("evidence_png", "image/png", b"synthetic-png".as_slice()),
            ("evidence_jpeg", "image/jpeg", b"synthetic-jpeg".as_slice()),
        ] {
            let response = fixture.get_evidence(WORKSPACE, id).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response
                    .headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok()),
                Some(content_type)
            );
            let bytes = response.into_body().collect().await.expect("image body").to_bytes();
            assert_eq!(bytes.as_ref(), expected);
        }

        fixture.expire("evidence_png").await;
        assert_eq!(
            fixture.get_evidence(WORKSPACE, "evidence_png").await.status(),
            StatusCode::GONE
        );
    }

    #[tokio::test]
    async fn playback_requires_device_source_ownership_and_media_flag() {
        let fixture = MediaApiFixture::new().await;
        let disabled = fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await;
        assert_eq!(disabled.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json(disabled).await["msg"], "playback_unavailable");
        assert!(fixture.provider.calls.lock().await.is_empty());

        fixture.enable_media().await;
        for (workspace, device, source) in [
            (OTHER_WORKSPACE, DEVICE, SOURCE),
            (WORKSPACE, "absent-device", SOURCE),
            (WORKSPACE, DEVICE, "absent-source"),
        ] {
            let response = fixture.get_playback(workspace, device, source).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        assert!(fixture.provider.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn playback_returns_direct_url_and_caps_expiry_at_five_minutes() {
        let fixture = MediaApiFixture::new().await;
        fixture.enable_media().await;
        let before = Utc::now();
        let response = fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = json(response).await;
        assert_eq!(body["result"]["protocol"], "hls");
        assert_eq!(body["result"]["url"], "https://camera.example/live.m3u8?sig=synthetic");
        let expires_at = DateTime::parse_from_rfc3339(body["result"]["expiresAt"].as_str().expect("expiry"))
            .expect("RFC3339 expiry")
            .with_timezone(&Utc);
        assert!(expires_at <= before + chrono::Duration::minutes(5) + chrono::Duration::seconds(1));
        assert!(expires_at >= before + chrono::Duration::minutes(5) - chrono::Duration::seconds(1));

        let upstream_expiry = Utc::now() + chrono::Duration::minutes(2);
        fixture.provider.set(Ok(upstream("mjpeg", upstream_expiry))).await;
        let body = json(fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await).await;
        assert_eq!(body["result"]["protocol"], "mjpeg");
        let actual = DateTime::parse_from_rfc3339(body["result"]["expiresAt"].as_str().expect("expiry"))
            .expect("RFC3339 expiry")
            .with_timezone(&Utc);
        assert_eq!(actual, upstream_expiry);
        assert_eq!(
            fixture.provider.calls.lock().await.as_slice(),
            &[
                (WORKSPACE.into(), DEVICE.into(), SOURCE.into()),
                (WORKSPACE.into(), DEVICE.into(), SOURCE.into())
            ]
        );
    }

    #[tokio::test]
    async fn playback_rejects_missing_url_and_unsupported_protocol() {
        let fixture = MediaApiFixture::new().await;
        fixture.enable_media().await;
        let mut missing = upstream("hls", Utc::now() + chrono::Duration::minutes(1));
        missing.url = None;
        fixture.provider.set(Ok(missing)).await;
        let response = fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json(response).await["msg"], "playback_unavailable");

        fixture
            .provider
            .set(Ok(upstream("rtsp", Utc::now() + chrono::Duration::minutes(1))))
            .await;
        let response = fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(json(response).await["msg"], "playback_unavailable");
    }

    #[tokio::test]
    async fn playback_rejects_every_unsafe_upstream_url() {
        let fixture = MediaApiFixture::new().await;
        fixture.enable_media().await;
        for unsafe_url in [
            "file:///tmp/a.m3u8",
            "javascript:alert(1)",
            "data:video/mp4;base64,AA",
            "https://user:pass@camera.example/live.m3u8",
            "http://camera.example/live.m3u8",
        ] {
            let mut descriptor = upstream("hls", Utc::now() + chrono::Duration::minutes(1));
            descriptor.url = Some(unsafe_url.into());
            fixture.provider.set(Ok(descriptor)).await;
            let response = fixture.get_playback(WORKSPACE, DEVICE, SOURCE).await;
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(json(response).await["msg"], "playback_unavailable");
        }
    }

    #[test]
    fn playback_url_rejects_credentials_and_unsafe_schemes() {
        for url in [
            "file:///tmp/a.m3u8",
            "javascript:alert(1)",
            "data:video/mp4;base64,AA",
            "https://user:pass@camera.example/live.m3u8",
            "http://camera.example/live.m3u8",
        ] {
            assert!(super::playback::validate_playback_url(url).is_err());
        }
        assert!(super::playback::validate_playback_url("https://camera.example/live.m3u8?sig=secret").is_ok());
        assert!(super::playback::validate_playback_url("http://localhost:8080/live.m3u8?sig=local").is_ok());
        assert!(super::playback::validate_playback_url("http://127.0.0.1:8080/live.m3u8").is_err());
    }

    #[test]
    fn descriptor_expiry_uses_the_earlier_upstream_or_five_minute_deadline() {
        let now = Utc::now();
        assert_eq!(
            super::playback::descriptor_expiry(now, now + chrono::Duration::minutes(2)),
            now + chrono::Duration::minutes(2)
        );
        assert_eq!(
            super::playback::descriptor_expiry(now, now + chrono::Duration::minutes(30)),
            now + chrono::Duration::minutes(5)
        );
    }
}
