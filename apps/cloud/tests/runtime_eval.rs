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
use tinyiothub_agent::harness::{
    EffectReconciler, HarnessError, ReconcileOutcome, SubagentExecutor, TaskOutcome, reduce,
};
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::provider::{ChatMessage, ChatRequest, ChatResponse, ModelProvider};
use tinyiothub_agent::port::runtime::AgentLoop;
use tinyiothub_agent::port::task::TaskStore;
use tinyiothub_cloud::Db;
use tinyiothub_cloud::domains::agent::events::AgentEventBus;
use tinyiothub_cloud::domains::agent::runtime::DbTaskStore;
use tinyiothub_cloud::domains::agent::runtime::stream::{RuntimeFrame, RuntimeRecorder, RuntimeStreamHub};
use tinyiothub_cloud::domains::agent::runtime::{EffectDispatcher, EffectRecovery, EffectSender, RecoveryMode};
use tinyiothub_cloud::domains::agent::thing_agent::active_runs::ActiveRunRegistry;
use tinyiothub_cloud::domains::agent::thing_agent::manager::AutonomousAgentProvider;
use tinyiothub_cloud::domains::agent::thing_agent::runner::{AgentHandle, RunContext, RunContextInner, Runner};
use tinyiothub_cloud::domains::agent::thing_agent::task::coordinator::TaskCoordinator;
use tinyiothub_cloud::domains::agent::thing_agent::task::prompt::{TaskPromptApproval, build_task_prompt};
use tinyiothub_cloud::domains::agent::thing_agent::types::{Priority, TriggerSource, WakeSignal};
use tinyiothub_cloud::shared::runtime_ports::{DbDeviceJobStore, WatchRuleStoreAdapter};
use tinyiothub_core::agent_runs::{Outcome, RunReport};
use tinyiothub_core::agent_task::{AgentTask, TaskStatus, TaskWake, WakeDispatchState};
use tinyiothub_core::device_job::{DeviceJob, DeviceJobState};
use tinyiothub_core::error::Error;
use tinyiothub_core::harness::{
    DeviceFactsSnapshot, EffectIntent, EffectState, ExecutionMode, HarnessFact, HarnessState, WorkflowDefinition,
    WorkflowRevision, WorkflowTrigger,
};
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::watch::{WatchCondition, WatchRule};
use tinyiothub_runtime::access::resources::{ResourceClaim, ResourceResolver};
use tinyiothub_runtime::access::validator::{AccessCallValidator, CallError, ValidatedCall};
use tinyiothub_runtime::arbitration::{ArbitrationRegistry, ResourceLease};
use tinyiothub_runtime::job::service::{JobRequest, JobService};
use tinyiothub_runtime::job::worker_adapter::{JobDeviceWorker, JobWorkerLocator};
use tinyiothub_storage::test_helpers::{WorkspaceFixture, fixture_workspace};
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
        if let RuntimeFrame::Event { event, .. } = frame
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
        tinyiothub_cloud::domains::agent::thing_agent::task::coordinator::Advance::Started { .. }
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
/// `TINYIOTHUB_EVAL_MODEL` and `TINYIOTHUB_EVAL_PROVIDER`. It runs the same
/// `evals/scenarios.json` harness records as the scripted eval, records
/// provider/model/version, and keeps live results separate from scripted
/// evidence. This test never prints the key, private objective text, signed
/// URLs, or image payloads, and mock successes are never used to infer
/// live-provider behaviour. Without authorized configuration it reports
/// `live=not run` and returns.
#[tokio::test]
#[ignore = "live staging provider configuration required; never run in CI"]
async fn live_provider_eval_opt_in() {
    let (Ok(base_url), Ok(api_key)) = (
        std::env::var("TINYIOTHUB_EVAL_BASE_URL"),
        std::env::var("TINYIOTHUB_EVAL_API_KEY"),
    ) else {
        eprintln!("live=not run (authorized TINYIOTHUB_EVAL_BASE_URL/TINYIOTHUB_EVAL_API_KEY not configured)");
        return;
    };
    let model = std::env::var("TINYIOTHUB_EVAL_MODEL").unwrap_or_else(|_| "staging-model".into());
    let provider = std::env::var("TINYIOTHUB_EVAL_PROVIDER").unwrap_or_else(|_| "openai-compatible".into());
    let client = reqwest::Client::new();

    for scenario in harness_eval_scenarios() {
        let response = client
            .post(format!("{}/chat/completions", base_url.trim_end_matches('/')))
            .bearer_auth(&api_key)
            .json(&json!({
                "model": model,
                "messages": [{"role": "user", "content": harness_eval_prompt(&scenario)}],
                "max_tokens": 512,
                "temperature": 0
            }))
            .send()
            .await
            .expect("staging provider request");
        assert!(
            response.status().is_success(),
            "scenario {}: staging provider returned status {}",
            scenario.id,
            response.status()
        );
        let body: serde_json::Value = response.json().await.expect("staging provider JSON");
        let text = body["choices"][0]["message"]["content"].as_str().unwrap_or_default();
        let outcome: TaskOutcome = serde_json::from_str(extract_json_object(text).unwrap_or(text))
            .unwrap_or_else(|error| panic!("scenario {}: invalid TaskOutcome JSON: {error}", scenario.id));
        assert_eq!(
            outcome.evidence_refs.len(),
            scenario.expected.evidence_refs.len(),
            "scenario {}: evidence refs must be preserved",
            scenario.id
        );
        for forbidden in &scenario.forbidden_side_effects {
            assert!(
                !outcome.recommended_actions.iter().any(|action| action == forbidden),
                "scenario {}: recommended forbidden side effect {forbidden}",
                scenario.id
            );
        }
        let actual_model = body["model"].as_str().unwrap_or(&model);
        let version = body["system_fingerprint"].as_str().unwrap_or("unspecified");
        eprintln!(
            "live provider={provider} model={actual_model} version={version} scenario={} status={}",
            scenario.id, outcome.status
        );
    }
    eprintln!(
        "live results are separate from scripted evidence; scripted mock success does not imply live model quality"
    );
}

