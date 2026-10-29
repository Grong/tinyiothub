//! T8 runtime evals: deterministic complete Task prompts, media capability at
//! the provider boundary, evidence-only traces, and local integration latency.
//!
//! All timings in this file are local in-process integration evidence. They are
//! not production p99 numbers and must not be read as such.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use rig_core::completion::{CompletionModel, CompletionRequest};
use rig_core::message::{
    DocumentSourceKind, Image, ImageMediaType, Message, ToolCallId, ToolResult as RigToolResult, ToolResultContent,
    UserContent,
};
use serde_json::json;
use tinyiothub_agent::adapters::rig::provider::PortModelAsRig;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::provider::{ChatMessage, ChatRequest, ChatResponse, ModelProvider};
use tinyiothub_agent::port::runtime::AgentLoop;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_agent::runtime::events::AgentEventBus;
use tinyiothub_agent::runtime::thing_agent::active_runs::ActiveRunRegistry;
use tinyiothub_agent::runtime::thing_agent::manager::AutonomousAgentProvider;
use tinyiothub_agent::runtime::thing_agent::runner::{AgentHandle, RunContext, RunContextInner, Runner};
use tinyiothub_agent::runtime::thing_agent::task::coordinator::TaskCoordinator;
use tinyiothub_agent::runtime::thing_agent::task::prompt::{TaskPromptApproval, build_task_prompt};
use tinyiothub_agent::runtime::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_cloud::Db;
use tinyiothub_cloud::domains::agent::runtime::DbTaskStore;
use tinyiothub_cloud::domains::agent::runtime::stream::{RuntimeFrame, RuntimeRecorder, RuntimeStreamHub};
use tinyiothub_cloud::shared::runtime_ports::{DbDeviceJobStore, WatchRuleStoreAdapter};
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::watch::{WatchCondition, WatchRule};
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver};
use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
use tinyiothub_runtime::job::service::{JobRequest, JobService};
use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobWorkerLocator};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ── Step 2a: prompt completeness and determinism ─────────────────────────────

struct PromptFixture {
    task: AgentTask,
    runs: Vec<RunReport>,
    jobs: Vec<DeviceJob>,
    watches: Vec<WatchRule>,
    approvals: Vec<TaskPromptApproval>,
    snapshot: String,
    pending_ids: Vec<String>,
    latest_stage_result: String,
}

impl PromptFixture {
    fn large_task_with_unordered_facts() -> Self {
        let now = Utc::now();
        let task = AgentTask {
            id: "task_eval".into(),
            workspace_id: "ws_eval".into(),
            idempotency_key: "eval-key".into(),
            objective: "Keep the line within declared limits and verify the final state".into(),
            session_key: None,
            created_by: "eval-user".into(),
            status: TaskStatus::Active,
            current_run_id: Some("run_new".into()),
            state_version: 3,
            created_at: now,
            updated_at: now,
            ended_at: None,
        };
        let mut jobs = (0..12)
            .map(|index| DeviceJob {
                id: format!("job_{index:02}"),
                workspace_id: "ws_eval".into(),
                device_id: format!("device_{index:02}"),
                tool: if index % 2 == 0 { "read_device" } else { "set_device" }.into(),
                args_json: json!({"source": format!("source_{index:02}")}),
                state: DeviceJobState::Running,
                stop_requested_at: None,
                progress_json: Some(json!({"percent": index})),
                result_json: Some(json!({"stage": format!("calibrating-{index:02}")})),
                task_id: Some("task_eval".into()),
                run_id: Some("run_new".into()),
                tool_call_id: Some(format!("call_{index:02}")),
                driver_call_id: None,
                timeout_ms: 30_000,
                started_at: now,
                ended_at: None,
            })
            .collect::<Vec<_>>();
        // Unordered input plus terminal noise: the prompt must order active facts
        // and must not present terminal jobs as pending.
        jobs.reverse();
        jobs.push(DeviceJob {
            id: "job_terminal".into(),
            workspace_id: "ws_eval".into(),
            device_id: "device_terminal".into(),
            tool: "read_device".into(),
            args_json: json!({}),
            state: DeviceJobState::Done,
            stop_requested_at: None,
            progress_json: None,
            result_json: Some(json!({"stage": "finished"})),
            task_id: Some("task_eval".into()),
            run_id: Some("run_old".into()),
            tool_call_id: None,
            driver_call_id: None,
            timeout_ms: 1_000,
            started_at: now,
            ended_at: Some(now),
        });
        let watches = (0..3)
            .rev()
            .map(|index| WatchRule {
                id: format!("watch_{index:02}"),
                workspace_id: "ws_eval".into(),
                condition: WatchCondition::Availability {
                    thing_id: format!("device_{index:02}"),
                    online: true,
                },
                persistent: false,
                timeout_secs: 60,
                created_at: now.to_rfc3339(),
                owner_task_id: Some("task_eval".into()),
            })
            .collect::<Vec<_>>();
        let approvals = vec![
            TaskPromptApproval {
                approval_id: "approval_b".into(),
                status: "pending".into(),
                detail: "second".into(),
            },
            TaskPromptApproval {
                approval_id: "approval_a".into(),
                status: "pending".into(),
                detail: "first".into(),
            },
        ];
        let latest_stage_result = json!({"stage": "calibrating-11"}).to_string();
        let mut pending_ids = jobs
            .iter()
            .filter(|job| job.state.is_active())
            .map(|job| job.id.clone())
            .collect::<Vec<_>>();
        pending_ids.extend(watches.iter().map(|watch| watch.id.clone()));
        pending_ids.extend(approvals.iter().map(|approval| approval.approval_id.clone()));
        Self {
            task,
            runs: vec![
                report("run_old", "old result"),
                report("run_new", "stage result: calibration complete"),
            ],
            jobs,
            watches,
            approvals,
            snapshot: "s".repeat(8_192),
            pending_ids,
            latest_stage_result,
        }
    }
}

