use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tokio::time::timeout;

use crate::domains::agent::session::context::TurnContextRegistry;
use crate::domains::agent::session::events::{SessionEventSink, session_event};
use crate::domains::agent::session::questions::QuestionBroker;
use crate::domains::agent::session::types::{Question, QuestionAnswer};

pub struct AskUserTool {
    assistant_id: String,
    contexts: Arc<TurnContextRegistry>,
    broker: Arc<QuestionBroker>,
    events: Arc<dyn SessionEventSink>,
    timeout: Duration,
}

impl AskUserTool {
    pub fn new(
        assistant_id: String,
        contexts: Arc<TurnContextRegistry>,
        broker: Arc<QuestionBroker>,
        events: Arc<dyn SessionEventSink>,
        timeout: Duration,
    ) -> Self {
        Self {
            assistant_id,
            contexts,
            broker,
            events,
            timeout,
        }
    }

    async fn run(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let Some(context) = self.contexts.get(&self.assistant_id) else {
            return Ok(failed(format!(
                "turn context missing for assistant {}",
                self.assistant_id
            )));
        };
        let questions: Vec<Question> = match args.get("questions").cloned() {
            Some(value) => match serde_json::from_value(value) {
                Ok(questions) => questions,
                Err(error) => return Ok(failed(format!("invalid questions: {error}"))),
            },
            None => return Ok(failed("missing questions")),
        };
        if let Err(message) = Question::validate_set(&questions) {
            return Ok(failed(message));
        }

        let (question_id, receiver) = self.broker.register(&context.session_key);
        if let Err(error) = self
            .events
            .emit(session_event(
                &context,
                "question/asked",
                json!({
                    "questionId": question_id.clone(),
                    "questions": questions.clone()
                }),
            ))
            .await
        {
            self.broker.cancel(&question_id);
            return Err(error.into());
        }

        match timeout(self.timeout, receiver).await {
            Ok(Ok(answer)) => {
                let output = format_answers(&questions, &answer);
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question/answered",
                        json!({
                            "questionId": question_id,
                            "answers": answer.answers,
                            "timedOut": false
                        }),
                    ))
                    .await;
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                    attachments: vec![],
                })
            }
            Ok(Err(_)) => {
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question/answered",
                        json!({
                            "questionId": question_id,
                            "answers": [],
                            "timedOut": false
                        }),
                    ))
                    .await;
                Ok(failed("用户未回答（连接已关闭）"))
            }
            Err(_) => {
                self.broker.cancel(&question_id);
                let _ = self
                    .events
                    .emit(session_event(
                        &context,
                        "question/answered",
                        json!({
                            "questionId": question_id,
                            "answers": [],
                            "timedOut": true
                        }),
                    ))
                    .await;
                Ok(failed("用户未在时限内回答"))
            }
        }
    }
}

fn failed(error: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(error.into()),
        attachments: vec![],
    }
}

fn format_answers(questions: &[Question], answer: &QuestionAnswer) -> String {
    let mut output = String::from("用户回答：\n");
    for (index, question) in questions.iter().enumerate() {
        let response = answer.answers.iter().find(|response| response.question_index == index);
        let selected = response
            .map(|response| response.selected.join("、"))
            .filter(|selected| !selected.is_empty())
            .unwrap_or_else(|| "（未选择）".to_string());
        output.push_str(&format!(
            "{}. {}\n   选择：{}\n",
            index + 1,
            question.question,
            selected
        ));
        if let Some(text) = response.and_then(|response| response.text.as_deref())
            && !text.trim().is_empty()
        {
            output.push_str(&format!("   补充：{}\n", text.trim()));
        }
    }
    output
}

impl Attributable for AskUserTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }

    fn alias(&self) -> &str {
        "ask_user"
    }
}

#[async_trait::async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user 1-4 blocking questions with 2-4 options each. Use this when a decision cannot be inferred safely."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string"},
                            "header": {"type": "string", "maxLength": 12},
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {"type": "string"},
                                        "description": {"type": "string"}
                                    },
                                    "required": ["label", "description"]
                                }
                            },
                            "multiSelect": {"type": "boolean"}
                        },
                        "required": ["question", "header", "options"]
                    }
                }
            },
            "required": ["questions"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        self.run(args).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::context::TurnContext;
    use crate::domains::agent::session::events::test_support::RecordingSink;
    use crate::domains::agent::session::questions::QuestionBroker;
    use crate::domains::agent::session::types::{QuestionAnswer, QuestionResponse};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    fn args() -> serde_json::Value {
        json!({
            "questions": [{
                "question": "要检查哪些设备？",
                "header": "检查范围",
                "options": [
                    {"label": "全部", "description": "检查全部设备"},
                    {"label": "仅离线", "description": "只检查离线设备"}
                ],
                "multiSelect": false
            }]
        })
    }

    #[tokio::test]
    async fn ask_user_blocks_until_answered() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let broker = Arc::new(QuestionBroker::default());
        let sink = Arc::new(RecordingSink::default());
        let tool = Arc::new(AskUserTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            broker.clone(),
            sink.clone(),
            Duration::from_secs(5),
        ));
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let handle = tokio::spawn({
            let tool = tool.clone();
            async move { tool.execute(args()).await }
        });

        // 等 question/asked 落进 sink，再回答。
        let mut question_id = None;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let events = sink.events.lock().await;
            if let Some(event) = events.iter().find(|event| event.event_type == "question/asked") {
                question_id = event.payload_json["questionId"].as_str().map(ToString::to_string);
                break;
            }
        }
        let question_id = question_id.expect("question/asked emitted");
        broker
            .answer(
                "agent:ws:agent_main/s1",
                &question_id,
                QuestionAnswer {
                    answers: vec![QuestionResponse {
                        question_index: 0,
                        selected: vec!["仅离线".into()],
                        text: None,
                    }],
                },
            )
            .unwrap();

        let result = handle.await.unwrap().unwrap();
        assert!(result.success);
        assert!(result.output.contains("仅离线"));
        let events = sink.events.lock().await;
        assert!(events.iter().any(|event| event.event_type == "question/answered"));
    }

    #[tokio::test]
    async fn ask_user_times_out_without_answer() {
        let contexts = Arc::new(TurnContextRegistry::default());
        let tool = AskUserTool::new(
            "agent_main".to_string(),
            contexts.clone(),
            Arc::new(QuestionBroker::default()),
            Arc::new(RecordingSink::default()),
            Duration::from_millis(10),
        );
        let ctx = TurnContext::new(
            "ws".into(),
            "agent_main".into(),
            "agent:ws:agent_main/s1".into(),
            "run1".into(),
            0,
        );
        let _guard = contexts.set(ctx);

        let result = tool.execute(args()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("未在时限内回答"));
    }
}
