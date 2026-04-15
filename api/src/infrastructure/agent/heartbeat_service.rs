// Heartbeat Service - IoT periodic inspection tasks wrapper around zeroclaw HeartbeatEngine
//
// The zeroclaw HeartbeatEngine collects tasks from HEARTBEAT.md but does not execute them.
// This service adds execution logic: high-priority tasks are sent to the ChatService
// as agent prompts, with results logged and (optionally) stored in memory.

use std::sync::Arc;

use futures::StreamExt;

use zeroclaw::heartbeat::engine::{HeartbeatEngine, TaskPriority};
use crate::api::mcp::handlers::{McpAuthContext, McpContextGuard};

/// Execution record for a single heartbeat run
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatExecutionRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub task_count: usize,
    pub status: String,
    pub error_message: Option<String>,
}

/// Global heartbeat state for API access
#[derive(Debug, Clone)]
pub struct HeartbeatState {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub workspace_id: String,
    pub agent_id: String,
    pub execution_history: Vec<HeartbeatExecutionRecord>,
    pub workspace_dir: std::path::PathBuf,
}

impl HeartbeatState {
    pub fn new(workspace_id: String, agent_id: String, interval_minutes: u32, workspace_dir: std::path::PathBuf) -> Self {
        Self {
            enabled: true,
            interval_minutes,
            workspace_id,
            agent_id,
            execution_history: Vec::new(),
            workspace_dir,
        }
    }

    pub fn add_execution_record(&mut self, record: HeartbeatExecutionRecord) {
        self.execution_history.insert(0, record);
        // Keep only last 100 records
        if self.execution_history.len() > 100 {
            self.execution_history.truncate(100);
        }
    }
}

/// A single heartbeat task
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTask {
    pub priority: String,
    pub text: String,
    pub paused: bool,
}

/// Read tasks from HEARTBEAT.md
pub async fn read_heartbeat_tasks(
    workspace_dir: &std::path::Path,
) -> anyhow::Result<Vec<HeartbeatTask>> {
    let path = workspace_dir.join("HEARTBEAT.md");
    if !path.exists() {
        return Ok(get_default_tasks());
    }
    let content = tokio::fs::read_to_string(&path).await?;
    Ok(parse_heartbeat_md(&content))
}

/// Write tasks to HEARTBEAT.md
pub async fn write_heartbeat_tasks(
    workspace_dir: &std::path::Path,
    tasks: &[HeartbeatTask],
) -> anyhow::Result<()> {
    let content = build_heartbeat_md(tasks);
    tokio::fs::write(workspace_dir.join("HEARTBEAT.md"), content).await?;
    Ok(())
}

fn get_default_tasks() -> Vec<HeartbeatTask> {
    vec![
        HeartbeatTask { priority: "high".into(), text: "检查离线设备并尝试自动重连".into(), paused: false },
        HeartbeatTask { priority: "medium".into(), text: "扫描未处理的高优先级告警".into(), paused: false },
        HeartbeatTask { priority: "medium".into(), text: "生成设备状态日报摘要".into(), paused: false },
        HeartbeatTask { priority: "low".into(), text: "检查系统磁盘和内存使用率".into(), paused: true },
    ]
}

fn parse_heartbeat_md(content: &str) -> Vec<HeartbeatTask> {
    let mut tasks = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if !line.starts_with('-') || !line.starts_with("- [") {
            continue;
        }
        // Parse "- [priority|paused] text" or "- [priority] text"
        if let Some(rest) = line.strip_prefix("- [") {
            let rest = rest.trim_end_matches(']').trim();
            let (priority_part, text) = rest.split_once(']').unwrap_or((rest, ""));
            let text = text.trim();
            let (priority, paused) = if priority_part.contains("|paused") {
                let p = priority_part.split('|').next().unwrap_or("low");
                (p.to_string(), true)
            } else {
                (priority_part.to_string(), false)
            };
            if !text.is_empty() {
                tasks.push(HeartbeatTask { priority, text: text.to_string(), paused });
            }
        } else if let Some(text) = line.strip_prefix("- ") {
            let text = text.trim();
            if !text.is_empty() {
                tasks.push(HeartbeatTask { priority: "low".into(), text: text.to_string(), paused: false });
            }
        }
    }
    tasks
}

fn build_heartbeat_md(tasks: &[HeartbeatTask]) -> String {
    let mut s = "# Periodic Tasks\n".to_string();
    for task in tasks {
        let flag = if task.paused { format!("{}\\paused", task.priority) } else { task.priority.clone() };
        s.push_str(&format!("- [{}] {}\\n", flag, task.text));
    }
    s
}

// Global heartbeat state - initialized when HeartbeatService is created
static HEARTBEAT_STATE: std::sync::OnceLock<Arc<tokio::sync::RwLock<HeartbeatState>>> = std::sync::OnceLock::new();

/// Initialize the global heartbeat state
fn init_heartbeat_state(state: HeartbeatState) {
    let _ = HEARTBEAT_STATE.set(Arc::new(tokio::sync::RwLock::new(state)));
}

/// Get a reference to the global heartbeat state
pub fn get_heartbeat_state() -> Option<&'static Arc<tokio::sync::RwLock<HeartbeatState>>> {
    HEARTBEAT_STATE.get()
}

