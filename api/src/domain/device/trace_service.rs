use std::sync::Arc;

use crate::{
    application::data_context::DataContext, infrastructure::persistence::Database,
    shared::error::Error,
};

/// 设备追踪服务
/// 负责设备追踪记录的管理和查询
pub struct DeviceTraceService {
    database: Arc<Database>,
    context: Arc<DataContext>,
}

impl DeviceTraceService {
    pub fn new(database: Arc<Database>, context: Arc<DataContext>) -> Self {
        Self { database, context }
    }

    /// 记录设备追踪信息
    pub async fn record_device_trace(
        &self,
        device_id: &str,
        trace_type: &str,
        level: &str,
        category: &str,
        title: &str,
        message: &str,
        details: Option<serde_json::Value>,
        source: Option<&str>,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<String, Error> {
        // 验证设备是否存在
        if self.context.get_device(device_id).is_none() {
            return Err(Error::IOError("Device not found".to_string()));
        }

        // 生成追踪记录ID
        let trace_id = uuid::Uuid::new_v4().to_string();

        // 将详细信息转换为JSON字符串
        let details_json = details.map(|d| d.to_string());

        // 插入追踪记录到数据库
        match sqlx::query(
            "INSERT INTO device_traces (id, device_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))"
        )
        .bind(&trace_id)
        .bind(device_id)
        .bind(trace_type)
        .bind(level)
        .bind(category)
        .bind(title)
        .bind(message)
        .bind(details_json)
        .bind(source.unwrap_or("system"))
        .bind(user_id)
        .bind(session_id)
        .execute(self.database.pool())
        .await
        {
            Ok(_) => {
                tracing::info!(
                    "Device trace recorded: device={}, type={}, level={}, title={}, trace_id={}",
                    device_id, trace_type, level, title, trace_id
                );

                // 发布追踪记录事件，可能触发告警或通知
                if level == "error" || level == "critical" {
                    tracing::warn!(
                        "Critical trace recorded for device {}: {} - {}",
                        device_id, title, message
                    );
                }

                Ok(trace_id)
            }
            Err(e) => {
                tracing::error!("Failed to record device trace: {}", e);
                Err(Error::IOError(format!("Failed to record trace: {}", e)))
            }
        }
    }

    /// 获取设备追踪记录
    pub async fn get_device_traces(
        &self,
        device_id: &str,
        trace_types: Option<&[String]>,
        levels: Option<&[String]>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<DeviceTrace>, Error> {
        // 验证设备是否存在
        if self.context.get_device(device_id).is_none() {
            return Err(Error::NotFound);
        }

        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        // 构建基础查询
        let mut query = "SELECT id, device_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at FROM device_traces WHERE device_id = ?".to_string();
        let mut bind_values: Vec<String> = vec![device_id.to_string()];

        // 添加类型过滤
        if let Some(types) = trace_types {
            if !types.is_empty() {
                let placeholders = types.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND trace_type IN ({})", placeholders));
                bind_values.extend(types.iter().cloned());
            }
        }

        // 添加级别过滤
        if let Some(lvls) = levels {
            if !lvls.is_empty() {
                let placeholders = lvls.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND level IN ({})", placeholders));
                bind_values.extend(lvls.iter().cloned());
            }
        }

        // 按时间倒序排列并分页
        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        bind_values.push(limit.to_string());
        bind_values.push(offset.to_string());

        // 动态绑定参数 - 使用 fold 避免 let mut 生命周期问题
        let query_builder = bind_values.iter().fold(
            sqlx::query_as::<_, DeviceTrace>(sqlx::AssertSqlSafe(query)),
            |qb, value| qb.bind(value)
        );

        // 执行查询
        match query_builder.fetch_all(self.database.pool()).await {
            Ok(traces) => {
                tracing::debug!(
                    "Retrieved {} trace records for device {}",
                    traces.len(),
                    device_id
                );
                Ok(traces)
            }
            Err(e) => {
                tracing::error!("Failed to get device traces for {}: {}", device_id, e);
                Err(Error::IOError(format!("Failed to get traces: {}", e)))
            }
        }
    }

    /// 获取追踪记录统计信息
    pub async fn get_device_trace_statistics(
        &self,
        device_id: &str,
        days: Option<u32>,
    ) -> Result<DeviceTraceStatistics, Error> {
        // 验证设备是否存在
        if self.context.get_device(device_id).is_none() {
            return Err(Error::NotFound);
        }

        let days = days.unwrap_or(7);

        // 查询总记录数
        let total_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND created_at > datetime('now', ?)"
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(e) => {
                tracing::debug!("Failed to query total traces for {}: {}", device_id, e);
                0
            }
        };

        // 查询各级别的记录数
        let error_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND level IN ('error', 'critical') AND created_at > datetime('now', ?)"
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        let warning_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND level = 'warn' AND created_at > datetime('now', ?)"
        )
        .bind(device_id)
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        let info_traces = total_traces - error_traces - warning_traces;

        // 查询最近的追踪记录
        let last_trace_time = match sqlx::query_scalar::<_, String>(
            "SELECT created_at FROM device_traces WHERE device_id = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(device_id)
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(time)) => Some(time),
            Ok(None) => None,
            Err(_) => None,
        };