/// Accepts bare JSON or a single fenced ```json block from a live model.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end >= start).then_some(&text[start..=end])
}

// ── Step 2e: ToolDescriptor single declaration source ───────────────────────

struct DeclaredCameraTool;

impl Attributable for DeclaredCameraTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "DeclaredCameraTool"
    }
}

#[async_trait]
impl tinyiothub_agent::port::tool::Tool for DeclaredCameraTool {
    fn name(&self) -> &str {
        "inspect_camera"
    }
    fn description(&self) -> &str {
        "Inspect the current camera frame"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": { "type": "string" },
                "credentials": { "type": "object" }
            },
            "required": ["thingId"]
        })
    }
    fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
        let mut descriptor = tinyiothub_skills::registry::ToolDescriptor::declared(
            self.name(),
            self.description(),
            self.parameters_schema(),
        );
        descriptor.prompt_summary = "Use when the current visual state matters.".into();
        descriptor.effect_kind = tinyiothub_skills::registry::ToolEffectKind::Read;
        descriptor.required_capabilities = vec![tinyiothub_skills::registry::Capability::Vision];
        descriptor.presentation.label = "查看画面".into();
        descriptor.presentation.group = "device".into();
        descriptor.presentation.group_label = "设备管理".into();
        descriptor
    }
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<tinyiothub_agent::port::tool::ToolResult> {
        Ok(tinyiothub_agent::port::tool::ToolResult {
            success: true,
            output: "{}".into(),
            error: None,
            attachments: vec![],
        })
    }
}

fn declared_tool_registry() -> tinyiothub_cloud::domains::agent::tools::ToolRegistry {
    let registry = tinyiothub_cloud::domains::agent::tools::ToolRegistry::default();
    registry.register_provider(Arc::new(|_workspace: &str, _runtime| {
        vec![(
            Box::new(DeclaredCameraTool) as Box<dyn tinyiothub_agent::port::tool::Tool>,
            tinyiothub_skills::trust::ToolSafety::ReadOnly,
        )]
    }));
    registry
}

#[tokio::test]
async fn tool_descriptor_alone_drives_llm_subset_catalog_and_redacted_summary() {
    let registry = declared_tool_registry();
    let runtime = tinyiothub_cloud::domains::agent::tools::ToolRuntimeContext::default();

    let llm = registry
        .llm_tool_schema("ws_eval", &runtime, &[tinyiothub_skills::registry::Capability::Vision])
        .await;
    let tool = llm["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "inspect_camera")
        .expect("new declared tool enters the LLM subset without an allowlist edit");
    assert_eq!(
        tool["description"],
        "Inspect the current camera frame\n\nUse when the current visual state matters."
    );

    let catalog = registry.build_catalog("ws_eval", &runtime).await;
    let group = catalog["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .find(|group| group["id"] == "device")
        .expect("declared group");
    assert!(
        group["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["id"] == "inspect_camera" && tool["label"] == "查看画面")
    );

    let summary = registry
        .public_arguments_summary(
            "ws_eval",
            &runtime,
            "inspect_camera",
            &json!({
                "thingId": "camera-1",
                "credentials": {
                    "password": "nested-password",
                    "token": "nested-token"
                }
            }),
        )
        .await
        .expect("declared tool summary");
    assert!(!summary.contains("nested-password"), "{summary}");
    assert!(!summary.contains("nested-token"), "{summary}");
    assert!(summary.contains("[REDACTED]"), "{summary}");

    assert!(matches!(
        registry
            .public_arguments_summary("ws_eval", &runtime, "not_declared", &json!({}))
            .await,
        Err(tinyiothub_skills::registry::ToolRegistryError::UnknownTool { .. })
    ));
    assert!(matches!(
        registry
            .public_arguments_summary("ws_eval", &runtime, "inspect_camera", &json!({}))
            .await,
        Err(tinyiothub_skills::registry::ToolRegistryError::InvalidArguments { .. })
    ));
}

#[test]
fn tool_descriptor_workflow_policy_only_tightens_and_revision_mismatch_fails_closed() {
    use tinyiothub_skills::registry::{ApprovalPolicy, ToolDescriptor, verify_revision_policy};

    let mut descriptor = ToolDescriptor::declared("set_device", "Write a property", json!({"type": "object"}));
    descriptor.approval_policy = ApprovalPolicy::mandatory("device safety");
    let workflow = ApprovalPolicy::from_workflow_value(&json!({
        "requiresApproval": false,
        "timeoutSecs": 1
    }))
    .expect("workflow policy shape");
    let effective = descriptor.effective_approval(Some(&workflow));
    assert!(effective.mandatory, "a template must not disable mandatory approval");
    assert_eq!(
        effective.timeout_secs, 60,
        "a template must not shorten the approval window"
    );

    assert!(verify_revision_policy(&effective, &ApprovalPolicy::none()).is_err());
}

struct VisionCaptureTool;

impl Attributable for VisionCaptureTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "VisionCaptureTool"
    }
}

#[async_trait]
impl tinyiothub_agent::port::tool::Tool for VisionCaptureTool {
    fn name(&self) -> &str {
        "capture_video_frame"
    }
    fn description(&self) -> &str {
        "Capture a video frame"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type": "object", "properties": {}})
    }
    fn descriptor(&self) -> tinyiothub_skills::registry::ToolDescriptor {
        let mut descriptor = tinyiothub_skills::registry::ToolDescriptor::declared(
            self.name(),
            self.description(),
            self.parameters_schema(),
        );
        descriptor.required_capabilities = vec![tinyiothub_skills::registry::Capability::Video];
        descriptor
    }
    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<tinyiothub_agent::port::tool::ToolResult> {
        Ok(tinyiothub_agent::port::tool::ToolResult {
            success: true,
            output: "{}".into(),
            error: None,
            attachments: vec![],
        })
    }
}

struct SilentNonVisionProvider {
    calls: Arc<Mutex<usize>>,
}

impl Attributable for SilentNonVisionProvider {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        "SilentNonVisionProvider"
    }
}

