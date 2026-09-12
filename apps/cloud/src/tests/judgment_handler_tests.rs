//! T9：feed API handler 测试（HTTP 层，真实 AppState + token 鉴权）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_utils::{
    auth_header, create_test_token, create_test_token_with_workspace, response_parts, seed_test_workspace,
    setup_test_app_with_pool,
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

async fn seed_judgment(app_state: &crate::state::AppState, ws: &str, verdict: Option<&str>) -> String {
    let jid = app_state.db.insert_judgment(ws, None, None, Some("t1")).await.unwrap();
    if let Some(v) = verdict {
        let verdict = match v {
            "noise" => tinyiothub_storage::judgment::JudgmentVerdict::Noise,
            "self_healable" => tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            _ => tinyiothub_storage::judgment::JudgmentVerdict::NeedsHuman,
        };
        app_state
            .db
            .judge_judgment(
                &jid,
                verdict,
                "测试理由",
                "{}",
                Some("重连"),
                Some("connection_recovery"),
                None,
            )
            .await
            .unwrap();
    }
    jid
}

#[tokio::test]
async fn list_judgments_returns_seeded() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let _jid = seed_judgment(&app_state, "ws-default-001", None).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req("GET", "/api/v1/judgments?tab=all", &token, None))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_s, json) = response_parts(response).await;
    let data = json["result"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["status"], "investigating");
}

#[tokio::test]
async fn list_judgments_workspace_isolated() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    seed_test_workspace(&pool, "tenant-b", "ws-b").await;
    seed_test_workspace(&pool, "tenant-a", "ws-a").await;
    let _jid = seed_judgment(&app_state, "ws-a", None).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token_b = create_test_token_with_workspace("user-2", "tenant-b", "ws-b");
    let response = app
        .oneshot(req("GET", "/api/v1/judgments?tab=all", &token_b, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    let data = json["result"].as_array().expect("data array");
    assert!(data.is_empty(), "ws-b must not see ws-a judgments");
}

#[tokio::test]
async fn feedback_wrong_requires_reason() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", Some("noise")).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    // 无原因 → 400
    let response = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/api/v1/judgments/{jid}/feedback"),
            &token,
            Some(json!({"verdict": "wrong"})),
        ))
        .await
        .unwrap();
    {
        let (_s, json) = response_parts(response).await;
        assert_eq!(json["code"], 400);
    }

    // 有原因 → 200，且反馈可见
    let response = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/api/v1/judgments/{jid}/feedback"),
            &token,
            Some(json!({"verdict": "wrong", "reason": "这个传感器梅雨季就是会越限"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(req("GET", "/api/v1/judgments?tab=all", &token, None))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    let fb = &json["result"][0]["latestFeedback"];
    assert_eq!(fb["verdict"], "wrong");
}

#[tokio::test]
async fn approve_rejects_non_awaiting_status() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    // investigating 状态（未 judged）不可批准
    let jid = seed_judgment(&app_state, "ws-default-001", None).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req(
            "POST",
            &format!("/api/v1/judgments/{jid}/approve"),
            &token,
            Some(json!({})),
        ))
        .await
        .unwrap();
    // 测试态无 directive_sink → 先报执行通道未就绪（状态未被翻转）
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].as_i64().unwrap() != 0, "should error without sink");
    let j = app_state
        .db
        .find_judgment_by_id(&jid, "ws-default-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        j.status,
        tinyiothub_storage::judgment::JudgmentStatus::Investigating,
        "状态未被错误翻转"
    );
}

#[tokio::test]
async fn reject_requires_reason_and_escalates() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", Some("self_healable")).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    // 无原因 → 400
    let response = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/api/v1/judgments/{jid}/reject"),
            &token,
            Some(json!({"reason": ""})),
        ))
        .await
        .unwrap();
    {
        let (_s, json) = response_parts(response).await;
        assert_eq!(json["code"], 400);
    }

    // 有原因 → escalated + ticket
    let response = app
        .oneshot(req(
            "POST",
            &format!("/api/v1/judgments/{jid}/reject"),
            &token,
            Some(json!({"reason": "产线维护窗口，现在不能重启"})),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let j = app_state
        .db
        .find_judgment_by_id(&jid, "ws-default-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
    assert!(j.ticket_id.is_some(), "ticket linked after reject");
}
