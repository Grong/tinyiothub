//! Assistant Session 的值类型 — 纯数据，无 I/O。
//!
//! 这些类型是 Session Loop、原语工具、HTTP DTO 的共享词汇。wire 用 camelCase，
//! Rust 字段用 snake_case。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Chat,
    Scheduled,
    Event,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Scheduled => "scheduled",
            Self::Event => "event",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "scheduled" => Some(Self::Scheduled),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

// 值类型住 core（db/assistant_schedule 也持久化该模式）；此处 re-export
// 保持会话模块的既有路径 `session::types::PermissionMode` 不变。
pub use tinyiothub_core::policy::PermissionMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub content: String,
    pub status: TodoStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_form: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoList {
    pub todos: Vec<Todo>,
}

impl TodoList {
    /// Claude Code 语义：非空、最多 50 项、最多一个 in_progress。
    pub fn validate(&self) -> Result<(), String> {
        if self.todos.is_empty() {
            return Err("todo list must not be empty".to_string());
        }
        if self.todos.len() > 50 {
            return Err("todo list exceeds 50 items".to_string());
        }
        let in_progress = self
            .todos
            .iter()
            .filter(|todo| todo.status == TodoStatus::InProgress)
            .count();
        if in_progress > 1 {
            return Err("at most one todo may be in_progress".to_string());
        }
        for todo in &self.todos {
            if todo.content.trim().is_empty() {
                return Err("todo content must not be empty".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multi_select: bool,
}

impl Question {
    /// Claude Code 语义：1–4 个问题，每题 2–4 个选项，header 不超过 12 字符。
    pub fn validate_set(questions: &[Self]) -> Result<(), String> {
        if questions.is_empty() || questions.len() > 4 {
            return Err("ask_user requires 1 to 4 questions".to_string());
        }
        for question in questions {
            if question.question.trim().is_empty() {
                return Err("question text must not be empty".to_string());
            }
            if question.header.trim().is_empty() {
                return Err("question header must not be empty".to_string());
            }
            if question.header.chars().count() > 12 {
                return Err("question header must be at most 12 characters".to_string());
            }
            if question.options.len() < 2 || question.options.len() > 4 {
                return Err("each question requires 2 to 4 options".to_string());
            }
            for option in &question.options {
                if option.label.trim().is_empty() {
                    return Err("question option label must not be empty".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionResponse {
    pub question_index: usize,
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub answers: Vec<QuestionResponse>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_defaults_to_ask() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    }

    #[test]
    fn permission_mode_round_trips() {
        for mode in [
            PermissionMode::ReadOnly,
            PermissionMode::Ask,
            PermissionMode::FullAccess,
        ] {
            assert_eq!(PermissionMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(PermissionMode::parse("allow_all"), None);
    }

    #[test]
    fn session_kind_round_trips() {
        for kind in [SessionKind::Chat, SessionKind::Scheduled, SessionKind::Event] {
            assert_eq!(SessionKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(SessionKind::parse("workflow"), None);
    }

    #[test]
    fn todo_list_rejects_two_in_progress() {
        let list = TodoList {
            todos: vec![
                Todo {
                    content: "检查离线事件".into(),
                    status: TodoStatus::InProgress,
                    active_form: "检查离线事件".into(),
                },
                Todo {
                    content: "检查设备异常".into(),
                    status: TodoStatus::InProgress,
                    active_form: "检查设备异常".into(),
                },
            ],
        };
        assert!(list.validate().is_err());
    }

    #[test]
    fn todo_list_accepts_one_in_progress() {
        let list = TodoList {
            todos: vec![Todo {
                content: "检查离线事件".into(),
                status: TodoStatus::InProgress,
                active_form: "检查离线事件".into(),
            }],
        };
        assert!(list.validate().is_ok());
    }

    #[test]
    fn question_set_rejects_out_of_range_options() {
        let question = Question {
            question: "要检查哪些设备？".into(),
            header: "检查范围".into(),
            options: vec![QuestionOption {
                label: "全部".into(),
                description: "检查全部设备".into(),
            }],
            multi_select: false,
        };
        assert!(Question::validate_set(std::slice::from_ref(&question)).is_err());
    }

    #[test]
    fn question_set_accepts_valid_question() {
        let question = Question {
            question: "要检查哪些设备？".into(),
            header: "检查范围".into(),
            options: vec![
                QuestionOption {
                    label: "全部".into(),
                    description: "检查全部设备".into(),
                },
                QuestionOption {
                    label: "仅离线".into(),
                    description: "只检查离线设备".into(),
                },
            ],
            multi_select: true,
        };
        assert!(Question::validate_set(&[question]).is_ok());
    }
}
