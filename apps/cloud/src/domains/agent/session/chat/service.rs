// Stateless ChatService — agent loop handle passed as parameter.
// Eliminates the SSE serialization round-trip:
//   Before: TurnEvent → bytes → reqwest::Response → parse bytes → ChatEvent → SSE
//   After:  TurnEvent → ChatEvent → SSE

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::domains::agent::pool::AgentPool;
use crate::domains::agent::session::chat_types::{ChatError, ChatEvent};
use tinyiothub_agent::AgentError;
use tinyiothub_agent::config::AgentRuntimeConfig;
use tinyiothub_agent::port::events::TurnEvent;
use tinyiothub_agent::port::runtime::AgentLoopHandle;

// ============================================================================
// Pool-facing entry points (db-backed; cloud side — Task 7 fix round 1)
// ============================================================================

/// Ensure a pooled agent exists: cache hit returns immediately; on a miss the
/// tools are resolved via the pool's ToolRegistry (composition-layer
/// providers, Task 14) and injected into the pool's pure `create`.
///
/// `session_tools_enabled` exposes the session primitives (`todo_write`,
/// `ask_user`, `spawn_subagent`) — only chat/subagent turns have a
/// `TurnContext`, so heartbeat/autonomous callers pass `false`.
///
/// **Cache isolation constraint:** `AgentPool` caches by `agent_id` and the
/// tool set is fixed on first creation. Chat and heartbeat/autonomous agents
/// must therefore use distinct ids, or the first creator's tool set leaks to
/// the other. Heartbeat uses `heartbeat_agent_id(workspace_id)`
/// (`__heartbeat__:<ws>`); `get_or_create_agent` still uses `workspace_id` as
/// the id and has no callers today — any future autonomous wiring must give it
/// its own prefix.
pub async fn ensure_agent_legacy(
    pool: &AgentPool,
    agent_id: &str,
    workspace_id: &str,
    config: &AgentRuntimeConfig,
    session_tools_enabled: bool,
) -> Result<AgentLoopHandle, AgentError> {
    ensure_agent_inner(pool, agent_id, workspace_id, config, session_tools_enabled, None).await
}

pub async fn ensure_agent(
    pool: &AgentPool,
    agent_id: &str,
    workspace_id: &str,
    config: &AgentRuntimeConfig,
    session_tools_enabled: bool,
    prompt: &crate::domains::agent::prompt::assistant::PromptPackage,
) -> Result<AgentLoopHandle, AgentError> {
    ensure_agent_inner(
        pool,
        agent_id,
        workspace_id,
        config,
        session_tools_enabled,
        Some(prompt),
    )
    .await
}

async fn ensure_agent_inner(
    pool: &AgentPool,
    agent_id: &str,
    workspace_id: &str,
    config: &AgentRuntimeConfig,
    session_tools_enabled: bool,
    prompt: Option<&crate::domains::agent::prompt::assistant::PromptPackage>,
) -> Result<AgentLoopHandle, AgentError> {
    let prompt_hash = prompt.map(|prompt| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&prompt.system_prompt, &mut hasher);
        std::hash::Hasher::finish(&hasher)
    });
    if let Some(agent) = pool.get_cached(agent_id) {
        // The tool set and static prompt are fixed at creation. Either change
        // requires a fresh loop, otherwise stale rules remain in the preamble.
        let tools_match = pool.agent_session_tools(agent_id) == Some(session_tools_enabled);
        let prompt_matches = prompt_hash.is_none() || pool.agent_prompt_hash(agent_id) == prompt_hash;
        if tools_match && prompt_matches {
            return Ok(agent);
        }
        pool.invalidate(agent_id);
    }
    let trust_config = pool.trust_config(workspace_id);
    let mut runtime = pool.runtime_context().await;
    runtime.agent_id = Some(agent_id.to_string());
    runtime.session_tools_enabled = session_tools_enabled;
    let tools = pool
        .tool_registry()
        .resolve_tools_for_agent(config, workspace_id, trust_config, &runtime)
        .await;
    match prompt {
        Some(prompt) => pool.create_with_prompt(agent_id, workspace_id, config, tools, session_tools_enabled, prompt),
        None => pool.create(agent_id, workspace_id, config, tools, session_tools_enabled),
    }
}

