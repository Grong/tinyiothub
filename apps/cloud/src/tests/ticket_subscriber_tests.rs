//! 工单订阅者测试（T5/T6/T9）：事件驱动开票、去重、复发折叠、对账补票。
//!
//! 真实 DB（test_helpers::test_pool），不 mock db 层（buzz 模式约定）。

use std::sync::Arc;

use tinyiothub_agent::runtime::events::{AgentEventBus, AgentEventKind};
use tinyiothub_core::agent_runs::{ActionRecord, ActionResult, EndReason, Outcome, RunReport};
use tinyiothub_storage::Db;

use crate::domains::agent::host::ticket_subscriber::{
    build_briefing, failure_hash, normalize, run_ticket_subscriber, should_ticket, synthesize_title, ticket_for_run,
};
use crate::domains::event::sse_manager::SseConnectionManager;

async fn test_db_async() -> Db {
    Db::new(tinyiothub_storage::test_helpers::test_pool().await)
}

fn report(run_id: &str, outcome: Outcome, end_reason: Option<EndReason>, thing: Option<&str>) -> RunReport {
    let actions = match thing {
        Some(t) => vec![ActionRecord {
            thing_id: t.to_string(),
            action_name: "reboot".to_string(),
            params: serde_json::Value::Null,
            result: ActionResult::Failed("E-401 闸阀执行器超时".to_string()),
            verified: false,
        }],
        None => vec![],
    };
    RunReport {
        run_id: run_id.to_string(),
        workspace_id: "ws1".to_string(),
        trigger: "thing:t1:event:temp_high".to_string(),
        outcome,
        summary: format!(
            "触发: thing:t1:event:temp_high\n动作:\n- t1.reboot: 失败: E-401\n动作被策略拒绝，建议检查自治策略配置 (run {run_id})"
        ),
        actions,
        verified: false,
        duration_ms: 100,
        tool_calls: 1,
        tokens: 10,
        end_reason,
        thing_id: thing.map(str::to_string),
    }
}

fn sse() -> Arc<SseConnectionManager> {
    Arc::new(SseConnectionManager::new())
}

#[tokio::test]
async fn tickets_for_rejected_run_and_dedups_recurrence() {
    let db = test_db_async().await;
    let sse = sse();
    let r1 = report("run_1", Outcome::Rejected, Some(EndReason::Policy), Some("t1"));
    ticket_for_run(&db, &sse, &r1, None, Some("thing:t1:event:temp_high")).await;
    let tickets = db.list_tickets("ws1", None, 20, 0).await.unwrap();
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].state, "open");
    assert_eq!(
        tickets[0].title, "thing:t1:event:temp_high",
        "title 去「触发:」前缀取首行"
    );

    // 同一故障复发 → 不开新票，复发折叠带 run_id
    let r2 = report("run_2", Outcome::Rejected, Some(EndReason::Policy), Some("t1"));
    ticket_for_run(&db, &sse, &r2, None, Some("thing:t1:event:temp_high")).await;
    let tickets = db.list_tickets("ws1", None, 20, 0).await.unwrap();
    assert_eq!(tickets.len(), 1, "活跃去重——不开第二张");
    let events = db.list_ticket_events(tickets[0].id).await.unwrap();
    assert_eq!(events.len(), 1);
    let payload: serde_json::Value = serde_json::from_str(events[0].payload.as_ref().unwrap()).unwrap();
    assert_eq!(payload["agent_run_id"], "run_2", "复发事件带 run_id 供钻取");
}

#[test]
fn acted_and_no_action_needed_do_not_ticket() {
    assert!(!should_ticket(Outcome::Acted));
    assert!(!should_ticket(Outcome::NoActionNeeded));
    assert!(should_ticket(Outcome::Failed));
    assert!(should_ticket(Outcome::BudgetExceeded));
    assert!(should_ticket(Outcome::Rejected));
}

#[tokio::test]
async fn subscriber_consumes_run_recorded_events() {
    let db = Arc::new(test_db_async().await);
    let sse = sse();
    let bus = AgentEventBus::new(64);
    let rx = bus.subscribe();
    let shutdown = tokio_util::sync::CancellationToken::new();
    let token = shutdown.clone();
    let db2 = Arc::clone(&db);
    let sse2 = Arc::clone(&sse);
    let handle = tokio::spawn(async move {
        run_ticket_subscriber(rx, db2, sse2, token).await;
    });

    bus.emit(AgentEventKind::RunRecorded {
        report: Box::new(report(
            "run_evt",
            Outcome::BudgetExceeded,
            Some(EndReason::Budget),
            Some("t1"),
        )),
        problem_key: None,
        dedup_key: Some("thing:t1:event:temp_high".to_string()),
    });

    // 轮询等待消费（有界，不挂死）
    let mut found = false;
    for _ in 0..200 {
        if !db.list_tickets("ws1", None, 20, 0).await.unwrap().is_empty() {
            found = true;
            break;
        }
        tokio::task::yield_now().await;
    }
    shutdown.cancel();
    let _ = handle.await;
    assert!(found, "subscriber should ticket the emitted failed run");
}