/// IoT Heartbeat Service
pub struct HeartbeatService {
    engine: HeartbeatEngine,
    chat_service: Arc<crate::application::agent::ChatService>,
    workspace_dir: std::path::PathBuf,
    workspace_id: String,
    agent_id: String,
    interval_minutes: u32,
    /// System prompt injected before each heartbeat task
    heartbeat_prompt: String,
}

impl HeartbeatService {
    /// Create a new heartbeat service
    pub fn new(
        workspace_dir: std::path::PathBuf,
        config: zeroclaw::config::schema::HeartbeatConfig,
        observer: Arc<dyn zeroclaw::observability::Observer>,
        chat_service: Arc<crate::application::agent::ChatService>,
        workspace_id: String,
        agent_id: String,
        heartbeat_prompt: String,
    ) -> Self {
        let interval_minutes = config.interval_minutes;

        // Initialize global state for API access
        let state = HeartbeatState::new(
            workspace_id.clone(),
            agent_id.clone(),
            interval_minutes,
            workspace_dir.clone(),
        );
        init_heartbeat_state(state);

        let engine = HeartbeatEngine::new(config, workspace_dir.clone(), observer);
        Self {
            engine,
            chat_service,
            workspace_dir,
            workspace_id,
            agent_id,
            interval_minutes,
            heartbeat_prompt,
        }
    }

    /// Ensure HEARTBEAT.md exists with default IoT tasks
    pub async fn ensure_heartbeat_file(&self) -> anyhow::Result<()> {
        let path = self.workspace_dir.join("HEARTBEAT.md");
        if !path.exists() {
            let content = "# Periodic Tasks\n\
                - [high] 检查离线设备并尝试自动重连\n\
                - [medium] 扫描未处理的高优先级告警\n\
                - [medium] 生成设备状态日报摘要\n\
                - [low|paused] 检查系统磁盘和内存使用率\n";
            tokio::fs::create_dir_all(&self.workspace_dir).await.ok();
            tokio::fs::write(&path, content).await?;
            tracing::info!("Created default HEARTBEAT.md at {:?}", path);
        }
        Ok(())
    }

    /// Run the heartbeat loop
    pub async fn run(&self) {
        if let Err(e) = self.ensure_heartbeat_file().await {
            tracing::warn!("Failed to ensure heartbeat file: {}", e);
        }

        let interval_mins = self.interval_minutes as u64;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_mins * 60));

        tracing::info!(
            "💓 HeartbeatService started for workspace={} agent={} (interval={}min)",
            self.workspace_id,
            self.agent_id,
            self.interval_minutes
        );

        loop {
            interval.tick().await;

            let timestamp = chrono::Utc::now();
            let mut task_count = 0usize;
            let mut status = "success".to_string();
            let mut error_message = None;

            match self.engine.collect_runnable_tasks().await {
                Ok(tasks) => {
                    task_count = tasks.len();
                    if !tasks.is_empty() {
                        tracing::info!("💓 Heartbeat collected {} tasks", tasks.len());
                    }
                    for task in tasks {
                        if task.priority == TaskPriority::High {
                            if let Err(e) = self.execute_task(&task.text).await {
                                status = "error".to_string();
                                error_message = Some(e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    status = "error".to_string();
                    error_message = Some(e.to_string());
                    tracing::warn!("💓 Heartbeat task collection failed: {}", e);
                }
            }

            // Record execution history
            if let Some(state) = get_heartbeat_state() {
                let record = HeartbeatExecutionRecord {
                    timestamp,
                    task_count,
                    status: status.clone(),
                    error_message,
                };
                let mut state_guard = state.write().await;
                state_guard.add_execution_record(record);
            }

            tracing::debug!("💓 Heartbeat tick completed: {} tasks, status={}", task_count, status);
        }
    }

    /// Execute a single heartbeat task via ChatService
    async fn execute_task(&self, task_text: &str) -> Result<(), String> {
        let session_key = format!("agent:{}:{}/heartbeat", self.workspace_id, self.agent_id);
        let run_id = format!("heartbeat-{}", chrono::Utc::now().timestamp_millis());
        let prompt = format!(
            "{}\n\n## 本次巡检任务\n\n{}\n\n请执行后给出简洁的结构化结果。",
            self.heartbeat_prompt,
            task_text
        );

        // Set MCP context for in-process tool calls (heartbeat system task)
        let mcp_ctx = McpAuthContext::for_heartbeat(
            self.workspace_id.clone(),
            self.agent_id.clone(),
        );
        let _guard = McpContextGuard::new(mcp_ctx);

        let request = crate::application::agent::ChatRequest {
            session_key,
            message: prompt.clone(),
            run_id,
            system_prompt_override: Some(prompt),
        };

        match self.chat_service.chat(request).await {
            Ok(stream) => {
                let mut stream = stream;
                while let Some(event) = stream.next().await {
                    match event {
                        crate::application::agent::ChatEvent::Final { message, .. } => {
                            tracing::info!(
                                "💓 Heartbeat task result: {}",
                                serde_json::to_string(&message).unwrap_or_default()
                            );
                        }
                        crate::application::agent::ChatEvent::Error { error, .. } => {
                            tracing::warn!("💓 Heartbeat task error: {}", error);
                            return Err(error);
                        }
                        _ => {}
                    }
                }
                tracing::info!("💓 Heartbeat task executed: {}", task_text);
                Ok(())
            }
            Err(e) => {
                let msg = format!("Chat service error: {}", e);
                tracing::warn!("💓 Heartbeat task execution failed: {}", msg);
                Err(msg)
            }
        }
    }
}