#[async_trait]
impl ModelProvider for SilentNonVisionProvider {
    fn supports_vision(&self) -> bool {
        false
    }
    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        *self.calls.lock().expect("calls") += 1;
        Ok(ChatResponse::default())
    }
}

/// Drives the real `AutonomousAgentFactory` (not the emit helper): a declared
/// video tool on a non-vision provider must fail before the model call, write
/// `capability.unavailable`, and create zero Effects.
#[tokio::test]
async fn tool_descriptor_factory_capability_gate_emits_event_and_creates_zero_effects() {
    use tinyiothub_cloud::domains::agent::host::autonomous_factory::AutonomousAgentFactory;
    use tinyiothub_cloud::domains::agent::host::tools::ThingToolContext;
    use tinyiothub_cloud::domains::agent::runtime::stream::{RuntimeRecorder, RuntimeStreamHub};
    use tinyiothub_cloud::domains::agent::thing_agent::RunContextInner;
    use tinyiothub_cloud::domains::event::bus::ThingEventBus;
    use tinyiothub_cloud::domains::event::router::ThrottleState;
    use tinyiothub_storage::test_helpers::fixture_workspace;
    use tokio::sync::RwLock;

    let fixture = fixture_workspace("ws-tool-descriptor-capability").await;
    let db = Arc::new(fixture.db.clone());
    let recorder = RuntimeRecorder::new(db.clone(), Arc::new(RuntimeStreamHub::default()));

    let calls = Arc::new(Mutex::new(0));
    let provider_calls = Arc::clone(&calls);
    let factory = AutonomousAgentFactory::new(
        fixture.db.pool().clone(),
        db,
        Arc::new(ThingEventBus::new()),
        Arc::new(ThrottleState::new(60)),
        Arc::new(tinyiothub_agent::port::memory::NoopMemory),
        Arc::new(tinyiothub_agent::port::observer::NoopObserver),
        Arc::new(move || {
            Ok(Box::new(SilentNonVisionProvider {
                calls: Arc::clone(&provider_calls),
            }))
        }),
        "non-vision".into(),
        ThingToolContext {
            pending_actions: Some(Arc::new(dashmap::DashMap::new())),
            runtime_recorder: Some(recorder),
            ..Default::default()
        },
    )
    .with_extra_tools(Arc::new(|_workspace: &str| {
        vec![(
            Box::new(VisionCaptureTool) as Box<dyn tinyiothub_agent::port::tool::Tool>,
            tinyiothub_skills::trust::ToolSafety::ReadOnly,
        )]
    }));

    let ctx = Arc::new(RwLock::new(RunContextInner {
        run_id: "run-capability".into(),
        workspace_id: "ws-tool-descriptor-capability".into(),
        task_id: Some("task-capability".into()),
        ..Default::default()
    }));
    let error = match factory.get_or_create("ws-tool-descriptor-capability", ctx).await {
        Ok(_) => panic!("video capability must fail closed on a non-vision provider"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("capability.unavailable"), "{error}");
    assert_eq!(*calls.lock().expect("calls"), 0, "model must not be called");

    let events = fixture
        .db
        .runtime_events_after("ws-tool-descriptor-capability", None, Some("task-capability"), 10)
        .await
        .expect("runtime events");
    let event = events
        .iter()
        .find(|event| event.run_id.as_deref() == Some("run-capability"))
        .expect("capability event persisted");
    assert_eq!(event.event_type, "capability.unavailable");
    assert!(
        event.data["missingCapabilities"]
            .as_array()
            .is_some_and(|missing| missing.iter().any(|value| value == "video")),
        "{:?}",
        event.data
    );
    assert!(event.data["recovery"].as_str().is_some_and(|text| !text.is_empty()));

    let effects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM effect_intents")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("effect count");
    assert_eq!(effects, 0, "a capability failure must create zero Effects");
}

/// `request_json` is an internal dispatch/reconcile field. Public DTO
/// serialization and Debug output must not carry it (or its secrets); only the
/// descriptor-redacted summary may be exposed.
#[test]
fn tool_descriptor_public_dto_and_debug_never_expose_request_json() {
    let now = Utc::now();
    let intent = tinyiothub_core::harness::EffectIntent {
        effect_id: "effect-eval".into(),
        session_id: "session-eval".into(),
        task_id: "task-eval".into(),
        run_id: "run-eval".into(),
        tool_call_id: "call-eval".into(),
        tool_name: "set_device".into(),
        request_json: json!({"password": "internal-secret", "token": "internal-token"}),
        request_digest: "digest".into(),
        idempotency_key: "key".into(),
        external_ref: None,
        state: tinyiothub_core::harness::EffectState::Prepared,
        recovery_from: None,
        state_version: 1,
        result_json: None,
        created_at: now,
        updated_at: now,
    };

    let public = serde_json::to_string(&intent).expect("serialize");
    assert!(!public.contains("request_json"), "{public}");
    assert!(!public.contains("internal-secret"), "{public}");
    assert!(!public.contains("internal-token"), "{public}");

    let debug = format!("{intent:?}");
    assert!(!debug.contains("request_json"), "{debug}");
    assert!(!debug.contains("internal-secret"), "{debug}");
}

// ── Step 2f: same-source scripted and opt-in live Harness evals (T12) ───────
//
// `evals/scenarios.json` is the single scenario source for both the scripted
// orchestration eval below and the ignored live-provider eval. The scripted
// pass only proves orchestration (descriptor/gate/reducer/runner); it makes no
// claim about live model quality or real device safety.

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HarnessEvalScenario {
    id: String,
    kind: String,
    facts: String,
    expected: HarnessEvalExpectation,
    forbidden_side_effects: Vec<String>,
    note: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HarnessEvalExpectation {
    tools: Vec<String>,
    events: Vec<String>,
    status: String,
    #[serde(default)]
    evidence_refs: Vec<String>,
}

fn eval_scenario_document() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../evals/scenarios.json"
    )))
    .expect("evals/scenarios.json")
}

