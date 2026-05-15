use tinyiothub_core::{
    error::{Error, Result},
    now_string,
};

use crate::{
    modules::device::trace_service::{DeviceTrace, DeviceTraceStatistics, SystemTraceOverview},
    shared::persistence::Database,
};

/// 设备追踪记录仓库 - 处理所有 device_traces 表的数据库操作
#[derive(Debug, Clone)]
pub struct DeviceTraceRepository {
    database: Database,
}

impl DeviceTraceRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// 检查设备是否存在
    pub async fn device_exists(&self, device_id: &str) -> Result<bool> {
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices WHERE id = ?")
            .bind(device_id)
            .fetch_optional(self.database.pool())
            .await
        {
            Ok(Some(count)) => Ok(count > 0),
            Ok(None) => Ok(false),
            Err(e) => {
                tracing::debug!("Failed to check device existence for {}: {}", device_id, e);
                Err(Error::IOError(format!("DB error: {}", e)))
            }
        }
    }

    /// 插入追踪记录
    pub async fn insert_trace(
        &self,
        trace_id: &str,
        device_id: &str,
        trace_type: &str,
        level: &str,
        category: &str,
        title: &str,
        message: &str,
        details_json: Option<String>,
        source: &str,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO device_traces (id, device_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))"
        )
        .bind(trace_id)
        .bind(device_id)
        .bind(trace_type)
        .bind(level)
        .bind(category)
        .bind(title)
        .bind(message)
        .bind(details_json)
        .bind(source)
        .bind(user_id)
        .bind(session_id)
        .execute(self.database.pool())
        .await
        .map_err(|e| Error::IOError(format!("Failed to record trace: {}", e)))?;

        Ok(())
    }

    /// 查询设备追踪记录（支持过滤和分页）
    pub async fn find_traces(
        &self,
        device_id: &str,
        trace_types: Option<&[String]>,
        levels: Option<&[String]>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<DeviceTrace>> {
        let mut query = "SELECT id, device_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at FROM device_traces WHERE device_id = ?".to_string();
        let mut bind_values: Vec<String> = vec![device_id.to_string()];

        if let Some(types) = trace_types
            && !types.is_empty()
        {
            let placeholders = types.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND trace_type IN ({})", placeholders));
            bind_values.extend(types.iter().cloned());
        }

        if let Some(lvls) = levels
            && !lvls.is_empty()
        {
            let placeholders = lvls.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND level IN ({})", placeholders));
            bind_values.extend(lvls.iter().cloned());
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        bind_values.push(limit.to_string());
        bind_values.push(offset.to_string());

        let query_builder = bind_values
            .iter()
            .fold(sqlx::query_as::<_, DeviceTrace>(sqlx::AssertSqlSafe(query)), |qb, value| {
                qb.bind(value)
            });

        query_builder
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| Error::IOError(format!("Failed to get traces: {}", e)))
    }

    /// 查询追踪记录统计
    pub async fn get_trace_statistics(
        &self,
        device_id: &str,
        days: u32,
    ) -> Result<DeviceTraceStatistics> {
        let days_param = format!("-{} days", days);

        let total_traces = self.count_traces(device_id, Some(&days_param), None).await.unwrap_or(0);

        let error_traces = self
            .count_traces(device_id, Some(&days_param), Some("error_critical"))
            .await
            .unwrap_or(0);

        let warning_traces =
            self.count_traces(device_id, Some(&days_param), Some("warn")).await.unwrap_or(0);

        let info_traces = total_traces - error_traces - warning_traces;

        let last_trace_time = match sqlx::query_scalar::<_, String>(
            "SELECT created_at FROM device_traces WHERE device_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(device_id)
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(time)) => Some(time),
            _ => None,
        };

        Ok(DeviceTraceStatistics {
            device_id: device_id.to_string(),
            total_traces,
            error_traces,
            warning_traces,
            info_traces,
            days_range: days,
            last_trace_time,
            last_updated: now_string(),
        })
    }

    /// 统计追踪记录数量
    async fn count_traces(
        &self,
        device_id: &str,
        days_param: Option<&str>,
        level_filter: Option<&str>,
    ) -> Result<u32> {
        let sql = match level_filter {
            Some("error_critical") => {
                "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND level IN ('error', 'critical') AND created_at > datetime('now', ?)"
            }
            Some("warn") => {
                "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND level = 'warn' AND created_at > datetime('now', ?)"
            }
            _ => {
                "SELECT COUNT(*) FROM device_traces WHERE device_id = ? AND created_at > datetime('now', ?)"
            }
        };

        let days_str = days_param.unwrap_or("-7 days");

        match sqlx::query_scalar::<_, i64>(sql)
            .bind(device_id)
            .bind(days_str)
            .fetch_optional(self.database.pool())
            .await
        {
            Ok(Some(count)) => Ok(count as u32),
            Ok(None) => Ok(0),
            Err(_) => Ok(0),
        }
    }

    /// 删除追踪记录
    pub async fn delete_traces(
        &self,
        device_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32> {
        let mut query = "DELETE FROM device_traces WHERE device_id = ?".to_string();
        let mut bind_values: Vec<String> = vec![device_id.to_string()];

        if let Some(date) = before_date {
            query.push_str(" AND created_at < ?");
            bind_values.push(date.to_string());
        }

        if let Some(types) = trace_types
            && !types.is_empty()
        {
            let placeholders = types.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND trace_type IN ({})", placeholders));
            bind_values.extend(types.iter().cloned());
        }

        let query_builder = bind_values
            .iter()
            .fold(sqlx::query(sqlx::AssertSqlSafe(query)), |qb, value| qb.bind(value));

        match query_builder.execute(self.database.pool()).await {
            Ok(result) => Ok(result.rows_affected() as u32),
            Err(e) => Err(Error::IOError(format!("Failed to clear traces: {}", e))),
        }
    }

    /// 清理过期追踪记录
    pub async fn cleanup_expired(&self, days_to_keep: u32) -> Result<u32> {
        match sqlx::query("DELETE FROM device_traces WHERE created_at < datetime('now', ?)")
            .bind(format!("-{} days", days_to_keep))
            .execute(self.database.pool())
            .await
        {
            Ok(result) => Ok(result.rows_affected() as u32),
            Err(e) => Err(Error::IOError(format!("Failed to cleanup traces: {}", e))),
        }
    }

    /// 查询所有追踪记录（支持系统级日志查询）
    pub async fn find_all_traces(
        &self,
        levels: Option<&[String]>,
        sources: Option<&[String]>,
        device_id: Option<&str>,
        device_ids: Option<&[String]>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<DeviceTrace>> {
        let mut query = "SELECT id, device_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at FROM device_traces WHERE 1=1".to_string();
        let mut bind_values: Vec<String> = Vec::new();

        if let Some(did) = device_id {
            query.push_str(" AND device_id = ?");
            bind_values.push(did.to_string());
        }

        if let Some(dids) = device_ids
            && !dids.is_empty()
        {
            let placeholders = dids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND device_id IN ({})", placeholders));
            bind_values.extend(dids.iter().cloned());
        }

        if let Some(lvls) = levels
            && !lvls.is_empty()
        {
            let placeholders = lvls.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND level IN ({})", placeholders));
            bind_values.extend(lvls.iter().cloned());
        }

        if let Some(srcs) = sources
            && !srcs.is_empty()
        {
            let placeholders = srcs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            query.push_str(&format!(" AND source IN ({})", placeholders));
            bind_values.extend(srcs.iter().cloned());
        }

        if let Some(start) = start_time {
            query.push_str(" AND created_at >= ?");
            bind_values.push(start.to_string());
        }

        if let Some(end) = end_time {
            query.push_str(" AND created_at <= ?");
            bind_values.push(end.to_string());
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        bind_values.push(limit.to_string());
        bind_values.push(offset.to_string());

        let query_builder = bind_values
            .iter()
            .fold(sqlx::query_as::<_, DeviceTrace>(sqlx::AssertSqlSafe(query)), |qb, value| {
                qb.bind(value)
            });

        query_builder
            .fetch_all(self.database.pool())
            .await
            .map_err(|e| Error::IOError(format!("Failed to get traces: {}", e)))
    }

    /// 获取系统追踪概览
    pub async fn get_system_overview(&self, days: u32) -> SystemTraceOverview {
        let days_param = format!("-{} days", days);

        let total_traces = self.count_all_traces(Some(&days_param)).await.unwrap_or(0);
        let error_traces = self
            .count_all_traces_with_level(Some(&days_param), "error_critical")
            .await
            .unwrap_or(0);
        let warning_traces =
            self.count_all_traces_with_level(Some(&days_param), "warn").await.unwrap_or(0);
        let info_traces = total_traces - error_traces - warning_traces;

        let active_devices = match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT device_id) FROM device_traces WHERE created_at > datetime('now', ?)",
        )
        .bind(&days_param)
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => count as u32,
            _ => 0,
        };

        SystemTraceOverview {
            total_traces,
            error_traces,
            warning_traces,
            info_traces,
            active_devices,
            days_range: days,
            last_updated: now_string(),
        }
    }

    async fn count_all_traces(&self, days_param: Option<&str>) -> Result<u32> {
        let days_str = days_param.unwrap_or("-7 days");
        match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_traces WHERE created_at > datetime('now', ?)",
        )
        .bind(days_str)
        .fetch_optional(self.database.pool())
        .await
        {
            Ok(Some(count)) => Ok(count as u32),
            _ => Ok(0),
        }
    }

    async fn count_all_traces_with_level(
        &self,
        days_param: Option<&str>,
        level_filter: &str,
    ) -> Result<u32> {
        let days_str = days_param.unwrap_or("-7 days");
        let sql = match level_filter {
            "error_critical" => {
                "SELECT COUNT(*) FROM device_traces WHERE level IN ('error', 'critical') AND created_at > datetime('now', ?)"
            }
            "warn" => {
                "SELECT COUNT(*) FROM device_traces WHERE level = 'warn' AND created_at > datetime('now', ?)"
            }
            _ => "SELECT COUNT(*) FROM device_traces WHERE created_at > datetime('now', ?)",
        };

        match sqlx::query_scalar::<_, i64>(sql)
            .bind(days_str)
            .fetch_optional(self.database.pool())
            .await
        {
            Ok(Some(count)) => Ok(count as u32),
            _ => Ok(0),
        }
    }
}
