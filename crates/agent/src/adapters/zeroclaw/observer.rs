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