fn harness_eval_scenarios() -> Vec<HarnessEvalScenario> {
    serde_json::from_value(
        eval_scenario_document()
            .get("harness")
            .cloned()
            .expect("evals/scenarios.json must define a harness array"),
    )
    .expect("harness eval scenarios")
}

fn harness_eval_scenario(id: &str) -> HarnessEvalScenario {
    harness_eval_scenarios()
        .into_iter()
        .find(|scenario| scenario.id == id)
        .unwrap_or_else(|| panic!("missing harness eval scenario {id}"))
}

fn scenario_workspace(scenario: &HarnessEvalScenario) -> String {
    format!("ws-eval-{}", scenario.id)
}

/// Deterministic prompt for the ignored live eval. It is intentionally derived
/// only from the same scenario record used by the scripted eval, and it never
/// contains credentials or device secrets.
fn harness_eval_prompt(scenario: &HarnessEvalScenario) -> String {
    let evidence_refs = if scenario.expected.evidence_refs.is_empty() {
        "无".to_string()
    } else {
        scenario.expected.evidence_refs.join(", ")
    };
    format!(
        "你是 TinyIoTHub 的工具编排评估器。只依据给定场景输出一个 JSON 对象，不要调用或执行工具。\n\
         场景 id: {}\n\
         事实: {}\n\
         禁止的副作用: {}\n\
         必须原样保留的 evidenceRefs: {evidence_refs}\n\
         输出字段（camelCase）: status, summary, observations, evidenceRefs, recommendedActions。\n\
         只输出 JSON。",
        scenario.id,
        scenario.facts,
        scenario.forbidden_side_effects.join(", ")
    )
}

#[test]
fn harness_eval_scenarios_cover_required_boundaries() {
    let scenarios = harness_eval_scenarios();
    let ids: std::collections::BTreeSet<&str> = scenarios.iter().map(|scenario| scenario.id.as_str()).collect();
    for required in [
        "declared-tool-catalog",
        "non-vision-capability-failure",
        "denied-approval-no-duplicate",
        "unknown-effect-no-duplicate",
        "ack-not-physical-completion",
        "task-outcome-schema",
    ] {
        assert!(ids.contains(required), "missing required eval scenario {required}");
    }
    for scenario in &scenarios {
        assert!(!scenario.facts.trim().is_empty(), "{} has no input facts", scenario.id);
        assert!(!scenario.note.trim().is_empty(), "{} has no note", scenario.id);
    }
    assert!(
        scenarios.iter().any(|scenario| scenario.kind == "non_vision"
            && scenario
                .forbidden_side_effects
                .iter()
                .any(|effect| effect == "model_call")),
        "non-vision scenarios must forbid the model call"
    );
}

