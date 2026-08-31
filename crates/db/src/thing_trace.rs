//! Thing trace 持久化：thing_traces 表
//!（自 cloud domains/thing/legacy/trace_repository.rs 迁入，Task 12）。
//!
//! 类型随 repo 住 db：ThingTrace/ThingTraceStatistics/SystemTraceOverview，
//! cloud 侧 legacy::trace 模块直接引用本模块路径。

use sqlx::SqlitePool;
use tinyiothub_core::{
    error::{Error, Result},
    now_string,
};

use crate::database::Db;

// ──────────────────────────────────────────────
// 持久化类型 — 自 cloud legacy/trace.rs 迁入
// ──────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct ThingTrace {
    pub id: String,
    pub thing_id: String,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThingTraceStatistics {
    pub thing_id: String,
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub days_range: u32,
    pub last_trace_time: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SystemTraceOverview {
    pub total_traces: u32,
    pub error_traces: u32,
    pub warning_traces: u32,
    pub info_traces: u32,
    pub active_things: u32,
    pub days_range: u32,
    pub last_updated: String,
}

// ──────────────────────────────────────────────
// 持久化函数（SQLite）
// ──────────────────────────────────────────────

/// 插入追踪记录
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_thing_trace(
    pool: &SqlitePool,
    trace_id: &str,
    thing_id: &str,
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
        "INSERT INTO thing_traces (id, thing_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))"
    )
    .bind(trace_id)
    .bind(thing_id)
    .bind(trace_type)
    .bind(level)
    .bind(category)
    .bind(title)
    .bind(message)
    .bind(details_json)
    .bind(source)
    .bind(user_id)
    .bind(session_id)
    .execute(pool)
    .await
    .map_err(|e| Error::IOError(format!("Failed to record trace: {}", e)))?;

    Ok(())
}

/// 查询设备追踪记录（支持过滤和分页）
pub(crate) async fn find_thing_traces(
    pool: &SqlitePool,
    thing_id: &str,
    trace_types: Option<&[String]>,
    levels: Option<&[String]>,
    limit: u32,
    offset: u32,
) -> Result<Vec<ThingTrace>> {
    let mut query = "SELECT id, thing_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at FROM thing_traces WHERE thing_id = ?".to_string();
    let mut bind_values: Vec<String> = vec![thing_id.to_string()];

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

    let query_builder = bind_values.iter().fold(
        sqlx::query_as::<_, ThingTrace>(sqlx::AssertSqlSafe(query)),
        |qb, value| qb.bind(value),
    );

    query_builder
        .fetch_all(pool)
        .await
        .map_err(|e| Error::IOError(format!("Failed to get traces: {}", e)))
}

/// 查询追踪记录统计
pub(crate) async fn get_thing_trace_statistics(
    pool: &SqlitePool,
    thing_id: &str,
    days: u32,
) -> Result<ThingTraceStatistics> {
    let days_param = format!("-{} days", days);

    let total_traces = count_thing_traces(pool, thing_id, Some(&days_param), None)
        .await
        .unwrap_or(0);

    let error_traces = count_thing_traces(pool, thing_id, Some(&days_param), Some("error_critical"))
        .await
        .unwrap_or(0);

    let warning_traces = count_thing_traces(pool, thing_id, Some(&days_param), Some("warn"))
        .await
        .unwrap_or(0);

    let info_traces = total_traces - error_traces - warning_traces;

    let last_trace_time = match sqlx::query_scalar::<_, String>(
        "SELECT created_at FROM thing_traces WHERE thing_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(thing_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(time)) => Some(time),
        _ => None,
    };

    Ok(ThingTraceStatistics {
        thing_id: thing_id.to_string(),
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
async fn count_thing_traces(
    pool: &SqlitePool,
    thing_id: &str,
    days_param: Option<&str>,
    level_filter: Option<&str>,
) -> Result<u32> {
    let sql = match level_filter {
        Some("error_critical") => {
            "SELECT COUNT(*) FROM thing_traces WHERE thing_id = ? AND level IN ('error', 'critical') AND created_at > datetime('now', ?)"
        }
        Some("warn") => {
            "SELECT COUNT(*) FROM thing_traces WHERE thing_id = ? AND level = 'warn' AND created_at > datetime('now', ?)"
        }
        _ => "SELECT COUNT(*) FROM thing_traces WHERE thing_id = ? AND created_at > datetime('now', ?)",
    };

    let days_str = days_param.unwrap_or("-7 days");

    match sqlx::query_scalar::<_, i64>(sql)
        .bind(thing_id)
        .bind(days_str)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(count)) => Ok(count as u32),
        Ok(None) => Ok(0),
        Err(_) => Ok(0),
    }
}

/// 删除追踪记录
pub(crate) async fn delete_thing_traces(
    pool: &SqlitePool,
    thing_id: &str,
    before_date: Option<&str>,
    trace_types: Option<&[String]>,
) -> Result<u32> {
    let mut query = "DELETE FROM thing_traces WHERE thing_id = ?".to_string();
    let mut bind_values: Vec<String> = vec![thing_id.to_string()];

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

    match query_builder.execute(pool).await {
        Ok(result) => Ok(result.rows_affected() as u32),
        Err(e) => Err(Error::IOError(format!("Failed to clear traces: {}", e))),
    }
}

/// 清理过期追踪记录
pub(crate) async fn cleanup_expired_thing_traces(pool: &SqlitePool, days_to_keep: u32) -> Result<u32> {
    match sqlx::query("DELETE FROM thing_traces WHERE created_at < datetime('now', ?)")
        .bind(format!("-{} days", days_to_keep))
        .execute(pool)
        .await
    {
        Ok(result) => Ok(result.rows_affected() as u32),
        Err(e) => Err(Error::IOError(format!("Failed to cleanup traces: {}", e))),
    }
}

/// 查询所有追踪记录（支持系统级日志查询）
#[allow(clippy::too_many_arguments)]
pub(crate) async fn find_all_thing_traces(
    pool: &SqlitePool,
    levels: Option<&[String]>,
    sources: Option<&[String]>,
    thing_id: Option<&str>,
    thing_ids: Option<&[String]>,
    start_time: Option<&str>,
    end_time: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<ThingTrace>> {
    let mut query = "SELECT id, thing_id, trace_type, level, category, title, message, details, source, user_id, session_id, created_at FROM thing_traces WHERE 1=1".to_string();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(did) = thing_id {
        query.push_str(" AND thing_id = ?");
        bind_values.push(did.to_string());
    }

    if let Some(dids) = thing_ids
        && !dids.is_empty()
    {
        let placeholders = dids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        query.push_str(&format!(" AND thing_id IN ({})", placeholders));
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

    let query_builder = bind_values.iter().fold(
        sqlx::query_as::<_, ThingTrace>(sqlx::AssertSqlSafe(query)),
        |qb, value| qb.bind(value),
    );

    query_builder
        .fetch_all(pool)
        .await
        .map_err(|e| Error::IOError(format!("Failed to get traces: {}", e)))
}

/// 获取系统追踪概览
pub(crate) async fn get_thing_trace_system_overview(pool: &SqlitePool, days: u32) -> SystemTraceOverview {
    let days_param = format!("-{} days", days);

    let total_traces = count_all_thing_traces(pool, Some(&days_param)).await.unwrap_or(0);
    let error_traces = count_all_thing_traces_with_level(pool, Some(&days_param), "error_critical")
        .await
        .unwrap_or(0);
    let warning_traces = count_all_thing_traces_with_level(pool, Some(&days_param), "warn")
        .await
        .unwrap_or(0);
    let info_traces = total_traces - error_traces - warning_traces;

    let active_things = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT thing_id) FROM thing_traces WHERE created_at > datetime('now', ?)",
    )
    .bind(&days_param)
    .fetch_optional(pool)
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
        active_things,
        days_range: days,
        last_updated: now_string(),
    }
}