fn report(run_id: &str, summary: &str) -> RunReport {
    RunReport {
        run_id: run_id.into(),
        workspace_id: "ws_eval".into(),
        trigger: "task".into(),
        outcome: Outcome::Acted,
        summary: summary.into(),
        actions: vec![],
        verified: true,
        duration_ms: 10,
        tool_calls: 1,
        tokens: 0,
        end_reason: None,
        thing_id: None,
        trace: vec![],
        task_count: None,
        snapshot: None,
        task_id: Some("task_eval".into()),
        wake_event_id: None,
    }
}

#[test]
fn task_prompt_is_complete_and_deterministic() {
    let fixture = PromptFixture::large_task_with_unordered_facts();
    let a = build_task_prompt(
        &fixture.task,
        &fixture.runs,
        &fixture.jobs,
        &fixture.watches,
        &fixture.approvals,
        &fixture.snapshot,
        None,
    );
    let b = build_task_prompt(
        &fixture.task,
        &fixture.runs,
        &fixture.jobs,
        &fixture.watches,
        &fixture.approvals,
        &fixture.snapshot,
        None,
    );
    assert_eq!(a, b, "prompt is not deterministic");
    for id in &fixture.pending_ids {
        assert!(a.contains(id), "missing pending fact {id}");
    }
    assert!(
        a.contains("stage result: calibration complete"),
        "latest run result is missing"
    );
    assert!(
        a.contains(&fixture.latest_stage_result),
        "latest Job stage result is missing"
    );
    assert!(a.contains("run_new"));
    assert!(!a.contains("run_old"), "stale run leaked into the prompt");
    assert!(
        !a.contains("job_terminal"),
        "terminal Job must not be presented as pending"
    );
    assert!(a.len() > fixture.snapshot.len(), "total prompt must not be truncated");
    assert_eq!(a.matches(&fixture.snapshot).count(), 1);
}

// ── Step 2b: media capability at the provider boundary ───────────────────────

const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\nT8_EXACT_PNG";
const JPEG_BYTES: &[u8] = b"\xff\xd8\xff\xe0T8_EXACT_JPEG";

#[derive(Default)]
struct RecordingProvider {
    vision: bool,
    seen: Mutex<Vec<ChatMessage>>,
}

impl Attributable for RecordingProvider {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "runtime-eval-provider"
    }
}

#[async_trait]
impl ModelProvider for RecordingProvider {
    fn supports_vision(&self) -> bool {
        self.vision
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        self.seen.lock().expect("seen").extend_from_slice(request.messages);
        Ok(ChatResponse {
            text: Some("ok".into()),
            ..ChatResponse::default()
        })
    }
}