#[test]
fn live_eval_prompt_is_deterministic_and_secret_free() {
    let scenario = harness_eval_scenario("task-outcome-schema");
    let first = harness_eval_prompt(&scenario);
    let second = harness_eval_prompt(&scenario);
    assert_eq!(first, second, "live eval prompt must be deterministic");
    assert!(first.contains(&scenario.facts));
    assert!(first.contains(&scenario.id));
    for evidence in &scenario.expected.evidence_refs {
        assert!(first.contains(evidence), "prompt must carry evidence ref {evidence}");
    }
    for forbidden in ["TINYIOTHUB_EVAL_API_KEY", "password", "token"] {
        assert!(!first.contains(forbidden), "prompt leaked {forbidden}");
    }
}

#[tokio::test]
async fn scripted_eval_declared_tool_uses_descriptor_single_source() {
    let scenario = harness_eval_scenario("declared-tool-catalog");
    assert_eq!(scenario.kind, "tool_descriptor");
    let registry = declared_tool_registry();
    let runtime = tinyiothub_cloud::domains::agent::tools::ToolRuntimeContext::default();

    let llm = registry
        .llm_tool_schema("ws_eval", &runtime, &[tinyiothub_skills::registry::Capability::Vision])
        .await;
    let names: Vec<&str> = llm["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for tool in &scenario.expected.tools {
        assert!(
            names.contains(&tool.as_str()),
            "declared tool {tool} missing from the LLM subset: {names:?}"
        );
    }

    let catalog = registry.build_catalog("ws_eval", &runtime).await;
    for tool in &scenario.expected.tools {
        assert!(
            catalog["groups"]
                .as_array()
                .expect("groups")
                .iter()
                .any(|group| group["tools"]
                    .as_array()
                    .is_some_and(|tools| tools.iter().any(|entry| entry["id"] == tool.as_str()))),
            "declared tool {tool} missing from the catalog"
        );
    }
}

