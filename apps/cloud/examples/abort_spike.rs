//! Abort go/no-go spike for the Thing Agent Loop (Task 1 / T0).
//!
//! Question: does `Agent::turn_streamed`'s third parameter
//! (`Option<CancellationToken>`) actually stop the tool loop when cancelled
//! mid-tool-execution?
//!
//! Setup: a scripted mock provider that always tells the agent to call
//! `slow_tool`, and a `slow_tool` that sleeps 30s. We cancel the token as
//! soon as the first `TurnEvent::ToolCall` arrives.
//!
//! GO criteria:
//!   1. `turn_streamed` returns `Err`
//!   2. the error chain contains `ToolLoopCancelled`
//!   3. total elapsed << 30s (the in-flight tool future is dropped, not awaited)
//!
//! Run: `cargo run -p tinyiothub-cloud --example abort_spike`

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zeroclaw::{
    agent::{Agent, TurnEvent, dispatcher::NativeToolDispatcher, loop_::is_tool_loop_cancelled},
    observability::{NoopObserver, Observer},
    providers::{ChatRequest, ChatResponse, ModelProvider, ToolCall},
    tools::{Tool, ToolResult},
};

const SLOW_TOOL_SECS: u64 = 30;
const GO_BUDGET: Duration = Duration::from_secs(5);

/// Mock provider: first `chat` call returns a native tool call to
/// `slow_tool`; anything after that returns plain text "done".
struct ScriptedProvider {
    responses: Mutex<Vec<ChatResponse>>,
}

#[async_trait]
impl ModelProvider for ScriptedProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<String> {
        Ok("fallback".into())
    }

    async fn chat(&self, _request: ChatRequest<'_>, _model: &str, _temperature: Option<f64>) -> Result<ChatResponse> {
        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            return Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(guard.remove(0))
    }
}

impl zeroclaw_api::attribution::Attributable for ScriptedProvider {
    fn role(&self) -> zeroclaw_api::attribution::Role {
        zeroclaw_api::attribution::Role::Provider(zeroclaw_api::attribution::ProviderKind::Model(
            zeroclaw_api::attribution::ModelProviderKind::Custom,
        ))
    }
    fn alias(&self) -> &str {
        "ScriptedProvider"
    }
}

/// Tool that sleeps 30s — stands in for a slow IoT operation.
struct SlowTool;

zeroclaw_api::mock_tool_attribution!(SlowTool);

#[async_trait]
impl Tool for SlowTool {
    fn name(&self) -> &str {
        "slow_tool"
    }

    fn description(&self) -> &str {
        "Sleeps 30s then replies"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
        tokio::time::sleep(Duration::from_secs(SLOW_TOOL_SECS)).await;
        Ok(ToolResult {
            success: true,
            output: "slow tool finished".into(),
            error: None,
        })
    }
}

fn build_agent() -> Agent {
    let provider = Box::new(ScriptedProvider {
        responses: Mutex::new(vec![ChatResponse {
            text: Some(String::new()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "slow_tool".into(),
                arguments: "{}".into(),
                extra_content: None,
            }],
            usage: None,
            reasoning_content: None,
        }]),
    });

    let memory_cfg = zeroclaw::config::schema::MemoryConfig {
        backend: "none".into(),
        ..Default::default()
    };
    let memory = zeroclaw::memory::create_memory(&memory_cfg, &std::env::temp_dir(), None).expect("create noop memory");
    let memory: Arc<dyn zeroclaw::memory::Memory> = Arc::from(memory);
    let observer: Arc<dyn Observer> = Arc::from(NoopObserver {});

    Agent::builder()
        .model_provider(provider)
        .tools(vec![Box::new(SlowTool) as Box<dyn Tool>])
        .memory(memory)
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .expect("build agent")
}

#[tokio::main]
async fn main() {
    let mut agent = build_agent();
    let token = CancellationToken::new();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);

    // Cancel as soon as the first ToolCall event (slow_tool dispatch) arrives.
    let cancel_on_tool_call = token.clone();
    let watcher = tokio::spawn(async move {
        while let Some(ev) = event_rx.recv().await {
            if let TurnEvent::ToolCall { ref name, .. } = ev {
                println!("[spike] ToolCall event: {name} -> cancelling token");
                cancel_on_tool_call.cancel();
                // keep draining so the sender never blocks
            }
        }
    });

    let start = Instant::now();
    let result = agent.turn_streamed("call the slow tool", event_tx, Some(token)).await;
    let elapsed = start.elapsed();
    watcher.abort();

    println!("[spike] elapsed: {elapsed:?}");

    let mut failures: Vec<String> = Vec::new();
    match &result {
        Ok(_) => failures.push("turn_streamed returned Ok — cancellation did not abort the loop".into()),
        Err(e) => {
            println!("[spike] error: {e:#}");
            if !is_tool_loop_cancelled(e) {
                failures.push(format!("error is not ToolLoopCancelled: {e:#}"));
            }
        }
    }
    if elapsed >= GO_BUDGET {
        failures.push(format!(
            "elapsed {elapsed:?} >= {GO_BUDGET:?} — in-flight tool was NOT dropped"
        ));
    }

    if failures.is_empty() {
        println!(
            "[spike] PASS: cancel aborted the tool loop in {elapsed:?} (tool alone sleeps {SLOW_TOOL_SECS}s) => GO"
        );
    } else {
        for f in &failures {
            eprintln!("[spike] FAIL: {f}");
        }
        eprintln!("[spike] => NO-GO");
        std::process::exit(1);
    }
}
