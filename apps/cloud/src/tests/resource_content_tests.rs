//! A-scheme knowledge storage: Markdown body lives in `resources.content` and
//! must round-trip through the workspace resource create/update API.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{auth_header, create_test_token_with_workspace, seed_test_workspace, setup_test_app_with_pool};

async fn setup() -> (axum::Router, sqlx::SqlitePool) {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-a").await;
    let api_router = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", api_router).with_state(app_state);
    (app, pool)
}

fn token() -> String {
    create_test_token_with_workspace("user-1", "tenant-1", "ws-a")
}

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(&token()))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn document_content_round_trips_create_and_update() {
    let (app, _pool) = setup().await;
    let created = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/v1/workspaces/ws-a/resources",
            json!({
                "name": "冷链手册",
                "description": "冷库处置",
                "resource_type": "document",
                "content": "# 冷链手册\n\n先检查门磁。",
                "file_path": "",
                "tags": ["设备手册"]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created = body_json(created).await;
    assert_eq!(created["code"], 0);
    assert_eq!(created["result"]["content"], "# 冷链手册\n\n先检查门磁。");
    let id = created["result"]["id"].as_str().unwrap().to_string();

    let updated = app
        .clone()
        .oneshot(json_request(
            "PUT",
            &format!("/api/v1/workspaces/ws-a/resources/{id}"),
            json!({"content": "# 冷链手册 v2\n\n先检查门磁，再看压缩机。"}),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = body_json(updated).await;
    assert_eq!(
        updated["result"]["content"],
        "# 冷链手册 v2\n\n先检查门磁，再看压缩机。"
    );
    assert_eq!(updated["result"]["name"], "冷链手册");
}