fn image_request(bytes: &[u8], media_type: ImageMediaType) -> CompletionRequest {
    let evidence = json!({
        "evidenceId": "evidence-eval-1",
        "mime": if media_type == ImageMediaType::PNG { "image/png" } else { "image/jpeg" },
        "sourceSeq": 7,
        "mediaTime": "2026-10-27T00:00:00Z"
    });
    let image = Image {
        data: DocumentSourceKind::Raw(bytes.to_vec()),
        media_type: Some(media_type),
        detail: None,
        additional_params: rig_core::message::AdditionalParams::from_entries([("tinyiothub_evidence", evidence)]),
    };
    CompletionRequest {
        model: Some("eval-model".into()),
        preamble: None,
        chat_history: vec![Message::User {
            content: vec![UserContent::ToolResult(RigToolResult {
                call: ToolCallId::new_or_mint("call-eval-1"),
                provider: None,
                name: "capture".into(),
                content: vec![ToolResultContent::Image(image)],
            })],
        }],
        documents: vec![],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    }
}

#[tokio::test]
async fn mock_vision_receives_exact_bytes_and_trace_keeps_only_evidence_refs() {
    for (bytes, media_type) in [(PNG_BYTES, ImageMediaType::PNG), (JPEG_BYTES, ImageMediaType::JPEG)] {
        let provider = Arc::new(RecordingProvider {
            vision: true,
            seen: Mutex::default(),
        });
        let model = PortModelAsRig::new(provider.clone(), "eval-model".into());
        model
            .completion(image_request(bytes, media_type.clone()))
            .await
            .expect("vision provider accepts image");
        let seen = provider.seen.lock().expect("seen").clone();
        let attachment = seen
            .iter()
            .flat_map(|message| message.attachments.iter())
            .find(|attachment| attachment.evidence_id == "evidence-eval-1")
            .expect("image attachment reached the provider");
        assert_eq!(attachment.bytes, bytes, "provider byte payload changed");
        assert_eq!(attachment.source_seq, 7);
        assert_eq!(
            attachment.mime,
            if media_type == ImageMediaType::PNG {
                "image/png"
            } else {
                "image/jpeg"
            }
        );
    }

    let attachment = tinyiothub_core::media::MediaAttachment {
        evidence_id: "evidence-eval-1".into(),
        mime: "image/png".into(),
        bytes: PNG_BYTES.to_vec(),
        source_seq: 7,
        media_time: chrono::DateTime::parse_from_rfc3339("2026-10-27T00:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc),
    };
    let report = Runner::new()
        .execute(
            Arc::new(tokio::sync::Mutex::new(EvidenceAgent { attachment })),
            "prompt".into(),
            RunContext::new("run_eval".into(), "ws_eval".into(), "eval".into()),
            CancellationToken::new(),
        )
        .await
        .report;
    assert_eq!(report.trace.len(), 1);
    let evidence = &report.trace[0].evidence;
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].evidence_id, "evidence-eval-1");
    assert_eq!(evidence[0].mime, "image/png");
    assert_eq!(evidence[0].source_seq, 7);
    let serialized = serde_json::to_string(&report).expect("report JSON");
    assert!(
        !serialized.contains("T8_EXACT_PNG"),
        "trace/report leaked raw image bytes"
    );
    // A weak literal check is not enough: search every common encoding an
    // accidental attachment or storage reference could take.
    let base64_engine = base64::engine::general_purpose::STANDARD;
    let forbidden = [
        base64_engine.encode(PNG_BYTES),
        hex::encode(PNG_BYTES),
        "\\u{89}PNG".to_string(),
        "[137,80,78,71".to_string(),
        "\\u{ff}\\u{d8}\\u{ff}".to_string(),
        "[255,216,255".to_string(),
    ];
    for encoding in forbidden {
        assert!(
            !serialized.contains(&encoding),
            "trace/report leaked image bytes encoded as {encoding}"
        );
    }
    for marker in [
        "storagePath",
        "storage_path",
        "T8_EXACT_PNG",
        "T8_EXACT_JPEG",
        "signed_url",
        "sig=",
    ] {
        assert!(
            !serialized.contains(marker),
            "trace/report leaked sensitive marker {marker}"
        );
    }
}