#[tokio::test]
async fn scripted_eval_non_vision_capability_fails_closed() {
    use tinyiothub_cloud::domains::agent::host::autonomous_factory::AutonomousAgentFactory;
    use tinyiothub_cloud::domains::agent::host::tools::ThingToolContext;
    use tinyiothub_cloud::domains::agent::thing_agent::RunContextInner;
    use tinyiothub_cloud::domains::event::bus::ThingEventBus;
    use tinyiothub_cloud::domains::event::router::ThrottleState;
    use tokio::sync::RwLock;

    let scenario = harness_eval_scenario("non-vision-capability-failure");
    assert_eq!(scenario.kind, "non_vision");
    let workspace = scenario_workspace(&scenario);

    // The declared tool is absent from the LLM subset when vision is missing.
    let registry = declared_tool_registry();
    let runtime = tinyiothub_cloud::domains::agent::tools::ToolRuntimeContext::default();
    let without_vision = registry.llm_tool_schema(&workspace, &runtime, &[]).await;
    assert!(
        !without_vision["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .any(|tool| tool["name"] == "inspect_camera"),
        "a vision-only declared tool must be filtered out without vision"
    );

    let fixture = fixture_workspace(&workspace).await;
    let db = Arc::new(fixture.db.clone());
    let recorder = RuntimeRecorder::new(db.clone(), Arc::new(RuntimeStreamHub::default()));
    let calls = Arc::new(Mutex::new(0));
    let provider_calls = Arc::clone(&calls);
    let factory = AutonomousAgentFactory::new(
        fixture.db.pool().clone(),
        db,
        Arc::new(ThingEventBus::new()),
        Arc::new(ThrottleState::new(60)),
        Arc::new(tinyiothub_agent::port::memory::NoopMemory),
        Arc::new(tinyiothub_agent::port::observer::NoopObserver),
        Arc::new(move || {
            Ok(Box::new(SilentNonVisionProvider {
                calls: Arc::clone(&provider_calls),
            }))
        }),
        "non-vision".into(),
        ThingToolContext {
            pending_actions: Some(Arc::new(dashmap::DashMap::new())),
            runtime_recorder: Some(recorder),
            ..Default::default()
        },
    )
    .with_extra_tools(Arc::new(|_workspace: &str| {
        vec![(
            Box::new(VisionCaptureTool) as Box<dyn tinyiothub_agent::port::tool::Tool>,
            tinyiothub_skills::trust::ToolSafety::ReadOnly,
        )]
    }));

    let ctx = Arc::new(RwLock::new(RunContextInner {
        run_id: "run-capability-eval".into(),
        workspace_id: workspace.clone(),
        task_id: Some("task-capability-eval".into()),
        ..Default::default()
    }));
    let error = match factory.get_or_create(&workspace, ctx).await {
        Ok(_) => panic!("video capability must fail closed on a non-vision provider"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("capability.unavailable"), "{error}");
    assert_eq!(*calls.lock().expect("calls"), 0, "model must not be called");

    let expected_event = scenario
        .expected
        .events
        .first()
        .expect("non-vision scenario must expect an event");
    let events = fixture
        .db
        .runtime_events_after(&workspace, None, Some("task-capability-eval"), 10)
        .await
        .expect("runtime events");
    assert!(
        events
            .iter()
            .any(|event| event.run_id.as_deref() == Some("run-capability-eval") && &event.event_type == expected_event),
        "missing {expected_event} event: {events:?}"
    );

    let effects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM effect_intents")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("effect count");
    assert_eq!(effects, 0, "capability failure must create zero Effects");
    fixture.close().await;
}

#[derive(Default)]
struct RecordingEffectSender {
    sends: Mutex<Vec<String>>,
}

#[async_trait]
impl EffectSender for RecordingEffectSender {
    async fn send(&self, intent: &EffectIntent) -> Result<(), String> {
        self.sends.lock().expect("sends").push(intent.effect_id.clone());
        Ok(())
    }
}

struct EvalReconciler {
    outcome: ReconcileOutcome,
    calls: Mutex<usize>,
}

impl EvalReconciler {
    fn new(outcome: ReconcileOutcome) -> Self {
        Self {
            outcome,
            calls: Mutex::new(0),
        }
    }

    fn calls(&self) -> usize {
        *self.calls.lock().expect("calls")
    }
}

#[async_trait]
impl EffectReconciler for EvalReconciler {
    async fn settle(&self, _intent: &EffectIntent) -> ReconcileOutcome {
        *self.calls.lock().expect("calls") += 1;
        self.outcome.clone()
    }
}

async fn seed_eval_effect(fixture: &WorkspaceFixture, effect_id: &str, state: EffectState) {
    let pool = fixture.workspace_pool().await;
    sqlx::query("INSERT INTO agent_tasks (id, run_id, state, state_version) VALUES (?, ?, 'running', 1)")
        .bind(effect_id)
        .bind(format!("run-{effect_id}"))
        .execute(&pool)
        .await
        .expect("task");
    sqlx::query("INSERT INTO agent_runs (id, task_id, state) VALUES (?, ?, 'running')")
        .bind(format!("run-{effect_id}"))
        .bind(effect_id)
        .execute(&pool)
        .await
        .expect("run");
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO effect_intents
            (effect_id, session_id, task_id, run_id, tool_call_id, tool_name, request_json,
             request_digest, idempotency_key, external_ref, state, state_version, result_json,
             created_at, updated_at)
         VALUES (?, 'session-eval', ?, ?, ?, 'set_speed', ?, ?, ?, 'driver-call-eval', ?, 1, NULL, ?, ?)",
    )
    .bind(effect_id)
    .bind(effect_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(json!({"device":"robot-1"}).to_string())
    .bind(format!("digest-{effect_id}"))
    .bind(effect_id)
    .bind(state.as_str())
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("effect");
    sqlx::query(
        "INSERT INTO device_jobs
            (id, workspace_id, device_id, tool, args_json, state, stop_requested_at, progress_json,
             result_json, task_id, run_id, tool_call_id, driver_call_id, timeout_ms, started_at, ended_at)
         VALUES (?, ?, 'robot-1', 'set_speed', '{}', 'running', NULL, NULL, NULL, ?, ?, ?,
                 'driver-call-eval', 5000, ?, NULL)",
    )
    .bind(format!("job-{effect_id}"))
    .bind(&fixture.workspace_id)
    .bind(effect_id)
    .bind(format!("run-{effect_id}"))
    .bind(format!("call-{effect_id}"))
    .bind(&now)
    .execute(&pool)
    .await
    .expect("job");
}

async fn seed_eval_decision(
    fixture: &WorkspaceFixture,
    effect_id: &str,
    kind: &str,
    state: tinyiothub_core::harness::DecisionState,
    answer: Option<&str>,
) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO decisions
            (decision_id, kind, state, session_id, run_id, task_id, effect_id, subject_json,
             options_json, created_at, updated_at, expires_at, answer, answer_actor,
             answer_event_id, consumed_at, consumed_by)
         VALUES (?, ?, ?, 'session-eval', ?, ?, ?, '{\"summary\":\"set speed\"}', ?, ?, ?, NULL, ?, NULL, NULL, NULL, NULL)",
    )
    .bind(format!("decision-{effect_id}"))
    .bind(kind)
    .bind(state.as_str())
    .bind(format!("run-{effect_id}"))
    .bind(effect_id)
    .bind(effect_id)
    .bind(json!(["applied", "not_applied"]).to_string())
    .bind(&now)
    .bind(&now)
    .bind(answer)
    .execute(&fixture.workspace_pool().await)
    .await
    .expect("decision");
}

async fn eval_event_count(fixture: &WorkspaceFixture, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM session_events WHERE type = ?")
        .bind(event_type)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("event count")
}

