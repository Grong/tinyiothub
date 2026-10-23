//! T5：GET /api/v1/resources/{id}/file — workspace 隔离 + 防路径穿越。
//!
//! `/uploads` 的裸 ServeDir 无认证无隔离；本套件验证带 JWT workspace 校验的
//! 服务面：同 workspace 200 字节一致、跨 workspace 404、路径穿越拒绝（404）。

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token_with_workspace, seed_test_workspace, setup_test_app_with_pool};

async fn setup() -> (axum::Router, sqlx::SqlitePool, crate::state::AppState) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    seed_test_workspace(&pool, "tenant-1", "ws-b").await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new()
        .nest("/api", api_router)
        .with_state(app_state.clone());
    (app, pool, app_state)
}

async fn insert_resource(pool: &sqlx::SqlitePool, id: &str, ws: &str, file_path: &str) {
    sqlx::query(
        "INSERT INTO resources (id, workspace_id, resource_type, name, file_path, tags, created_at, updated_at) \
         VALUES (?, ?, 'scene3d', '园区三维模型', ?, '[]', '2026-10-19 00:00:00', '2026-10-19 00:00:00')",
    )
    .bind(id)
    .bind(ws)
    .bind(file_path)
    .execute(pool)
    .await
    .expect("insert resource row");
}

fn get_file(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", auth_header(token))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn resource_file_served_within_workspace() {
    let (app, pool, app_state) = setup().await;

    let stored = format!("{}_park.glb", uuid::Uuid::new_v4());
    let dir = app_state.agents_base_dir.join("ws-a").join("uploads");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join(&stored);
    let payload: &[u8] = b"glTF-binary-fixture";
    tokio::fs::write(&file, payload).await.unwrap();

    let db_path = format!("/uploads/ws-a/uploads/{stored}");
    insert_resource(&pool, "res-t5-ok", "ws-a", &db_path).await;

    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-a");
    let resp = app
        .oneshot(get_file("/api/v1/resources/res-t5-ok/file", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert_eq!(ct, "model/gltf-binary", "glb 必须映射 glTF 二进制类型");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], payload, "返回字节必须与上传一致");

    let _ = tokio::fs::remove_file(&file).await;
}

#[tokio::test]
async fn resource_file_cross_workspace_404() {
    let (app, pool, app_state) = setup().await;

    let stored = format!("{}_park.glb", uuid::Uuid::new_v4());
    let dir = app_state.agents_base_dir.join("ws-a").join("uploads");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join(&stored);
    tokio::fs::write(&file, b"x").await.unwrap();

    let db_path = format!("/uploads/ws-a/uploads/{stored}");
    insert_resource(&pool, "res-t5-xws", "ws-a", &db_path).await;

    // 资源属 ws-a，调用方持 ws-b 令牌 → 404（不可见，非 403）。
    let token_b = create_test_token_with_workspace("user-1", "tenant-1", "ws-b");
    let resp = app
        .oneshot(get_file("/api/v1/resources/res-t5-xws/file", &token_b))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let _ = tokio::fs::remove_file(&file).await;
}

#[tokio::test]
async fn resource_file_path_traversal_rejected() {
    let (app, pool, _app_state) = setup().await;

    insert_resource(&pool, "res-t5-traversal", "ws-a", "/uploads/ws-a/../../../../etc/hosts").await;

    let token = create_test_token_with_workspace("user-1", "tenant-1", "ws-a");
    let resp = app
        .oneshot(get_file("/api/v1/resources/res-t5-traversal/file", &token))
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "含 .. 的 file_path 必须拒绝（404，不区分存在性）"
    );
}