#[tokio::test]
async fn non_vision_provider_reports_image_unavailable_without_calling_the_model() {
    let provider = Arc::new(RecordingProvider {
        vision: false,
        seen: Mutex::default(),
    });
    let model = PortModelAsRig::new(provider.clone(), "eval-model".into());
    let error = model
        .completion(image_request(PNG_BYTES, ImageMediaType::PNG))
        .await
        .expect_err("non-vision provider must fail closed");
    assert!(
        error.to_string().contains("image_unavailable"),
        "unexpected non-vision error: {error}"
    );
    assert!(
        provider.seen.lock().expect("seen").is_empty(),
        "non-vision provider must not receive the request"
    );
}

struct EvidenceAgent {
    attachment: tinyiothub_core::media::MediaAttachment,
}

#[async_trait]
impl AgentLoop for EvidenceAgent {
    async fn turn_streamed(
        &self,
        _user_message: &str,
        event_tx: mpsc::Sender<TurnEvent>,
        _cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        event_tx
            .send(TurnEvent::ToolCall {
                id: "call_eval".into(),
                name: "capture".into(),
                args: json!({"deviceId": "device_eval"}),
            })
            .await?;
        event_tx
            .send(TurnEvent::ToolResult {
                id: "call_eval".into(),
                name: "capture".into(),
                output: "frame captured".into(),
                attachments: vec![self.attachment.clone()],
            })
            .await?;
        Ok("captured".into())
    }

    async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
        Ok("captured".into())
    }

    async fn clear_history(&self) {}

    async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── Step 2c: watch→Run and Job→UI local integration timings ──────────────────

async fn db_fixture() -> (Arc<Db>, RuntimeRecorder, Arc<RuntimeStreamHub>) {
    let pool = tinyiothub_storage::test_helpers::test_pool().await;
    let db = Arc::new(Db::new(pool));
    tinyiothub_storage::seed::seed_system(&db).await.expect("system seed");
    sqlx::query(
        "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) \
         VALUES ('ws_eval','eval','tenant-default-001','2025-01-01','2025-01-01')",
    )
    .execute(db.pool())
    .await
    .expect("workspace");
    let hub = Arc::new(RuntimeStreamHub::default());
    let recorder = RuntimeRecorder::new(db.clone(), hub.clone());
    (db, recorder, hub)
}

fn task() -> AgentTask {
    let now = Utc::now();
    AgentTask {
        id: "task_eval".into(),
        workspace_id: "ws_eval".into(),
        idempotency_key: "eval-key".into(),
        objective: "keep the line within limits".into(),
        session_key: None,
        created_by: "eval-user".into(),
        status: TaskStatus::Active,
        current_run_id: None,
        state_version: 0,
        created_at: now,
        updated_at: now,
        ended_at: None,
    }
}

fn wake(source: TriggerSource) -> TaskWake {
    let now = Utc::now();
    TaskWake {
        task_id: "task_eval".into(),
        event_id: "wake_eval".into(),
        signal_json: serde_json::to_string(&WakeSignal {
            workspace_id: "ws_eval".into(),
            priority: Priority::Normal,
            dedup_key: Some("watch:wake_eval".into()),
            source,
        })
        .expect("wake JSON"),
        dispatch_state: WakeDispatchState::Pending,
        created_at: now,
        dispatched_at: None,
        resolved_at: None,
    }
}

struct ImmediateAgent;

#[async_trait]
impl AgentLoop for ImmediateAgent {
    async fn turn_streamed(
        &self,
        _user_message: &str,
        _event_tx: mpsc::Sender<TurnEvent>,
        _cancel_token: Option<CancellationToken>,
    ) -> anyhow::Result<String> {
        Ok("done".into())
    }
    async fn run_single(&self, _message: &str) -> anyhow::Result<String> {
        Ok("done".into())
    }
    async fn clear_history(&self) {}
    async fn seed_history(&self, _messages: &[ChatMessage]) -> anyhow::Result<()> {
        Ok(())
    }
}

struct ImmediateProvider;

