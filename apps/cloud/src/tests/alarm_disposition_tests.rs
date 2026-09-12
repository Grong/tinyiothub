//! T3/T7/T8 报警处置流测试：kill switch / 严重级切分 / flapping 防抖 / 预算闸。
//! 真实 DB（迁移 + seed），AlarmService::create_alarm 公共入口驱动。

use std::sync::Arc;

use tinyiothub_storage::alarm::{Alarm, AlarmLevel, AlarmType};
use tinyiothub_storage::judgment::JudgmentStatus;
use tinyiothub_storage::Db;

use crate::domains::alarm::service::AlarmService;

async fn test_db() -> Arc<Db> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    tinyiothub_storage::migrations::run_migrations(&pool)
        .await
        .expect("run migrations");
    tinyiothub_storage::seed::seed_system(&Db::new(pool.clone())).await.expect("seed");
    sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
        .execute(&pool).await.unwrap();
    Arc::new(Db::new(pool))
}

fn make_alarm(level: AlarmLevel) -> Alarm {
    Alarm::new(
        "t1".to_string(),
        None,
        None,
        AlarmType::PropertyThreshold,
        level,
        "温度越限".to_string(),
        None,
        None,
        Some("ws1".to_string()),
    )
}

/// T7：kill switch 关闭 → 不产生判断、不走处置流（报警保持 Active 人工路径）。
#[tokio::test]
async fn kill_switch_off_produces_no_judgment() {
    let db = test_db().await;
    // 关闭开关
    let mut config = tinyiothub_storage::heartbeat::WorkspaceHeartbeatConfig::validated(true, 15).unwrap();
    config.ai_triage_enabled = false;
    db.save_heartbeat_config("ws1", &config).await.unwrap();

    let svc = AlarmService::new(db.clone());
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();

    let judgments = db.list_judgments("ws1", None, None, 10).await.unwrap();
    assert!(judgments.is_empty(), "kill switch off → no judgment");
}

/// T3：Warning 报警 → AI 分诊（investigating 判断落库）。
#[tokio::test]
async fn warning_alarm_creates_investigating_judgment() {
    let db = test_db().await;
    let svc = AlarmService::new(db.clone());
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();

    let judgments = db.list_judgments("ws1", None, None, 10).await.unwrap();
    assert_eq!(judgments.len(), 1);
    assert_eq!(judgments[0].status, JudgmentStatus::Investigating);
    assert_eq!(judgments[0].alarm_id.is_some(), true);
}

/// T3：flapping 防抖——同 thing+rule 已有未终态判断时，新报警不再发起调查。
#[tokio::test]
async fn flapping_alarm_does_not_duplicate_judgment() {
    let db = test_db().await;
    let svc = AlarmService::new(db.clone());
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();

    let judgments = db.list_judgments("ws1", None, None, 10).await.unwrap();
    assert_eq!(judgments.len(), 1, "flapping deduped at source");
}

