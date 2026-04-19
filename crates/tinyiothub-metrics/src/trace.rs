//! Distributed tracing primitives

use std::collections::HashMap;

/// A trace span representing a unit of work.
#[derive(Debug, Clone)]
pub struct Span {
    pub trace_id: String,
    pub span_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub start_time_ms: u64,
    pub end_time_ms: Option<u64>,
    pub status: SpanStatus,
    pub attributes: HashMap<String, String>,
}

impl Span {
    pub fn new(name: impl Into<String>, trace_id: impl Into<String>, span_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_id: None,
            start_time_ms: 0,
            end_time_ms: None,
            status: SpanStatus::Ok,
            attributes: HashMap::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    pub fn finish(mut self) -> Self {
        self.end_time_ms = Some(0); // Caller should set actual timestamp
        self
    }
}

/// Span execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStatus {
    Ok,
    Error,
}

/// Context propagated across async boundaries.
#[derive(Debug, Clone, Default)]
pub struct SpanContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}
