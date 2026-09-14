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
    let jid = app_state
        .db
        .insert_judgment(ws, None, None, Some("t1"), "annotate")
        .await
        .unwrap();
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


struct RecordingSink(std::sync::Mutex<Vec<tinyiothub_agent::runtime::thing_agent::types::WakeSignal>>);

#[async_trait::async_trait]
impl tinyiothub_agent::runtime::thing_agent::DirectiveSink for RecordingSink {
    fn enqueue(
        &self,
        signal: tinyiothub_agent::runtime::thing_agent::types::WakeSignal,
    ) -> Result<(), tinyiothub_agent::runtime::thing_agent::scheduler::EnqueueError> {
        self.0.lock().unwrap().push(signal);
        Ok(())
    }
}

struct FailingSink;

#[async_trait::async_trait]
impl tinyiothub_agent::runtime::thing_agent::DirectiveSink for FailingSink {
    fn enqueue(
        &self,
        _signal: tinyiothub_agent::runtime::thing_agent::types::WakeSignal,
    ) -> Result<(), tinyiothub_agent::runtime::thing_agent::scheduler::EnqueueError> {
        Err(tinyiothub_agent::runtime::thing_agent::scheduler::EnqueueError::Rejected)
    }
}

#[tokio::test]
async fn approve_dispatches_execution_signal() {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", Some("self_healable")).await;
    let sink = std::sync::Arc::new(RecordingSink(std::sync::Mutex::new(vec![])));
    app_state.set_directive_sink(sink.clone());

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req("POST", &format!("/api/v1/judgments/{jid}/approve"), &token, Some(json!({}))))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let j = app_state
        .db
        .find_judgment_by_id(&jid, "ws-default-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Executing);

    let signals = sink.0.lock().unwrap();
    assert_eq!(signals.len(), 1, "恰好派发一条执行 directive");
    let tinyiothub_agent::runtime::thing_agent::types::TriggerSource::UserDirective {
        problem_key, ..
    } = &signals[0].source
    else {
        panic!("exec 信号必须是 UserDirective");
    };
    assert_eq!(problem_key.as_deref(), Some(format!("exec:{jid}").as_str()));
    assert_eq!(signals[0].dedup_key.as_deref(), Some(format!("exec:{jid}").as_str()), "C5/T-14 防重");
}

#[tokio::test]
async fn approve_enqueue_failure_rolls_back_to_awaiting() {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", Some("self_healable")).await;
    app_state.set_directive_sink(std::sync::Arc::new(FailingSink));

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let response = app
        .oneshot(req("POST", &format!("/api/v1/judgments/{jid}/approve"), &token, Some(json!({}))))
        .await
        .unwrap();
    let (_s, json) = response_parts(response).await;
    assert!(json["code"].as_i64().unwrap() != 0, "派发失败必须报错");

    let j = app_state
        .db
        .find_judgment_by_id(&jid, "ws-default-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        j.status,
        tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
        "D3 补偿回滚：executing → awaiting_approval，可重试"
    );
}

/// 评审补测：approve 重复点击 → 409（条件迁移防并发互撞）。
#[tokio::test]
async fn approve_twice_second_gets_409() {
    let (mut app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", Some("self_healable")).await;
    app_state.set_directive_sink(std::sync::Arc::new(RecordingSink(std::sync::Mutex::new(vec![]))));

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let r1 = app
        .clone()
        .oneshot(req("POST", &format!("/api/v1/judgments/{jid}/approve"), &token, Some(json!({}))))
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let r2 = app
        .oneshot(req("POST", &format!("/api/v1/judgments/{jid}/approve"), &token, Some(json!({}))))
        .await
        .unwrap();
    let (_s, json) = response_parts(r2).await;
    assert!(json["code"].as_i64().unwrap() != 0, "重复批准必须 409 风格报错");
}

/// 评审补测：feedback 非法 verdict → 400；不存在 id → 404；reject 404。
#[tokio::test]
async fn feedback_and_reject_negative_paths() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    let jid = seed_judgment(&app_state, "ws-default-001", None).await;

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");

    // 非法 verdict 值 → 400
    let r = app
        .clone()
        .oneshot(
            req(
                "POST",
                &format!("/api/v1/judgments/{jid}/feedback"),
                &token,
                Some(json!({"verdict": "meh"})),
            ),
        )
        .await
        .unwrap();
    let (_s, json) = response_parts(r).await;
    assert_ne!(json["code"].as_i64().unwrap(), 0, "非法 verdict 必须 400");

    // 不存在 id 的 feedback/reject → 404
    for path in ["feedback", "reject"] {
        let body = if path == "feedback" {
            json!({"verdict": "right"})
        } else {
            json!({"reason": "不需要"})
        };
        let r = app
            .clone()
            .oneshot(req("POST", &format!("/api/v1/judgments/nonexistent/{path}"), &token, Some(body)))
            .await
            .unwrap();
        let (_s, json) = response_parts(r).await;
        assert_ne!(json["code"].as_i64().unwrap(), 0, "{path} 不存在 id 必须 404");
    }
}

/// 评审补测（F-C/T-17 误判恢复闭环服务端侧）：suppress 模式 noise 归档后
/// ✕ 反馈 → 报警恢复 Active + judgment 重开 investigating。
#[tokio::test]
async fn wrong_feedback_on_noise_restores_and_reopens() {
    let (app_state, pool) = setup_test_app_with_pool().await;
    seed_test_workspace(&pool, "tenant-1", "ws-default-001").await;
    // 设备 + 被抑制的报警（恢复链路的对象）
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type, state, created_at, updated_at) VALUES ('t1','t1','ws-default-001','sensor',1,'2025-01-01','2025-01-01')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO thing_alarms (id, thing_id, workspace_id, alarm_level, alarm_message, alarm_time, is_suppressed) VALUES ('a1','t1','ws-default-001','warning','温度越限','2025-01-01',1)")
        .execute(&pool).await.unwrap();
    let jid = app_state
        .db
        .insert_judgment("ws-default-001", Some("a1"), None, Some("t1"), "suppress")
        .await
        .unwrap();
    app_state
        .db
        .judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::Noise,
            "误判的噪声",
            "{}",
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let app = crate::api::create_router(&app_state);
    let app = axum::Router::new().nest("/api", app).with_state(app_state.clone());
    let token = create_test_token("user-1", "tenant-1");
    let r = app
        .oneshot(
            req(
                "POST",
                &format!("/api/v1/judgments/{jid}/feedback"),
                &token,
                Some(json!({"verdict": "wrong", "reason": "这不是噪声，温度真的有问题"})),
            ),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    let a = app_state
        .db
        .find_alarm_by_id("a1", Some("ws-default-001"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        a.status,
        tinyiothub_storage::alarm::AlarmStatus::Active,
        "✕ 反馈后被抑制报警恢复 Active"
    );
    let j = app_state
        .db
        .find_judgment_by_id(&jid, "ws-default-001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        j.status,
        tinyiothub_storage::judgment::JudgmentStatus::Investigating,
        "judgment 重开重调查"
    );
}
