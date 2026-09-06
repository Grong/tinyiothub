//! Observer 接口面 — vendored 自 zeroclaw-api/src/observability_traits.rs。
//!
//! `ObserverEvent`/`ObserverMetric` 的 zeroclaw 原始形状各拖 20+ 变体
//! （AgentStart/LlmRequest/ToolCall/... 全生命周期事件），本仓库 phase 1
//! 没有任何生产端构造它们 —— 故 port 为最小枚举（单占位变体）。
//! phase 2 rig 适配器经 hooks 重新产生观测事件时，再按需扩变体。

/// Discrete events emitted by the agent runtime for observability.
///
/// phase 1 占位形状：rig 适配器（phase 2）经 hooks 重新产生观测事件后
/// 再扩充实质变体。
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ObserverEvent {
    /// 占位变体 —— phase 1 不产生真实观测事件。
    #[doc(hidden)]
    NeverUsed,
}

/// Numeric metrics emitted by the agent runtime.
///
/// 同 [`ObserverEvent`]：phase 1 占位形状，phase 2 经 hooks 重新产生。
#[derive(Debug, Clone)]
pub enum ObserverMetric {
    /// 占位变体 —— phase 1 不产生真实观测指标。
    #[doc(hidden)]
    NeverUsed,
}

/// Core observability trait for recording agent runtime telemetry.
///
/// Implement this trait to integrate with any monitoring backend (structured
/// logging, Prometheus, OpenTelemetry, etc.). The agent runtime holds one or
/// more `Observer` instances and calls [`record_event`](Observer::record_event)
/// and [`record_metric`](Observer::record_metric) at key lifecycle points.
///
/// Implementations must be `Send + Sync + 'static` because the observer is
/// shared across async tasks via `Arc`.
pub trait Observer: Send + Sync + 'static {
    /// Record a discrete lifecycle event.
    ///
    /// Called synchronously on the hot path; implementations should avoid
    /// blocking I/O. Buffer events internally and flush asynchronously
    /// when possible.
    fn record_event(&self, event: &ObserverEvent);

    /// Record a numeric metric sample.
    ///
    /// Called synchronously; same non-blocking guidance as
    /// [`record_event`](Observer::record_event).
    fn record_metric(&self, metric: &ObserverMetric);

    /// Flush any buffered telemetry data to the backend.
    ///
    /// The runtime calls this during graceful shutdown. The default
    /// implementation is a no-op, which is appropriate for backends
    /// that write synchronously.
    fn flush(&self) {}

    /// Return the human-readable name of this observer backend.
    ///
    /// Used in logs and diagnostics (e.g., `"console"`, `"prometheus"`,
    /// `"opentelemetry"`).
    fn name(&self) -> &str;
}

/// No-op observer — 供测试与 phase 2 未接线场景。
pub struct NoopObserver;

impl Observer for NoopObserver {
    fn record_event(&self, _event: &ObserverEvent) {}

    fn record_metric(&self, _metric: &ObserverMetric) {}

    fn name(&self) -> &str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_observer_records_nothing() {
        let observer = NoopObserver;
        observer.record_event(&ObserverEvent::NeverUsed);
        observer.record_metric(&ObserverMetric::NeverUsed);
        observer.flush();
        assert_eq!(observer.name(), "noop");
    }
}
