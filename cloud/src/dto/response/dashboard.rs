use chrono::{DateTime, Utc};
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

/// 设备状态分布
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatusDistribution {
    /// 在线设备数
    pub online: i64,
    /// 离线设备数
    pub offline: i64,
    /// 故障设备数
    pub error: i64,
    /// 维护中设备数
    pub maintenance: i64,
}

/// 关键设备信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickDevice {
    /// 设备ID
    pub id: String,
    /// 设备名称
    pub name: String,
    /// 设备状态
    pub status: String,
    /// 最后在线时间
    pub last_seen: DateTime<Utc>,
    /// 设备类型
    pub device_type: String,
}

/// 最新告警信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentAlarm {
    /// 告警ID
    pub id: String,
    /// 设备ID
    pub device_id: String,
    /// 设备名称
    pub device_name: String,
    /// 告警级别
    pub level: String,
    /// 告警消息
    pub message: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 告警状态
    pub status: String,
}