/// Chat send formerly on `impl AgentPool` (`host/agent/chat.rs`). The session
/// key check runs before any agent build; config/memory_service/db are
/// injected by the caller (proxy handler).
#[allow(clippy::too_many_arguments)]
pub async fn send_with_pool(
    pool: &AgentPool,
    db_pool: &sqlx::SqlitePool,
    agent_id: &str,
    session_key: &str,
    message: &str,
    run_id: &str,
    system_prompt: &str,
    authorized_workspace: &str,
    config: &AgentRuntimeConfig,
    memory_service: Option<Arc<tinyiothub_memory::service::MemoryService>>,
    session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
) -> Result<mpsc::Receiver<ChatEvent>, AgentError> {
    let parsed = tinyiothub_agent::session::SessionKey::parse(session_key)?;
    // Empty authorized workspace = unscoped (admin) token; nothing to check against.
    if !authorized_workspace.is_empty() {
        parsed.verify_workspace(authorized_workspace)?;
    }
    let agent = ensure_agent_legacy(pool, agent_id, &parsed.workspace_id, config, true).await?;
    let event_publisher = pool.event_publisher.read().await.clone();
    send_message(
        &agent,
        message,
        run_id,
        session_key,
        system_prompt,
        &pool.chat_handles,
        memory_service,
        event_publisher,
        config.enable_reflection,
        &config.model,
        &parsed.workspace_id,
        agent_id,
        db_pool,
        session_runtime,
    )
    .await
    .map_err(|e| AgentError::RequestFailed(e.to_string()))
}

/// Convert zeroclaw TurnEvent → ChatEvent (no intermediate JSON serialization)
fn turn_event_to_chat_event(evt: &TurnEvent, run_id: &str, session_key: &str) -> ChatEvent {
    match evt {
        TurnEvent::Chunk { delta } => ChatEvent::Delta {
            run_id: run_id.to_string(),
            session_key: session_key.to_string(),
            message: serde_json::json!({
                "role": "assistant",
                "content": [{ "type": "text", "text": delta }],
            }),
        },
        TurnEvent::Thinking { delta } => ChatEvent::Thinking {
            run_id: run_id.to_string(),
            session_key: session_key.to_string(),
            thinking: delta.clone(),
        },
        TurnEvent::ToolCall { name, args, .. } => {
            let args_str = serde_json::to_string(args).unwrap_or_default();
            let a2ui_jsonl = if name == "canvas" {
                let raw = args.get("jsonl").and_then(|v| v.as_str()).unwrap_or("").to_string();
                // Validate each JSONL line — LLM-generated JSON may have
                // mismatched braces. Log warnings for debugging but pass
                // through; the frontend has its own error recovery.
                for line in raw.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if serde_json::from_str::<serde_json::Value>(trimmed).is_err() {
                        tracing::warn!(
                            line = %trimmed.chars().take(80).collect::<String>(),
                            "a2ui: invalid JSONL line from LLM"
                        );
                    }
                }
                raw
            } else {
                String::new()
            };
            ChatEvent::ToolCallStart {
                run_id: run_id.to_string(),
                session_key: session_key.to_string(),
                tool_name: name.clone(),
                tool_args: args_str,
                a2ui: if a2ui_jsonl.is_empty() { None } else { Some(a2ui_jsonl) },
            }
        }
        TurnEvent::ToolResult { name, output, .. } => ChatEvent::ToolResult {
            run_id: run_id.to_string(),
            session_key: session_key.to_string(),
            tool_name: name.clone(),
            result: output.clone(),
        },
        TurnEvent::ApprovalRequest {
            tool_name,
            arguments_summary,
            ..
        } => ChatEvent::ToolCallStart {
            run_id: run_id.to_string(),
            session_key: session_key.to_string(),
            tool_name: tool_name.clone(),
            tool_args: arguments_summary.clone(),
            a2ui: None,
        },
        TurnEvent::Usage { .. } => {
            // Usage events are informational only; filter these out downstream
            ChatEvent::Delta {
                run_id: run_id.to_string(),
                session_key: session_key.to_string(),
                message: serde_json::json!({"__usage": true}),
            }
        }
    }
}

/// Adapts the runner's per-Turn events back into chat SSE events.
struct ChatEventObserver {
    tx: mpsc::Sender<ChatEvent>,
    run_id: String,
    session_key: String,
}

