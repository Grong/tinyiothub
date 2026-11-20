//! TodoStore — 当前会话的 Todo 列表缓存。
//!
//! DB 里的 `todo/write` 事件是真相；这里只保存最近一次写入，供同进程内的
//! `/sessions/{key}/todos` 快速读取。重启后由事件重放恢复。

use std::sync::Arc;

use dashmap::DashMap;

use super::types::Todo;

#[derive(Default)]
pub struct TodoStore {
    by_session: DashMap<String, Vec<Todo>>,
}

impl TodoStore {
    pub fn replace(&self, session_key: &str, todos: Vec<Todo>) -> Vec<Todo> {
        self.by_session.insert(session_key.to_string(), todos.clone());
        todos
    }

    pub fn get(&self, session_key: &str) -> Vec<Todo> {
        self.by_session
            .get(session_key)
            .map(|entry| entry.value().clone())
            .unwrap_or_default()
    }
}

pub type SharedTodoStore = Arc<TodoStore>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::session::types::{Todo, TodoStatus};

    #[test]
    fn replace_and_get_are_session_scoped() {
        let store = TodoStore::default();
        let todo = Todo {
            content: "检查离线事件".into(),
            status: TodoStatus::Pending,
            active_form: "检查离线事件".into(),
        };
        store.replace("s1", vec![todo.clone()]);
        assert_eq!(store.get("s1"), vec![todo]);
        assert!(store.get("s2").is_empty());
    }
}
