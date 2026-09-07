//! port Observer → zeroclaw Observer 桥。

use std::sync::Arc;

use crate::port::observer::Observer;

/// port Observer 包装为 zeroclaw Observer。
///
/// phase 1 无生产端构造 port 观测事件（port::observer::ObserverEvent 是
/// NeverUsed 占位枚举），record_event/record_metric 体留空；phase 2 rig 适配器
/// 经 hooks 重新产生观测事件后再按需接线。
///
/// port Observer 不带 Attributable 超 trait（Task 4 裁定），zeroclaw 侧 Attributable
/// 以固定 role + observer name 作为 alias 满足类型要求（观测事件的归属仅影响日志）。
pub struct PortObserverAsZeroclaw(pub Arc<dyn Observer>);

impl zeroclaw_api::attribution::Attributable for PortObserverAsZeroclaw {
    fn role(&self) -> zeroclaw_api::attribution::Role {
        zeroclaw_api::attribution::Role::Agent
    }
    fn alias(&self) -> &str {
        self.0.name()
    }
}

impl zeroclaw::observability::Observer for PortObserverAsZeroclaw {
    fn record_event(&self, _event: &zeroclaw_api::observability_traits::ObserverEvent) {}

    fn record_metric(&self, _metric: &zeroclaw_api::observability_traits::ObserverMetric) {}

    fn flush(&self) {}

    fn name(&self) -> &str {
        self.0.name()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// zeroclaw Observer 包装为 port Observer（组合层注入用）。
///
/// port 观测事件是 phase 1 占位形状（无生产端构造），record_event/
/// record_metric 体留空；flush/name 直通。
pub struct ZeroclawObserverAsPort(pub Arc<dyn zeroclaw::observability::Observer>);

impl Observer for ZeroclawObserverAsPort {
    fn record_event(&self, _event: &crate::port::observer::ObserverEvent) {}

    fn record_metric(&self, _metric: &crate::port::observer::ObserverMetric) {}

    fn flush(&self) {
        self.0.flush();
    }

    fn name(&self) -> &str {
        self.0.name()
    }
}

/// 用 zeroclaw 观测实现建一个 port Observer（组合层注入入口；backend
/// 字符串映射同迁移前 pool 自建 observer 的逻辑）。
pub fn create_observer(backend: &str) -> Arc<dyn Observer> {
    let backend = match backend {
        "none" | "noop" => zeroclaw::config::schema::ObservabilityBackend::None,
        "verbose" => zeroclaw::config::schema::ObservabilityBackend::Verbose,
        "prometheus" => zeroclaw::config::schema::ObservabilityBackend::Prometheus,
        "otel" | "opentelemetry" | "otlp" => zeroclaw::config::schema::ObservabilityBackend::Otel,
        _ => zeroclaw::config::schema::ObservabilityBackend::Log,
    };
    let config = zeroclaw::config::schema::ObservabilityConfig {
        backend,
        ..Default::default()
    };
    Arc::new(ZeroclawObserverAsPort(Arc::from(
        zeroclaw::observability::create_observer(&config),
    )))
}