#[async_trait]
impl AutonomousAgentProvider for ImmediateProvider {
    async fn get_or_create(
        &self,
        _workspace_id: &str,
        _ctx: Arc<tokio::sync::RwLock<RunContextInner>>,
    ) -> anyhow::Result<AgentHandle> {
        Ok(Arc::new(tokio::sync::Mutex::new(ImmediateAgent)))
    }
    fn invalidate(&self, _workspace_id: &str) {}
}

fn coordinator(db: &Arc<Db>, recorder: RuntimeRecorder) -> TaskCoordinator {
    TaskCoordinator::new(
        Arc::new(DbTaskStore::new(db.clone()).with_recorder(recorder)),
        None,
        Arc::new(WatchRuleStoreAdapter::new((**db).clone())),
        Arc::new(dashmap::DashMap::new()),
        Arc::new(AgentEventBus::new(32)),
        ActiveRunRegistry::new(8),
        Arc::new(Runner::new()),
        Arc::new(ImmediateProvider),
    )
}

async fn next_matching_frame(
    connection: &mut tinyiothub_cloud::domains::agent::runtime::stream::RuntimeConnection,
    deadline: Duration,
    mut matches: impl FnMut(&tinyiothub_storage::runtime_event::RuntimeEvent) -> bool,
) -> Option<tinyiothub_storage::runtime_event::RuntimeEvent> {
    let started = Instant::now();
    loop {
        let remaining = deadline.checked_sub(started.elapsed())?;
        let frame = tokio::time::timeout(remaining, connection.next_frame()).await.ok()??;
        if let RuntimeFrame::Event { event } = frame
            && matches(&event)
        {
            return Some(*event);
        }
    }
}

#[tokio::test]
async fn local_watch_to_run_starts_under_two_seconds() {
    let (db, recorder, hub) = db_fixture().await;
    let store = DbTaskStore::new(db.clone()).with_recorder(recorder.clone());
    store
        .create_task_with_wake(
            &task(),
            &wake(TriggerSource::Watch {
                rule_id: "watch_eval".into(),
                summary: "declared threshold crossed".into(),
                current_value: Some("31.5".into()),
                health: None,
                zone: None,
            }),
        )
        .await
        .expect("create task");
    let coordinator = coordinator(&db, recorder);
    let mut connection = hub
        .connect(db.clone(), "ws_eval", "task_eval", None)
        .await
        .expect("connect stream");
    assert!(matches!(
        connection.next_frame().await,
        Some(RuntimeFrame::Snapshot { .. })
    ));

    let started = Instant::now();
    let advance = coordinator.advance("task_eval", "wake_eval").await.expect("advance");
    assert!(matches!(
        advance,
        tinyiothub_agent::runtime::thing_agent::task::coordinator::Advance::Started { .. }
    ));
    let event = next_matching_frame(&mut connection, Duration::from_secs(2), |event| {
        event.event_type == "run.started"
    })
    .await
    .expect("run.started frame");
    let elapsed = started.elapsed();
    assert_eq!(event.task_id.as_deref(), Some("task_eval"));
    assert!(
        elapsed < Duration::from_secs(2),
        "local watch→Run evidence exceeded 2s: {elapsed:?}"
    );
    eprintln!("local integration evidence (not production p99): watch→Run {elapsed:?}");
}

struct NoopWorker;

impl JobDeviceWorker for NoopWorker {
    fn submit(&self, _cmd: ThingCommand, _job_id: &str, _lease: Option<ResourceLease>) -> Result<(), Error> {
        Ok(())
    }
    fn cancel_job(&self, _job_id: &str) -> bool {
        true
    }
    fn has_recoverable_call(&self, _job_id: &str) -> bool {
        true
    }
    fn complete_job(&self, _job_id: &str) {}
}

struct NoopWorkers(Arc<NoopWorker>);

impl JobWorkerLocator for NoopWorkers {
    fn worker_for(&self, _device_id: &str) -> Option<Arc<dyn JobDeviceWorker>> {
        Some(self.0.clone())
    }
}

struct EvalValidator;

#[async_trait]
impl AccessCallValidator for EvalValidator {
    async fn validate(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        tool: &str,
        _params: Option<&serde_json::Value>,
    ) -> Result<ValidatedCall, CallError> {
        let manifest_tool = serde_json::from_value(json!({ "name": tool }))
            .map_err(|error| CallError::InvalidParams(error.to_string()))?;
        Ok(ValidatedCall {
            tool: manifest_tool,
            timeout_ms: 1_000,
            clamped: false,
        })
    }
}