async fn eval_effect_state(fixture: &WorkspaceFixture, effect_id: &str) -> String {
    sqlx::query_scalar("SELECT state FROM effect_intents WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("effect state")
}

#[tokio::test]
async fn scripted_eval_denied_effect_never_dispatches_and_settles_once() {
    let scenario = harness_eval_scenario("denied-approval-no-duplicate");
    assert_eq!(scenario.kind, "denied_effect");
    let workspace = scenario_workspace(&scenario);
    let effect_id = "effect-denied";
    let fixture = fixture_workspace(&workspace).await;
    seed_eval_effect(&fixture, effect_id, EffectState::Prepared).await;
    seed_eval_decision(
        &fixture,
        effect_id,
        "tool_approval",
        tinyiothub_core::harness::DecisionState::Answered,
        Some("denied"),
    )
    .await;

    let db = Arc::new(fixture.db.clone());
    let intent = fixture
        .db
        .effect_intent(&workspace, effect_id)
        .await
        .expect("intent")
        .expect("effect exists");
    let sender = RecordingEffectSender::default();
    let dispatched = EffectDispatcher::new(db.clone())
        .dispatch(&workspace, &intent, &sender)
        .await
        .expect("dispatch gate");
    assert!(!dispatched, "a denied approval must not dispatch");
    assert!(
        sender.sends.lock().expect("sends").is_empty(),
        "forbidden side effect: dispatch"
    );

    let recovery = EffectRecovery::new(db);
    let reconciler = EvalReconciler::new(ReconcileOutcome::DefinitelyNotApplied);
    for _ in 0..2 {
        let report = recovery
            .recover_workspace(&workspace, RecoveryMode::Restart, &reconciler)
            .await
            .expect("denied recovery");
        assert!(report.dispatch_permits.is_empty(), "denied recovery must not dispatch");
    }

    assert_eq!(eval_effect_state(&fixture, effect_id).await, scenario.expected.status);
    for event in &scenario.expected.events {
        assert!(
            eval_event_count(&fixture, event).await == 1,
            "expected exactly one {event} event"
        );
    }
    assert_eq!(
        eval_event_count(&fixture, "step.tool_result").await,
        1,
        "forbidden side effect: duplicate tool result"
    );
    let effects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM effect_intents")
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("effect count");
    assert_eq!(effects, 1, "recovery must not duplicate the effect row");
    fixture.close().await;
}

#[tokio::test]
async fn scripted_eval_unknown_effect_never_redispatches_or_settles() {
    let scenario = harness_eval_scenario("unknown-effect-no-duplicate");
    assert_eq!(scenario.kind, "unknown_effect");
    let workspace = scenario_workspace(&scenario);
    let effect_id = "effect-unknown";
    let fixture = fixture_workspace(&workspace).await;
    // No reconciliation decision exists yet: fencing a dispatching effect must
    // create exactly one, and later recovery passes must reuse it.
    seed_eval_effect(&fixture, effect_id, EffectState::Dispatching).await;

    let db = Arc::new(fixture.db.clone());
    let recovery = EffectRecovery::new(db.clone());
    let reconciler = EvalReconciler::new(ReconcileOutcome::Unknown {
        reason: "device unreachable".into(),
    });
    for _ in 0..2 {
        let report = recovery
            .recover_workspace(&workspace, RecoveryMode::Restart, &reconciler)
            .await
            .expect("unknown recovery");
        assert!(report.dispatch_permits.is_empty(), "unknown recovery must not dispatch");
    }
    assert_eq!(eval_effect_state(&fixture, effect_id).await, scenario.expected.status);

    let intent = fixture
        .db
        .effect_intent(&workspace, effect_id)
        .await
        .expect("intent")
        .expect("effect exists");
    let sender = RecordingEffectSender::default();
    let dispatched = EffectDispatcher::new(db)
        .dispatch(&workspace, &intent, &sender)
        .await
        .expect("dispatch gate");
    assert!(!dispatched, "an unknown effect must not be re-dispatched");
    assert!(
        sender.sends.lock().expect("sends").is_empty(),
        "forbidden side effect: dispatch"
    );
    for event in &scenario.expected.events {
        assert!(
            eval_event_count(&fixture, event).await == 1,
            "expected exactly one {event} event"
        );
    }
    assert_eq!(
        eval_event_count(&fixture, "effect.settled").await,
        0,
        "forbidden side effect: settle without evidence"
    );
    assert_eq!(
        eval_event_count(&fixture, "step.tool_result").await,
        0,
        "forbidden side effect: tool result without evidence"
    );
    let decisions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM decisions WHERE effect_id = ?")
        .bind(effect_id)
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("decision count");
    assert_eq!(decisions, 1, "recovery must not duplicate the reconciliation decision");
    assert_eq!(reconciler.calls(), 2, "each recovery pass reconciles once");
    fixture.close().await;
}

#[tokio::test]
async fn scripted_eval_ack_is_not_physical_completion() {
    let scenario = harness_eval_scenario("ack-not-physical-completion");
    assert_eq!(scenario.kind, "ack");
    let workspace = scenario_workspace(&scenario);
    let effect_id = "effect-ack";
    let fixture = fixture_workspace(&workspace).await;
    seed_eval_effect(&fixture, effect_id, EffectState::Dispatching).await;

    let db = Arc::new(fixture.db.clone());
    let intent = fixture
        .db
        .effect_intent(&workspace, effect_id)
        .await
        .expect("intent")
        .expect("effect exists");
    let sender = RecordingEffectSender::default();
    let dispatched = EffectDispatcher::new(db)
        .dispatch(&workspace, &intent, &sender)
        .await
        .expect("dispatch gate");
    assert!(dispatched, "an active dispatching effect may be sent");
    assert_eq!(sender.sends.lock().expect("sends").len(), 1);

    // The sender accepted the command (ACK), but no physical result exists yet.
    assert_eq!(eval_effect_state(&fixture, effect_id).await, scenario.expected.status);
    let job_state: String = sqlx::query_scalar("SELECT state FROM device_jobs WHERE id = ?")
        .bind(format!("job-{effect_id}"))
        .fetch_one(&fixture.workspace_pool().await)
        .await
        .expect("job state");
    assert_eq!(job_state, "running", "forbidden side effect: job.done");
    for event in &scenario.expected.events {
        assert!(
            eval_event_count(&fixture, event).await >= 1,
            "missing expected event {event}"
        );
    }
    assert_eq!(
        eval_event_count(&fixture, "effect.settled").await,
        0,
        "forbidden side effect: ACK must not settle the effect"
    );

    // Only a device result completes the effect; an ACK without a result is
    // rejected by the production reducer.
    let state = HarnessState {
        workspace_id: workspace.clone(),
        effect_id: effect_id.into(),
        session_id: "session-eval".into(),
        task_id: effect_id.into(),
        run_id: format!("run-{effect_id}"),
        execution_mode: ExecutionMode::Production,
        status: EffectState::Dispatching,
        state_version: 1,
    };
    let ack_only = HarnessFact {
        transition_id: "ack-only".into(),
        effect_id: effect_id.into(),
        next: EffectState::Completed,
        occurred_at: Utc::now(),
        external_ref: Some("driver-call-eval".into()),
        result: None,
        job: None,
        wake: None,
    };
    assert!(matches!(reduce(&state, &ack_only), Err(HarnessError::MissingResult)));
    let with_result = HarnessFact {
        result: Some(json!({"position": 2})),
        ..ack_only
    };
    let transition = reduce(&state, &with_result).expect("physical result completes");
    assert_eq!(
        transition.effect.expect("effect mutation").state,
        EffectState::Completed
    );
    fixture.close().await;
}

struct EvalOutcomeExecutor {
    outcome: TaskOutcome,
}

#[async_trait]
impl SubagentExecutor for EvalOutcomeExecutor {
    async fn execute(&self, _input: tinyiothub_agent::harness::SubagentInput) -> anyhow::Result<TaskOutcome> {
        Ok(self.outcome.clone())
    }
}

#[tokio::test]
async fn scripted_eval_task_outcome_schema_and_evidence_refs() {
    use tinyiothub_cloud::domains::agent::workflows::SubagentRunner;

    let scenario = harness_eval_scenario("task-outcome-schema");
    assert_eq!(scenario.kind, "task_outcome");
    let outcome = TaskOutcome {
        status: scenario.expected.status.clone(),
        summary: scenario.facts.clone(),
        observations: vec!["temperature 20 from snapshot".into()],
        evidence_refs: scenario.expected.evidence_refs.clone(),
        recommended_actions: vec![],
    };
    let runner = SubagentRunner::new(Arc::new(EvalOutcomeExecutor {
        outcome: outcome.clone(),
    }));
    let returned = runner
        .run(tinyiothub_agent::harness::SubagentInput {
            session_id: "caller-must-not-win".into(),
            task_id: "task-outcome-eval".into(),
            run_id: "run-outcome-eval".into(),
            workflow: WorkflowRevision {
                workflow_id: "workflow-eval".into(),
                version: 1,
                enabled: true,
                definition: WorkflowDefinition {
                    objective: scenario.facts.clone(),
                    trigger: WorkflowTrigger::Event {
                        trigger_id: "eval".into(),
                    },
                    device_selector: json!({}),
                    source_selector: json!({}),
                    tool_ids: vec![],
                    skill_refs: vec![],
                    required_capabilities: vec![],
                    approval_policy: json!({}),
                    evidence_policy: json!({}),
                },
                created_at: Utc::now(),
            },
            facts: DeviceFactsSnapshot {
                observed_at: Utc::now(),
                declaration_hash: "declaration-eval".into(),
                driver_version: "driver-eval".into(),
                facts: json!({"temperature": 20}),
            },
            source: json!({"snapshotSeq": 7}),
            transcript: vec![ChatMessage::user("stale transcript must be cleared")],
        })
        .await
        .expect("workflow runner executes");
    assert_eq!(returned, outcome);

    let value = serde_json::to_value(&returned).expect("TaskOutcome JSON");
    let keys: std::collections::BTreeSet<&str> =
        value.as_object().expect("object").keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from([
            "evidenceRefs",
            "observations",
            "recommendedActions",
            "status",
            "summary",
        ]),
        "TaskOutcome schema changed"
    );
    assert_eq!(
        value["evidenceRefs"],
        json!(scenario.expected.evidence_refs),
        "evidence refs must be preserved"
    );
    let round_trip: TaskOutcome = serde_json::from_value(value).expect("TaskOutcome round-trip");
    assert_eq!(round_trip, returned);
}