        Ok(DeviceTraceStatistics {
            device_id: device_id.to_string(),
            total_traces,
            error_traces,
            warning_traces,
            info_traces,
            days_range: days,
            last_trace_time,
            last_updated: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    /// 清理设备追踪记录
    pub async fn clear_device_traces(
        &self,
        device_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32, Error> {
        // 验证设备是否存在
        if self.context.get_device(device_id).is_none() {
            return Err(Error::IOError("Device not found".to_string()));
        }

        let mut query = "DELETE FROM device_traces WHERE device_id = ?".to_string();
        let mut bind_values: Vec<String> = vec![device_id.to_string()];

        // 添加时间条件
        if let Some(date) = before_date {
            query.push_str(" AND created_at < ?");
            bind_values.push(date.to_string());
        }

        // 添加类型条件
        if let Some(types) = trace_types {
            if !types.is_empty() {
                let placeholders = types.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                query.push_str(&format!(" AND trace_type IN ({})", placeholders));
                bind_values.extend(types.iter().cloned());
            }
        }

        // 动态绑定参数 - 使用 fold 避免 let mut 生命周期问题
        let query_builder = bind_values.iter().fold(
            sqlx::query(sqlx::AssertSqlSafe(query.clone())),
            |qb, value| qb.bind(value)
        );

        // 执行删除操作
        match query_builder.execute(self.database.pool()).await {
            Ok(result) => {
                let cleared_count = result.rows_affected() as u32;
                tracing::info!(
                    "Cleared {} trace records for device {}, before_date={:?}, types={:?}",
                    cleared_count,
                    device_id,
                    before_date,
                    trace_types
                );
                Ok(cleared_count)
            }
            Err(e) => {
                tracing::error!("Failed to clear device traces: {}", e);
                Err(Error::IOError(format!("Failed to clear traces: {}", e)))
            }
        }
    }

    /// 批量清理过期的追踪记录
    pub async fn cleanup_expired_traces(&self, days_to_keep: u32) -> Result<u32, Error> {
        match sqlx::query("DELETE FROM device_traces WHERE created_at < datetime('now', ?)")
            .bind(format!("-{} days", days_to_keep))
            .execute(self.database.pool())
            .await
        {
            Ok(result) => {
                let cleaned_count = result.rows_affected() as u32;
                tracing::info!(
                    "Cleaned up {} expired trace records (older than {} days)",
                    cleaned_count,
                    days_to_keep
                );
                Ok(cleaned_count)
            }
            Err(e) => {
                tracing::error!("Failed to cleanup expired traces: {}", e);
                Err(Error::IOError(format!("Failed to cleanup traces: {}", e)))
            }
        }
    }

    /// 获取系统追踪记录概览
    pub async fn get_system_trace_overview(&self, days: Option<u32>) -> SystemTraceOverview {
        let days = days.unwrap_or(7);

        // 查询总记录数
        let total_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE created_at > datetime('now', ?)",
        )
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        // 查询错误记录数
        let error_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE level IN ('error', 'critical') AND created_at > datetime('now', ?)"
        )
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        // 查询警告记录数
        let warning_traces = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE level = 'warn' AND created_at > datetime('now', ?)"
        )
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        let info_traces = total_traces - error_traces - warning_traces;

        // 查询活跃设备数（有追踪记录的设备）
        let active_devices = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT device_id) FROM device_traces WHERE created_at > datetime('now', ?)"
        )
        .bind(format!("-{} days", days))
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            Ok(None) => 0,
            Err(_) => 0,
        };

        SystemTraceOverview {
            total_traces,
            error_traces,
            warning_traces,
            info_traces,
            active_devices,
            days_range: days,
            last_updated: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// 设备追踪记录
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct DeviceTrace {
    pub id: String,
    pub device_id: String,
    pub trace_type: String,
    pub level: String,
    pub category: String,
    pub title: String,
    pub message: String,
    pub details: Option<String>,
    pub source: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
}

/// 设备追踪记录统计
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceTraceStatistics {
    pub device_id: String,
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub days_range: u32,
    pub last_trace_time: Option<String>,
    pub last_updated: String,
}

/// 系统追踪记录概览
#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemTraceOverview {
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub active_devices: u32,
    pub days_range: u32,
    pub last_updated: String,
}