#[async_trait::async_trait]
impl crate::domains::agent::session::TurnObserver for ChatEventObserver {
    async fn on_turn_event(&self, event: &TurnEvent) {
        if matches!(event, TurnEvent::Usage { .. }) {
            return;
        }
        let chat = turn_event_to_chat_event(event, &self.run_id, &self.session_key);
        let _ = self.tx.send(chat).await;
    }
}

/// Send a chat message to a pooled agent loop and receive ChatEvents directly.
///
/// Returns an mpsc::Receiver<ChatEvent> — no bytes round-trip.
#[allow(clippy::too_many_arguments)]
pub async fn send_message(
    agent: &AgentLoopHandle,
    message: &str,
    run_id: &str,
    session_key: &str,
    system_prompt: &str,
    chat_handles: &Arc<tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>>,
    memory_service: Option<std::sync::Arc<tinyiothub_memory::service::MemoryService>>,
    event_publisher: Option<std::sync::Arc<crate::domains::agent::event::bus::AiEventPublisher>>,
    enable_reflection: bool,
    model: &str,
    workspace_id: &str,
    agent_id: &str,
    db_pool: &sqlx::SqlitePool,
    session_runtime: Arc<crate::domains::agent::session::SessionRuntime>,
) -> Result<mpsc::Receiver<ChatEvent>, ChatError> {
    let agent = Arc::clone(agent);
    // Resolve a leading `/trigger` skill token: prepend the full skill body to
    // this turn (so the model reliably has it) and keep a clean message for
    // display/reflection. A bare trigger (e.g. auto-`/workspace`) falls back to
    // a neutral default request.
    let (turn_message, reflect_message, trigger_skill) = {
        let ws_dir = crate::domains::agent::prompt::paths::workspace_dir(workspace_id);
        let skills = crate::domains::agent::pool::load_workspace_skills(&ws_dir);
        match tinyiothub_skills::resolve_trigger(message, &skills) {
            Some(hit) => {
                let user_text = if hit.cleaned_message.is_empty() {
                    "综合情况".to_string()
                } else {
                    hit.cleaned_message
                };
                let version = skills
                    .iter()
                    .find(|skill| skill.name == hit.name)
                    .map(|skill| skill.version.clone())
                    .unwrap_or_default();
                let turn = tinyiothub_skills::wrap_injected_skill(&hit.name, &hit.body, &user_text);
                (turn, user_text, Some((hit.name, version)))
            }
            None => (message.to_string(), message.to_string(), None),
        }
    };
    let run_id = run_id.to_string();
    let session_key = session_key.to_string();
    let system_prompt = system_prompt.to_string();
    let chat_handles = Arc::clone(chat_handles);
    let chat_handles_inner = Arc::clone(&chat_handles);
    let workspace_id = workspace_id.to_string();
    let agent_id = agent_id.to_string();
    let reflection_model = model.to_string();
    let event_publisher = event_publisher.clone();
    let db_pool = db_pool.clone();
    let session_runtime = session_runtime.clone();

    let (tx, rx) = mpsc::channel::<ChatEvent>(100);

    let run_id_for_handle = run_id.clone();
    let run_id_for_remove = run_id.clone();
    let handle = tokio::spawn(async move {
        // Probe before ensure_session inserts the row, so we know whether this
        // turn opened the session.
        let is_new_session =
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM chat_sessions WHERE session_key = ?")
                .bind(&session_key)
                .fetch_one(&db_pool)
                .await
            {
                Ok(count) => count == 0,
                Err(error) => {
                    tracing::warn!(%error, %session_key, "Failed to probe chat session");
                    false
                }
            };

        // One lock across reseed + turn: the agent's in-memory history is
        // shared across sessions, so each turn rebuilds context from THIS
        // session's persisted messages. Splitting this into two lock
        // acquisitions would let a concurrent turn on another session swap
        // the history in between.
        let ag = agent.lock().await;

        if let Err(e) = super::history::ensure_session(&db_pool, &session_key, &workspace_id, &agent_id).await {
            tracing::warn!(error = %e, %session_key, "Failed to ensure chat session row");
        }
        let prior =
            super::history::list_messages(&db_pool, &session_key, super::history::SESSION_CONTEXT_MESSAGE_LIMIT)
                .await
                .unwrap_or_default();
        ag.clear_history().await;
        if !system_prompt.is_empty() || !prior.is_empty() {
            let mut seed = Vec::with_capacity(prior.len() + 1);
            if !system_prompt.is_empty() {
                seed.push(tinyiothub_agent::port::provider::ChatMessage::system(
                    system_prompt.clone(),
                ));
            }
            seed.extend(
                prior
                    .into_iter()
                    .map(|(role, content)| tinyiothub_agent::port::provider::ChatMessage {
                        role,
                        content,
                        attachments: vec![],
                    }),
            );
            if let Err(error) = ag.seed_history(&seed).await {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: error.to_string(),
                    })
                    .await;
                chat_handles_inner.lock().await.remove(&run_id_for_remove);
                return;
            }
        }
        if let Err(e) = super::history::append_message(&db_pool, &session_key, "user", &reflect_message, &run_id).await
        {
            tracing::warn!(error = %e, %session_key, "Failed to persist user message");
        }

        let permission_mode = tinyiothub_storage::Db::new(db_pool.clone())
            .get_session(&session_key)
            .await
            .ok()
            .flatten()
            .and_then(|session| crate::domains::agent::session::types::PermissionMode::parse(&session.permission_mode))
            .unwrap_or_default();
        let context = crate::domains::agent::session::TurnContext::new(
            workspace_id.clone(),
            agent_id.clone(),
            session_key.clone(),
            run_id.clone(),
            0,
        )
        .with_permission_mode(permission_mode);
        if is_new_session
            && let Err(error) = session_runtime
                .events
                .emit(crate::domains::agent::session::session_event(
                    &context,
                    "session.started",
                    crate::domains::agent::session::session_started_payload(&session_key, "chat"),
                ))
                .await
        {
            tracing::warn!(%error, %session_key, "Failed to persist session.started event");
        }
        if let Err(error) = session_runtime
            .events
            .emit(crate::domains::agent::session::session_event(
                &context,
                "message.created",
                serde_json::json!({
                    "messageId": uuid::Uuid::new_v4().to_string(),
                    "role": "user",
                    "text": reflect_message.as_str()
                }),
            ))
            .await
        {
            tracing::warn!(%error, %session_key, "Failed to persist user session event");
        }
        if let Some((skill_name, skill_version)) = trigger_skill
            && let Err(error) = session_runtime
                .events
                .emit(crate::domains::agent::session::session_event(
                    &context,
                    "skill.loaded",
                    serde_json::json!({
                        "skillId": skill_name,
                        "name": skill_name,
                        "version": skill_version
                    }),
                ))
                .await
        {
            tracing::warn!(%error, %session_key, "Failed to persist skill.loaded event");
        }

        let observer: Arc<dyn crate::domains::agent::session::TurnObserver> = Arc::new(ChatEventObserver {
            tx: tx.clone(),
            run_id: run_id.clone(),
            session_key: session_key.clone(),
        });
        let outcome = session_runtime
            .runner
            .run_turn_bounded(
                &*ag,
                &context,
                &turn_message,
                Some(observer),
                crate::domains::agent::session::SESSION_TURN_TIMEOUT,
            )
            .await;
        drop(ag);

        let final_text = match outcome {
            crate::domains::agent::session::TurnOutcome::Completed { text, .. } => {
                let _ = tx
                    .send(ChatEvent::Final {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        message: serde_json::json!({
                            "role": "assistant",
                            "content": [{ "type": "text", "text": &text }],
                        }),
                    })
                    .await;
                Some(text)
            }
            crate::domains::agent::session::TurnOutcome::Cancelled => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: "Agent execution cancelled".to_string(),
                    })
                    .await;
                None
            }
            crate::domains::agent::session::TurnOutcome::Failed { error } => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error,
                    })
                    .await;
                None
            }
        };

        if let Some(ref assistant_text) = final_text
            && let Err(e) =
                super::history::append_message(&db_pool, &session_key, "assistant", assistant_text, &run_id).await
        {
            tracing::warn!(error = %e, %session_key, "Failed to persist assistant message");
        }

        // Spawn reflection after the turn completes (fire-and-forget)
        if enable_reflection
            && let Some(assistant_text) = final_text
            && let Some(ms) = memory_service
        {
            let turn_messages = vec![
                tinyiothub_llm::session::types::ChatTurnMessage {
                    role: "user".into(),
                    content: reflect_message.clone(),
                    ..Default::default()
                },
                tinyiothub_llm::session::types::ChatTurnMessage {
                    role: "assistant".into(),
                    content: assistant_text,
                    ..Default::default()
                },
            ];
            let ep = event_publisher.clone();
            let ws_id = workspace_id.clone();
            let aid = agent_id.clone();
            let sk = session_key.clone();
            tokio::spawn(async move {
                if let Err(e) = ms
                    .reflect_conversation_turn(&ws_id, &aid, &sk, &reflection_model, &turn_messages)
                    .await
                {
                    tracing::warn!(%ws_id, %aid, "Reflection failed: {}", e);
                    if let Some(ref ep) = ep {
                        ep.publish(crate::domains::agent::event::types::AiEvent::ReflectionFailed {
                            workspace_id: ws_id.clone(),
                            agent_id: aid.clone(),
                            session_key: sk.clone(),
                            reason: e.to_string(),
                        });
                    }
                }
            });
        }

        chat_handles_inner.lock().await.remove(&run_id_for_remove);
    });

    chat_handles.lock().await.insert(run_id_for_handle, handle);

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_agent::port::attribution::{Attributable, ProviderKind, Role};
    use tinyiothub_agent::port::provider::{ChatRequest, ChatResponse, ModelProvider};

    /// Builds without touching provider config (the minimax factory requires a
    /// `[minimax]` config section at build time).
    struct ScriptedProvider;

    #[async_trait::async_trait]
    impl ModelProvider for ScriptedProvider {
        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> anyhow::Result<ChatResponse> {
            Ok(ChatResponse {
                text: Some("scripted".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            })
        }
    }

    impl Attributable for ScriptedProvider {
        fn role(&self) -> Role {
            Role::Provider(ProviderKind::Model(
                tinyiothub_agent::port::attribution::ModelProviderKind::Custom,
            ))
        }

        fn alias(&self) -> &str {
            "ScriptedProvider"
        }
    }

    #[tokio::test]
    async fn send_with_pool_rejects_session_from_other_workspace() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&db).await.unwrap();
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            tinyiothub_agent::provider::minimax_provider_factory(),
        )
        .expect("test AgentPool");
        let session_runtime = Arc::new(crate::domains::agent::session::SessionRuntime::new(
            Arc::new(tinyiothub_storage::Db::new(db.clone())),
            Arc::new(crate::domains::agent::runtime::session_stream::SessionStreamHub::default()),
        ));
        let err = send_with_pool(
            &pool,
            &db,
            "agent_main",
            "agent:ws_other:agent_main/s1",
            "hi",
            "r1",
            "",
            "ws_mine",
            &AgentRuntimeConfig::default(),
            None,
            session_runtime,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AgentError::NotFound(_)));
        // The workspace check must run before any agent is built.
        assert_eq!(pool.pool_size(), 0);
    }

    #[test]
    fn chat_timeout_allows_question_answer() {
        use crate::domains::agent::session::{ASK_USER_TIMEOUT, SESSION_TURN_TIMEOUT};
        assert!(SESSION_TURN_TIMEOUT > ASK_USER_TIMEOUT);
        assert!(SESSION_TURN_TIMEOUT >= std::time::Duration::from_secs(15 * 60));
    }

    #[tokio::test]
    async fn ensure_agent_rebuilds_when_session_tools_flip() {
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            Arc::new(|| Ok(Box::new(ScriptedProvider))),
        )
        .expect("test AgentPool");
        let config = AgentRuntimeConfig::default();
        let prompt = crate::domains::agent::prompt::assistant::PromptPackage {
            system_prompt: "stable-prompt".into(),
            turn_input: "input".into(),
        };
        let cached = pool
            .create("agent_main", "ws-test", &config, vec![], false)
            .expect("seed cached agent");
        assert_eq!(pool.agent_session_tools("agent_main"), Some(false));

        let rebuilt = ensure_agent(&pool, "agent_main", "ws-test", &config, true, &prompt)
            .await
            .expect("rebuild with session tools");
        assert!(
            !Arc::ptr_eq(&cached, &rebuilt),
            "tool-set mismatch must invalidate the cached agent"
        );
        assert_eq!(pool.agent_session_tools("agent_main"), Some(true));

        let again = ensure_agent(&pool, "agent_main", "ws-test", &config, true, &prompt)
            .await
            .expect("cache hit");
        assert!(
            Arc::ptr_eq(&rebuilt, &again),
            "matching flavor reuses the cached handle"
        );
    }

    #[tokio::test]
    async fn ensure_agent_rebuilds_only_when_prompt_hash_changes() {
        let pool = AgentPool::new(
            &tinyiothub_core::config::AgentSettings::default(),
            Arc::new(tinyiothub_agent::port::memory::NoopMemory),
            Arc::new(tinyiothub_agent::port::observer::NoopObserver),
            Arc::new(|| Ok(Box::new(ScriptedProvider))),
        )
        .expect("test AgentPool");
        let config = AgentRuntimeConfig::default();
        let prompt_a = crate::domains::agent::prompt::assistant::PromptPackage {
            system_prompt: "prompt-a".into(),
            turn_input: "input".into(),
        };
        let prompt_same = prompt_a.clone();
        let prompt_b = crate::domains::agent::prompt::assistant::PromptPackage {
            system_prompt: "prompt-b".into(),
            turn_input: "input".into(),
        };

        let first = ensure_agent(&pool, "agent_main", "ws-test", &config, false, &prompt_a)
            .await
            .unwrap();
        let same = ensure_agent(&pool, "agent_main", "ws-test", &config, false, &prompt_same)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&first, &same));

        let rebuilt = ensure_agent(&pool, "agent_main", "ws-test", &config, false, &prompt_b)
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &rebuilt));
        let cached = ensure_agent(&pool, "agent_main", "ws-test", &config, false, &prompt_b)
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&rebuilt, &cached));
    }

    #[test]
    fn test_turn_event_chunk_to_delta() {
        let evt = TurnEvent::Chunk {
            delta: "Hello".to_string(),
        };
        let chat_evt = turn_event_to_chat_event(&evt, "run1", "sess1");
        match chat_evt {
            ChatEvent::Delta {
                run_id,
                session_key,
                message,
            } => {
                assert_eq!(run_id, "run1");
                assert_eq!(session_key, "sess1");
                let content = message["content"][0]["text"].as_str().unwrap();
                assert_eq!(content, "Hello");
            }
            _ => panic!("Expected Delta"),
        }
    }

    #[test]
    fn test_turn_event_thinking() {
        let evt = TurnEvent::Thinking {
            delta: "Hmm...".to_string(),
        };
        let chat_evt = turn_event_to_chat_event(&evt, "r", "s");
        match chat_evt {
            ChatEvent::Thinking { thinking, .. } => assert_eq!(thinking, "Hmm..."),
            _ => panic!("Expected Thinking"),
        }
    }

    #[test]
    fn test_turn_event_tool_call_with_canvas() {
        let args = serde_json::json!({"jsonl": "line1\nline2"});
        let evt = TurnEvent::ToolCall {
            name: "canvas".to_string(),
            args: args.clone(),
            id: "tc1".to_string(),
        };
        let chat_evt = turn_event_to_chat_event(&evt, "r", "s");
        match chat_evt {
            ChatEvent::ToolCallStart { tool_name, a2ui, .. } => {
                assert_eq!(tool_name, "canvas");
                assert_eq!(a2ui, Some("line1\nline2".to_string()));
            }
            _ => panic!("Expected ToolCallStart"),
        }
    }

    #[test]
    fn test_turn_event_tool_result() {
        let evt = TurnEvent::ToolResult {
            name: "bad_tool".to_string(),
            output: "error output".to_string(),
            id: "tc1".to_string(),
            attachments: vec![],
        };
        let chat_evt = turn_event_to_chat_event(&evt, "r", "s");
        match chat_evt {
            ChatEvent::ToolResult { tool_name, result, .. } => {
                assert_eq!(tool_name, "bad_tool");
                assert_eq!(result, "error output");
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_turn_event_approval_request() {
        let evt = TurnEvent::ApprovalRequest {
            request_id: "req1".to_string(),
            tool_name: "delete_thing".to_string(),
            arguments_summary: "Delete device X".to_string(),
            timeout_secs: 30,
        };
        let chat_evt = turn_event_to_chat_event(&evt, "r", "s");
        match chat_evt {
            ChatEvent::ToolCallStart {
                tool_name, tool_args, ..
            } => {
                assert_eq!(tool_name, "delete_thing");
                assert_eq!(tool_args, "Delete device X");
            }
            _ => panic!("Expected ToolCallStart"),
        }
    }
}
