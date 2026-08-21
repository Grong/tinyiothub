use serde::{Deserialize, Serialize};

/// Dashboard 统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    /// 设备总数
    pub total_devices: i64,
    /// 在线设备数
    pub online_devices: i64,
    /// 活跃告警数
    pub active_alarms: i64,
    /// 系统状态 (healthy, warning, error)
    pub system_status: String,
    /// 系统运行时间（秒）
    pub system_uptime: i64,
    /// 今日消息数
    pub today_messages: i64,
    /// 月度增长数据
    pub monthly_growth: MonthlyGrowth,
}

/// 月度增长数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyGrowth {
    /// 设备增长数
    pub devices: i64,
    /// 消息增长数
    pub messages: i64,
}

/// 系统性能指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardMetrics {
    /// CPU 使用率 (%)
    pub cpu: f64,
    /// 内存使用率 (%)
    pub memory: f64,
    /// 磁盘使用率 (%)
    pub disk: f64,
    /// 网络指标
    pub network: NetworkMetrics,
}

/// 网络指标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    /// 入站流量 (bytes)
    pub inbound: i64,
    /// 出站流量 (bytes)
    pub outbound: i64,
}

// `DeviceStatusDistribution` / `QuickDevice` moved to
// `tinyiothub_storage::device` (final-review F1) — the device plane
// (query service + dashboard handler) was their only consumer.