/// T3+D8：Critical 报警 → 直达工单（escalation 端口）+ judgment 关联。
#[tokio::test]
async fn critical_alarm_escalates_directly_to_ticket() {
    use std::sync::Mutex;
    // 真开工单（judgments.ticket_id 有 FK，假 id 会违反约束）
    struct RealEscalation {
        db: Arc<Db>,
        calls: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl crate::domains::ticket::AlarmEscalation for RealEscalation {
        async fn escalate_alarm(&self, alarm: &Alarm) -> Option<i64> {
            self.calls.lock().unwrap().push(alarm.id.clone());
            let sse = crate::domains::event::sse_manager::SseConnectionManager::new();
            crate::domains::ticket::create_escalation(
                &self.db,
                &sse,
                crate::domains::ticket::Escalation {
                    workspace_id: alarm.workspace_id.clone().unwrap(),
                    thing_id: Some(alarm.thing_id.clone()),
                    agent_run_id: format!("alarm:{}", alarm.id),
                    title: alarm.message.clone(),
                    briefing: serde_json::json!({"problem": alarm.message, "source": "critical_alarm_direct"}),
                    failure_hash: format!("pk:alarm:{}:-", alarm.thing_id),
                },
            )
            .await
        }
    }

    let db = test_db().await;
    let svc = AlarmService::new(db.clone());
    let spy = Arc::new(RealEscalation { db: db.clone(), calls: Mutex::new(vec![]) });
    svc.set_escalation(spy.clone());

    svc.create_alarm(make_alarm(AlarmLevel::Critical)).await.unwrap();

    assert_eq!(spy.calls.lock().unwrap().len(), 1, "critical escalated directly");
    let judgments = db.list_judgments("ws1", None, None, 10).await.unwrap();
    assert_eq!(judgments.len(), 1);
    assert!(judgments[0].ticket_id.is_some(), "judgment linked to real ticket");
}

/// T8：超日预算 → 报警不发起调查，judgment 落 budget_skipped 标记。
#[tokio::test]
async fn over_budget_alarm_gets_budget_skipped_marker() {
    let db = test_db().await;
    // 填满今日预算（budget_skipped 不计入，全用 investigating）
    for _ in 0..100 {
        db.insert_judgment("ws1", None, None, Some("t1")).await.unwrap();
    }
    let svc = AlarmService::new(db.clone());
    svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();

    let all = db.list_judgments("ws1", None, None, 200).await.unwrap();
    let skipped = all.iter().filter(|j| j.status == JudgmentStatus::BudgetSkipped).count();
    assert_eq!(skipped, 1, "over-budget alarm marked budget_skipped");
    // budget_skipped 不计入额度（否则明天也永远超预算）
    let count = db.count_judgments_today("ws1").await.unwrap();
    assert_eq!(count, 100);
}

/// T12 E2E（链路组合，真实 DB）：报警 → enter_disposition 建判断 → 调查
/// RunRecorded → subscriber 路由 → 噪声抑制归档。不经真实 LLM（报告直接构造）。
#[tokio::test]
async fn full_chain_alarm_to_noise_archive() {
    use tinyiothub_agent::runtime::events::{AgentEvent, AgentEventKind};
    use tinyiothub_core::agent_runs::{Outcome, RunReport};

    let db = test_db().await;
    let svc = AlarmService::new(db.clone());
    let alarm = svc.create_alarm(make_alarm(AlarmLevel::Warning)).await.unwrap();

    // 链路第一段：判断已建（investigating）
    let judgments = db.list_judgments("ws1", None, None, 10).await.unwrap();
    assert_eq!(judgments.len(), 1);
    let judgment = &judgments[0];
    assert_eq!(judgment.alarm_id.as_deref(), Some(alarm.id.as_str()));

    // 链路第二段：调查 run 完成 → subscriber 路由 noise
    let report = RunReport {
        run_id: "run-inv-1".to_string(),
        workspace_id: "ws1".to_string(),
        trigger: "alarm".to_string(),
        outcome: Outcome::NoActionNeeded,
        summary: "查过了。\n```json\n{\"verdict\": \"noise\", \"reason\": \"短暂波动已自行回落\", \"suggested_action\": null, \"action_category\": \"other\"}\n```".to_string(),
        actions: vec![],
        verified: false,
        duration_ms: 1000,
        tool_calls: 2,
        tokens: 100,
        end_reason: None,
        thing_id: Some("t1".to_string()),
    };
    let event = AgentEvent {
        seq: 1,
        occurred_at: chrono::Utc::now(),
        kind: AgentEventKind::RunRecorded {
            report: Box::new(report),
            problem_key: Some("alarm:t1:-".to_string()),
            dedup_key: None,
        },
    };
    let sse = crate::domains::event::sse_manager::SseConnectionManager::new();
    crate::domains::agent::host::judgment_subscriber::project(&event, &db, &sse, &svc).await;

    // 终态：判断归档 + 报警被抑制
    let j = db.find_judgment_by_id(&judgment.id, "ws1").await.unwrap().unwrap();
    assert_eq!(j.status, JudgmentStatus::NoiseArchived);
    let a = db.find_alarm_by_id(&alarm.id, Some("ws1")).await.unwrap().unwrap();
    assert_eq!(a.status, tinyiothub_storage::alarm::AlarmStatus::Suppressed);
}
