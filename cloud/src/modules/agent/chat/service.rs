// Stateless ChatService — zeroclaw Agent passed as parameter.
// Eliminates the SSE serialization round-trip:
//   Before: zeroclaw TurnEvent → bytes → reqwest::Response → parse bytes → ChatEvent → SSE
//   After:  zeroclaw TurnEvent → ChatEvent → SSE

use std::sync::Arc;

use tokio::sync::mpsc;
use zeroclaw::agent::TurnEvent;

use crate::modules::agent::types::{ChatError, ChatEvent};

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
        TurnEvent::ApprovalRequest { tool_name, arguments_summary, .. } => {
            ChatEvent::ToolCallStart {
                run_id: run_id.to_string(),
                session_key: session_key.to_string(),
                tool_name: tool_name.clone(),
                tool_args: arguments_summary.clone(),
                a2ui: None,
            }
        }
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

/// Send a chat message to a zeroclaw Agent and receive ChatEvents directly.
///
/// Returns an mpsc::Receiver<ChatEvent> — no bytes round-trip.
#[allow(clippy::too_many_arguments)]
pub async fn send_message(
    agent: &Arc<tokio::sync::Mutex<zeroclaw::agent::Agent>>,
    message: &str,
    run_id: &str,
    session_key: &str,
    system_prompt: &str,
    chat_handles: &Arc<
        tokio::sync::Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>,
    >,
    reflection_service: Option<
        std::sync::Arc<super::super::reflection::service::ReflectionService>,
    >,
    enable_reflection: bool,
    model: &str,
    workspace_id: &str,
    agent_id: &str,
) -> Result<mpsc::Receiver<ChatEvent>, ChatError> {
    let agent = Arc::clone(agent);
    let message = message.to_string();
    let run_id = run_id.to_string();
    let session_key = session_key.to_string();
    let system_prompt = system_prompt.to_string();
    let chat_handles = Arc::clone(chat_handles);
    let chat_handles_inner = Arc::clone(&chat_handles);
    let workspace_id = workspace_id.to_string();
    let agent_id = agent_id.to_string();
    let reflection_model = model.to_string();

    let (tx, rx) = mpsc::channel::<ChatEvent>(100);

    let run_id_for_handle = run_id.clone();
    let run_id_for_remove = run_id.clone();
    let handle = tokio::spawn(async move {
        // Set system prompt on first message
        {
            let mut ag = agent.lock().await;
            if ag.history().is_empty() && !system_prompt.is_empty() {
                ag.seed_history(&[zeroclaw::providers::traits::ChatMessage {
                    role: "system".into(),
                    content: system_prompt,
                }]);
            }
        }

        // Create TurnEvent channel
        let (event_tx, event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(32);
        let event_rx = Arc::new(tokio::sync::Mutex::new(event_rx));

        // Spawn forward task: TurnEvent → ChatEvent → tx
        let forward_tx = tx.clone();
        let forward_run = run_id.clone();
        let forward_session = session_key.clone();
        tokio::spawn(async move {
            let mut rx = event_rx.lock().await;
            while let Some(evt) = rx.recv().await {
                let chat_event = turn_event_to_chat_event(&evt, &forward_run, &forward_session);
                // Skip usage-only events
                if let ChatEvent::Delta { message, .. } = &chat_event
                    && message.get("__usage").is_some()
                {
                    continue;
                }
                if forward_tx.send(chat_event).await.is_err() {
                    break;
                }
            }
        });

        // Run turn_streamed with 120s timeout
        let mut ag = agent.lock().await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            ag.turn_streamed(&message, event_tx, None),
        )
        .await;
        drop(ag);

        let final_text = match result {
            Ok(Ok(text)) => {
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
            Ok(Err(e)) => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: e.to_string(),
                    })
                    .await;
                None
            }
            Err(_) => {
                let _ = tx
                    .send(ChatEvent::Error {
                        run_id: run_id.clone(),
                        session_key: session_key.clone(),
                        error: "Agent execution timed out after 120 seconds".to_string(),
                    })
                    .await;
                None
            }
        };

        // Spawn micro_reflect after the turn completes (fire-and-forget)
        if enable_reflection
            && let (Some(svc), Some(assistant_text)) = (reflection_service, final_text)
        {
            let turn_messages = vec![
                super::super::reflection::pipeline::ChatMessage {
                    role: "user".into(),
                    content: message.clone(),
                },
                super::super::reflection::pipeline::ChatMessage {
                    role: "assistant".into(),
                    content: assistant_text,
                },
            ];
            tokio::spawn(async move {
                svc.micro_reflect(
                    &workspace_id,
                    &agent_id,
                    &session_key,
                    &reflection_model,
                    &turn_messages,
                )
                .await;
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

    #[test]
    fn test_turn_event_chunk_to_delta() {
        let evt = TurnEvent::Chunk { delta: "Hello".to_string() };
        let chat_evt = turn_event_to_chat_event(&evt, "run1", "sess1");
        match chat_evt {
            ChatEvent::Delta { run_id, session_key, message } => {
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
        let evt = TurnEvent::Thinking { delta: "Hmm...".to_string() };
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
            tool_name: "delete_device".to_string(),
            arguments_summary: "Delete device X".to_string(),
            timeout_secs: 30,
        };
        let chat_evt = turn_event_to_chat_event(&evt, "r", "s");
        match chat_evt {
            ChatEvent::ToolCallStart { tool_name, tool_args, .. } => {
                assert_eq!(tool_name, "delete_device");
                assert_eq!(tool_args, "Delete device X");
            }
            _ => panic!("Expected ToolCallStart"),
        }
    }
}
