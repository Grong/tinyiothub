//! QuestionBroker — 把 `ask_user` 的阻塞等待与 HTTP 回答端点解耦。
//!
//! 工具注册一个 oneshot，事件 `question.asked` 把 question_id 送到前端；
//! 回答端点按 session_key 校验后 resolve。

use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use thiserror::Error;
use tokio::sync::oneshot;

use super::types::QuestionAnswer;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum QuestionError {
    #[error("question not found")]
    NotFound,
    #[error("question belongs to another session")]
    SessionMismatch,
    #[error("question is no longer waiting for an answer")]
    Closed,
}

struct PendingQuestion {
    session_key: String,
    tx: oneshot::Sender<QuestionAnswer>,
}

#[derive(Default)]
pub struct QuestionBroker {
    pending: DashMap<String, PendingQuestion>,
}

impl QuestionBroker {
    pub fn register(&self, session_key: &str) -> (String, oneshot::Receiver<QuestionAnswer>) {
        let question_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.insert(
            question_id.clone(),
            PendingQuestion {
                session_key: session_key.to_string(),
                tx,
            },
        );
        (question_id, rx)
    }

    pub fn answer(&self, session_key: &str, question_id: &str, answer: QuestionAnswer) -> Result<(), QuestionError> {
        // Hold the shard lock for the whole check-and-remove: a concurrent
        // wrong-session attempt must not remove the entry out from under the
        // legitimate owner (which would surface as a spurious `NotFound`).
        match self.pending.entry(question_id.to_string()) {
            Entry::Occupied(entry) => {
                if entry.get().session_key != session_key {
                    return Err(QuestionError::SessionMismatch);
                }
                let pending = entry.remove();
                pending.tx.send(answer).map_err(|_| QuestionError::Closed)
            }
            Entry::Vacant(_) => Err(QuestionError::NotFound),
        }
    }

    pub fn cancel(&self, question_id: &str) {
        self.pending.remove(question_id);
    }

    /// HUD 用：当前 workspace 下等待回答的问题数。
    pub fn count_pending_for_workspace(&self, workspace_id: &str) -> usize {
        let prefix = format!("agent:{workspace_id}:");
        self.pending
            .iter()
            .filter(|entry| entry.value().session_key.starts_with(&prefix))
            .count()
    }
}

pub type SharedQuestionBroker = Arc<QuestionBroker>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::types::{QuestionAnswer, QuestionResponse};

    #[tokio::test]
    async fn answer_resolves_registered_question() {
        let broker = QuestionBroker::default();
        let (id, rx) = broker.register("agent:ws:a/s1");
        let answer = QuestionAnswer {
            answers: vec![QuestionResponse {
                question_index: 0,
                selected: vec!["全部".into()],
                text: None,
            }],
        };
        broker.answer("agent:ws:a/s1", &id, answer.clone()).unwrap();
        assert_eq!(rx.await.unwrap(), answer);
    }

    #[tokio::test]
    async fn wrong_session_cannot_answer() {
        let broker = QuestionBroker::default();
        let (id, rx) = broker.register("agent:ws:a/s1");
        let error = broker
            .answer("agent:ws:a/s2", &id, QuestionAnswer { answers: vec![] })
            .unwrap_err();
        assert!(matches!(error, QuestionError::SessionMismatch));

        // A rejected attempt must leave the question answerable by the owner.
        let answer = QuestionAnswer {
            answers: vec![QuestionResponse {
                question_index: 0,
                selected: vec!["全部".into()],
                text: None,
            }],
        };
        broker.answer("agent:ws:a/s1", &id, answer.clone()).unwrap();
        assert_eq!(rx.await.unwrap(), answer);
    }

    #[test]
    fn wrong_session_race_does_not_block_legitimate_answer() {
        let broker = QuestionBroker::default();
        for _ in 0..2000 {
            let (id, _rx) = broker.register("agent:ws:a/s1");
            let barrier = std::sync::Barrier::new(2);
            let id = &id;
            let (wrong, right) = std::thread::scope(|scope| {
                let wrong = scope.spawn(|| {
                    barrier.wait();
                    broker.answer("agent:ws:a/s2", id, QuestionAnswer { answers: vec![] })
                });
                let right = scope.spawn(|| {
                    barrier.wait();
                    broker.answer("agent:ws:a/s1", id, QuestionAnswer { answers: vec![] })
                });
                (wrong.join().unwrap(), right.join().unwrap())
            });
            assert!(right.is_ok(), "legitimate answer lost the race");
            assert!(matches!(
                wrong,
                Err(QuestionError::SessionMismatch) | Err(QuestionError::NotFound)
            ));
        }
    }

    #[test]
    fn unknown_question_is_not_found() {
        let broker = QuestionBroker::default();
        let error = broker
            .answer("agent:ws:a/s1", "missing", QuestionAnswer { answers: vec![] })
            .unwrap_err();
        assert!(matches!(error, QuestionError::NotFound));
    }

    #[test]
    fn counts_pending_questions_by_workspace() {
        let broker = QuestionBroker::default();
        let (_a, _ra) = broker.register("agent:ws-a:a/s1");
        let (_b, _rb) = broker.register("agent:ws-a:a/s2");
        let (_c, _rc) = broker.register("agent:ws-b:a/s1");
        assert_eq!(broker.count_pending_for_workspace("ws-a"), 2);
        assert_eq!(broker.count_pending_for_workspace("ws-b"), 1);
        assert_eq!(broker.count_pending_for_workspace("ws-c"), 0);
    }
}