#[test]
fn failure_hash_source_rules() {
    let r = report("run_h", Outcome::Rejected, Some(EndReason::Policy), Some("t1"));
    // problem_key 优先
    assert_eq!(failure_hash(&r, Some("pk-1"), Some("thing:t1:event:x")), "pk:pk-1");
    // ThingEvent dedup_key 次之
    assert_eq!(failure_hash(&r, None, Some("thing:t1:event:x")), "dk:thing:t1:event:x");
    // Timer dedup_key（过粗）→ hash 回退
    let h = failure_hash(&r, None, Some("timer:ws1"));
    assert!(h.starts_with("hash:policy:"), "timer 源走 hash 回退: {h}");
    // 无键 → hash 回退
    assert!(failure_hash(&r, None, None).starts_with("hash:policy:"));
}

#[test]
fn normalize_both_directions() {
    // 欠 normalize 方向：run_id/数字差异被折叠 → 同故障同 hash
    let a = normalize("执行被预算截断（工具调用超过 3 次）run_abc123");
    let b = normalize("执行被预算截断（工具调用超过 5 次）run_xyz789");
    assert_eq!(a, b, "数字串与 id 应折叠: {a} vs {b}");
    // 过 normalize 方向：语义骨架保留（含 CJK）→ 不同故障不同 hash
    let c = normalize("动作被策略拒绝（action_not_allowed）");
    let d = normalize("LLM 失败（turn 返回错误）");
    assert_ne!(c, d, "不同语义无碰撞");
    assert!(c.contains("策略拒绝"), "CJK 保留: {c}");
}

#[test]
fn briefing_contract_fields() {
    let r = report("run_b", Outcome::Rejected, Some(EndReason::Policy), Some("t1"));
    let b = build_briefing(&r);
    assert!(b["problem"].is_string());
    assert_eq!(b["steps_attempted"][0]["action"], "t1.reboot");
    assert_eq!(b["steps_attempted"][0]["result"], "失败");
    assert_eq!(b["last_error"], "E-401 闸阀执行器超时");
    assert_eq!(b["failure_kind"], "policy");
    // Rejected 时 suggested = LLM 原文（summary）
    assert_eq!(b["suggested_next_steps"][0], r.summary);
    // Failed 时 suggested 为空
    let rf = report("run_f", Outcome::Failed, Some(EndReason::Llm), None);
    assert_eq!(build_briefing(&rf)["suggested_next_steps"].as_array().unwrap().len(), 0);
}

#[test]
fn title_strips_trigger_prefix_and_truncates() {
    let r = report("run_t", Outcome::Failed, Some(EndReason::Llm), None);
    let title = synthesize_title(&r);
    assert!(!title.starts_with("触发"), "title 不带「触发:」前缀: {title}");
    let long = RunReport {
        summary: format!("触发: {}", "长".repeat(200)),
        ..report("run_l", Outcome::Failed, None, None)
    };
    let t = synthesize_title(&long);
    assert!(t.chars().count() <= 81, "80 字符截断 + 省略号");
}

/// 闭环链（Success Criteria #1 的数据层验证）：Agent 失败 → 开票 → 人工解决
/// → resolution 经 TicketResolutionProvider 出现在下次 run 的 prompt 输入里。
#[tokio::test]
async fn closure_chain_resolution_flows_into_next_prompt() {
    use crate::domains::agent::host::ports::DbTicketResolutionProvider;
    use tinyiothub_agent::runtime::thing_agent::prompt::build_prompt;
    use tinyiothub_agent::runtime::thing_agent::traits::TicketResolutionProvider;
    use tinyiothub_agent::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};

    let db = Arc::new(test_db_async().await);
    let sse = sse();

    // 1. Agent 自治失败（策略全拒）→ 订阅者开票
    let r = report("run_chain", Outcome::Rejected, Some(EndReason::Policy), Some("t1"));
    ticket_for_run(&db, &sse, &r, None, Some("thing:t1:event:temp_high")).await;
    let tickets = db.list_tickets("ws1", None, 20, 0).await.unwrap();
    assert_eq!(tickets.len(), 1);
    let id = tickets[0].id;

    // 2. 人工认领 → 开始处理 → 解决（必填 resolution）
    db.claim_ticket(id, "wang").await.unwrap();
    db.start_ticket(id, "wang").await.unwrap();
    db.resolve_ticket(id, "wang", "现场更换轴承 NSK-6205，复位闸阀执行器")
        .await
        .unwrap();

    // 3. 下一次 run 的 prompt 注入源（cloud 适配器 → db 真源）
    let provider = DbTicketResolutionProvider::new(db.clone());
    let resolutions = provider.recent_resolutions("ws1", Some("t1"), 5).await.unwrap();
    assert_eq!(resolutions.len(), 1);
    assert!(resolutions[0].1.contains("NSK-6205"));

    // 4. prompt 组装：解法出现在 <ticket_resolutions> 段（闭环成立）
    let signal = WakeSignal {
        workspace_id: "ws1".to_string(),
        priority: Priority::High,
        source: TriggerSource::ThingEvent {
            thing_id: "t1".to_string(),
            event_name: "temp_high".to_string(),
            event_id: 99,
            level: 3,
            data: serde_json::json!({"temp": 88}),
        },
        dedup_key: Some("thing:t1:event:temp_high".to_string()),
    };
    let prompt = build_prompt(&signal, &[], &[], &["reboot".to_string()], &resolutions);
    assert!(prompt.contains("<ticket_resolutions>"));
    assert!(prompt.contains("现场更换轴承 NSK-6205"), "解法应出现在 prompt 里");
    assert!(prompt.contains("不可信人工输入"), "注入段必须带不可信标注");
}