async fn count_all_thing_traces(pool: &SqlitePool, days_param: Option<&str>) -> Result<u32> {
    let days_str = days_param.unwrap_or("-7 days");
    match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_traces WHERE created_at > datetime('now', ?)")
        .bind(days_str)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(count)) => Ok(count as u32),
        _ => Ok(0),
    }
}

async fn count_all_thing_traces_with_level(
    pool: &SqlitePool,
    days_param: Option<&str>,
    level_filter: &str,
) -> Result<u32> {
    let days_str = days_param.unwrap_or("-7 days");
    let sql = match level_filter {
        "error_critical" => {
            "SELECT COUNT(*) FROM thing_traces WHERE level IN ('error', 'critical') AND created_at > datetime('now', ?)"
        }
        "warn" => "SELECT COUNT(*) FROM thing_traces WHERE level = 'warn' AND created_at > datetime('now', ?)",
        _ => "SELECT COUNT(*) FROM thing_traces WHERE created_at > datetime('now', ?)",
    };

    match sqlx::query_scalar::<_, i64>(sql)
        .bind(days_str)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(count)) => Ok(count as u32),
        _ => Ok(0),
    }
}

// ──────────────────────────────────────────────
// Db 委托
// ──────────────────────────────────────────────

impl Db {
    /// 插入追踪记录。
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_thing_trace(
        &self,
        trace_id: &str,
        thing_id: &str,
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
        insert_thing_trace(
            self.pool(),
            trace_id,
            thing_id,
            trace_type,
            level,
            category,
            title,
            message,
            details_json,
            source,
            user_id,
            session_id,
        )
        .await
    }

    /// 查询设备追踪记录（过滤 + 分页）。
    pub async fn find_thing_traces(
        &self,
        thing_id: &str,
        trace_types: Option<&[String]>,
        levels: Option<&[String]>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ThingTrace>> {
        find_thing_traces(self.pool(), thing_id, trace_types, levels, limit, offset).await
    }

    /// 查询追踪记录统计。
    pub async fn get_thing_trace_statistics(&self, thing_id: &str, days: u32) -> Result<ThingTraceStatistics> {
        get_thing_trace_statistics(self.pool(), thing_id, days).await
    }

    /// 删除追踪记录。
    pub async fn delete_thing_traces(
        &self,
        thing_id: &str,
        before_date: Option<&str>,
        trace_types: Option<&[String]>,
    ) -> Result<u32> {
        delete_thing_traces(self.pool(), thing_id, before_date, trace_types).await
    }

    /// 清理过期追踪记录。
    pub async fn cleanup_expired_thing_traces(&self, days_to_keep: u32) -> Result<u32> {
        cleanup_expired_thing_traces(self.pool(), days_to_keep).await
    }

    /// 系统级追踪查询。
    #[allow(clippy::too_many_arguments)]
    pub async fn find_all_thing_traces(
        &self,
        levels: Option<&[String]>,
        sources: Option<&[String]>,
        thing_id: Option<&str>,
        thing_ids: Option<&[String]>,
        start_time: Option<&str>,
        end_time: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ThingTrace>> {
        find_all_thing_traces(
            self.pool(),
            levels,
            sources,
            thing_id,
            thing_ids,
            start_time,
            end_time,
            limit,
            offset,
        )
        .await
    }

    /// 系统追踪概览。
    pub async fn get_thing_trace_system_overview(&self, days: u32) -> SystemTraceOverview {
        get_thing_trace_system_overview(self.pool(), days).await
    }
}
