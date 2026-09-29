//! T4（eng-review R-G1/G4）：patrol approvals handler 级 HTTP 测试——
//! 状态码/错误体映射的直接护栏（单元层已覆盖翻转逻辑，本文件锁 HTTP 映射）：
//! - 409：reject 一个已批准的提案（审计不可覆盖，冲突显式化）
//! - 403：非 admin 调用 approve（F9 角色闸）
//! - 400：reject 空原因（X2 必填原因契约）

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, response_parts, seed_admin_role, seed_test_workspace, setup_test_app_with_pool,
};

fn req(method: &str, uri: &str, token: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", auth_header(token));
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .unwrap()
}

/// 种一条 patrol 提案行（与生产 heartbeat 写入同构）。
async fn seed_proposal(pool: &sqlx::SqlitePool, ws: &str, proposal_id: &str, status: &str) {
    let content = json!({
        "proposalId": proposal_id,
        "status": status,
        "toolName": "write_properties",
        "thingId": "dev_1",
        "summary": "set temp",
        "reason": "tune",
        "risk": "medium",
        "parameters": {"thing_id": "dev_1", "properties": {"target_temp": 22}},
    });
    sqlx::query(
        "INSERT INTO agent_actions (id, workspace_id, agent_id, event_type, action_type, content, created_at) \
         VALUES (?, ?, '__heartbeat__:x', 'heartbeat', 'proposal', ?, '2026-07-20 10:00:00')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(ws)
    .bind(content.to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// 409：reject 已批准提案 → 条件翻转 0 行命中 → 409（审计记录不可覆盖）。
#[tokio::test]
async fn reject_approved_proposal_gets_409() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    seed_admin_role(&pool, "user-1").await;
    seed_proposal(&pool, "ws-1", "p-done", "approved").await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req(
            "POST",
            "/api/v1/workspaces/ws-1/heartbeat/approvals/p-done/reject",
            &token,
            Some(json!({"reason": "迟到的否决，不应覆盖"})),
        ))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 409, "已批准提案的 reject 必须 409: {json}");
    // 审计不可变：content 仍是 approved
    let (content,): (String,) =
        sqlx::query_as("SELECT content FROM agent_actions WHERE json_extract(content, '$.proposalId') = 'p-done'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["status"], "approved", "已执行批准的审计不可被覆盖");
}

/// 403：非 admin 调用 approve → F9 角色闸（fail-closed）。
#[tokio::test]
async fn approve_non_admin_gets_403() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    // 注意：不 seed admin 角色——user-1 是 member
    seed_proposal(&pool, "ws-1", "p-1", "pending").await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req(
            "POST",
            "/api/v1/workspaces/ws-1/heartbeat/approvals/p-1/approve",
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 403, "非 admin 批准必须 403: {json}");
    // 提案未被触碰
    let (content,): (String,) =
        sqlx::query_as("SELECT content FROM agent_actions WHERE json_extract(content, '$.proposalId') = 'p-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["status"], "pending", "403 不得翻转提案");
}

/// 400：reject 空原因 → X2 必填原因契约（至少 4 字符）。
#[tokio::test]
async fn reject_empty_reason_gets_400() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    seed_admin_role(&pool, "user-1").await;
    seed_proposal(&pool, "ws-1", "p-2", "pending").await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req(
            "POST",
            "/api/v1/workspaces/ws-1/heartbeat/approvals/p-2/reject",
            &token,
            Some(json!({"reason": "  "})),
        ))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert_eq!(json["code"], 400, "空原因 reject 必须 400: {json}");
}

/// 冒烟：合法 reject 全路径 → 200 + dismissed 落 content（对照组，防 harness 误报）。
#[tokio::test]
async fn reject_pending_with_reason_succeeds() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-1").await;
    seed_admin_role(&pool, "user-1").await;
    seed_proposal(&pool, "ws-1", "p-3", "pending").await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req(
            "POST",
            "/api/v1/workspaces/ws-1/heartbeat/approvals/p-3/reject",
            &token,
            Some(json!({"reason": "误报，无需处理"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (content,): (String,) =
        sqlx::query_as("SELECT content FROM agent_actions WHERE json_extract(content, '$.proposalId') = 'p-3'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["status"], "rejected");
    assert_eq!(parsed["dismiss_reason"], "误报，无需处理");
}