struct EvalResources;

#[async_trait]
impl ResourceResolver for EvalResources {
    async fn resolve_action(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _tool_name: &str,
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![])
    }
    async fn resolve_properties(
        &self,
        _workspace_id: &str,
        _thing_id: &str,
        _properties: &[String],
    ) -> Result<Vec<ResourceClaim>, String> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn local_job_to_ui_frame_under_one_second() {
    let (db, recorder, hub) = db_fixture().await;
    let store = DbTaskStore::new(db.clone()).with_recorder(recorder.clone());
    store
        .create_task_with_wake(&task(), &wake(TriggerSource::Timer))
        .await
        .expect("create task");
    let job_store = DbDeviceJobStore::new((*db).clone()).with_recorder(recorder.clone());
    let jobs = JobService::new(
        Arc::new(job_store),
        Arc::new(NoopWorkers(Arc::new(NoopWorker))),
        Arc::new(EvalValidator),
        Arc::new(EvalResources),
        Arc::new(ArbitrationRegistry::default()),
    );
    let mut connection = hub
        .connect(db.clone(), "ws_eval", "task_eval", None)
        .await
        .expect("connect stream");
    assert!(matches!(
        connection.next_frame().await,
        Some(RuntimeFrame::Snapshot { .. })
    ));

    let started = Instant::now();
    let job = jobs
        .submit(JobRequest {
            job_id: Some("job_eval".into()),
            workspace_id: "ws_eval".into(),
            device_id: "device_eval".into(),
            tool: "read_device".into(),
            args_json: json!({"source": "temperature"}),
            task_id: Some("task_eval".into()),
            run_id: Some("run_eval".into()),
            tool_call_id: Some("call_eval".into()),
        })
        .await
        .expect("submit job");
    let event = next_matching_frame(&mut connection, Duration::from_secs(1), |event| {
        event.job_id.as_deref() == Some("job_eval")
    })
    .await
    .expect("Job UI frame");
    let elapsed = started.elapsed();
    assert_eq!(event.task_id.as_deref(), Some("task_eval"));
    assert_eq!(event.device_id.as_deref(), Some("device_eval"));
    assert!(job.id == "job_eval");
    assert!(
        elapsed < Duration::from_secs(1),
        "local Job→UI evidence exceeded 1s: {elapsed:?}"
    );
    eprintln!("local integration evidence (not production p99): Job→UI {elapsed:?}");
}

// ── Step 2d: opt-in live provider eval (never part of CI) ────────────────────

/// Requires an already-authorized staging provider configuration:
/// `TINYIOTHUB_EVAL_BASE_URL`, `TINYIOTHUB_EVAL_API_KEY`, optional
/// `TINYIOTHUB_EVAL_MODEL`. This test never prints the key, private objective
/// text, signed URLs, or image payloads, and mock successes are never used to
/// infer live-provider behaviour.
#[tokio::test]
#[ignore = "live staging provider configuration required; never run in CI"]
async fn live_provider_eval_opt_in() {
    let (Ok(base_url), Ok(api_key)) = (
        std::env::var("TINYIOTHUB_EVAL_BASE_URL"),
        std::env::var("TINYIOTHUB_EVAL_API_KEY"),
    ) else {
        return;
    };
    let model = std::env::var("TINYIOTHUB_EVAL_MODEL").unwrap_or_else(|_| "staging-model".into());
    let response = reqwest::Client::new()
        .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
        .bearer_auth(&api_key)
        .json(&json!({
            "model": model,
            "messages": [{"role": "user", "content": "Reply with the single word: ready"}],
            "max_tokens": 8,
            "temperature": 0
        }))
        .send()
        .await
        .expect("staging provider request");
    assert!(
        response.status().is_success(),
        "staging provider returned status {}",
        response.status()
    );
    let body: serde_json::Value = response.json().await.expect("staging provider JSON");
    let text = body["choices"][0]["message"]["content"].as_str().unwrap_or_default();
    assert!(
        text.to_ascii_lowercase().contains("ready"),
        "staging provider reply did not match the eval contract (content not printed)"
    );
}
