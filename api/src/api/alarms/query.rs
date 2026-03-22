use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};

use crate::{
    domain::alarm::{AlarmLevel, AlarmQueryCriteria, AlarmStatus, TimeRange},
    dto::{
        entity::{AlarmDto, AlarmStatisticsDto},
        request::{AlarmQueryParams, StatisticsQueryParams},
        response::{api_response::ApiResponse, builder::ApiResponseBuilder},
    },
    shared::{app_state::AppState, error_handling::ErrorCode, security::jwt::Claims},
};

#[derive(serde::Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(serde::Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub total_count: u64,
}

/// 查询报警列表
pub async fn list_alarms(
    Query(params): Query<AlarmQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<PaginatedResponse<AlarmDto>>> {
    // 构建查询条件
    let time_range = if params.start_time.is_some() || params.end_time.is_some() {
        let start = params
            .start_time
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - chrono::Duration::days(30));

        let end = params
            .end_time
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Some(TimeRange { start, end })
    } else {
        None
    };

    let alarm_levels = params.levels.as_ref().and_then(|levels| {
        let parsed: Vec<AlarmLevel> =
            levels.iter().filter_map(|l| AlarmLevel::from_str(l)).collect();
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    });

    let statuses = params.statuses.as_ref().and_then(|statuses| {
        let parsed: Vec<AlarmStatus> =
            statuses.iter().filter_map(|s| AlarmStatus::from_str(s)).collect();
        if parsed.is_empty() {
            None
        } else {
            Some(parsed)
        }
    });

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20);
    let offset = (page - 1) * page_size;

    let criteria = AlarmQueryCriteria {
        device_ids: params.device_ids,
        property_ids: None,
        alarm_levels,
        alarm_types: None,
        statuses,
        time_range,
        sort_by: Some("alarm_time".to_string()),
        sort_order: Some(crate::domain::alarm::SortOrder::Desc),
        limit: Some(page_size),
        offset: Some(offset),
    };

    // 查询报警
    match state.alarm_service.get_alarm_history(criteria.clone()).await {
        Ok(alarms) => {
            let total = state.alarm_service.count_alarms(criteria).await.unwrap_or(0);
            let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;

            let data: Vec<AlarmDto> = alarms.into_iter().map(AlarmDto::from).collect();

            ApiResponseBuilder::success(PaginatedResponse {
                data,
                pagination: PaginationInfo { page, page_size, total_pages, total_count: total },
            })
        }
        Err(e) => ApiResponseBuilder::error(format!("查询报警失败: {}", e)),
    }
}

/// 获取报警详情
pub async fn get_alarm(
    Path(id): Path<String>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmDto>> {
    match state.alarm_service.get_alarm_by_id(&id).await {
        Ok(Some(alarm)) => ApiResponseBuilder::success(AlarmDto::from(alarm)),
        Ok(None) => ApiResponseBuilder::error_with_code(ErrorCode::NotFound.as_i32(), "报警不存在"),
        Err(e) => ApiResponseBuilder::error(format!("获取报警失败: {}", e)),
    }
}

/// 获取报警统计
pub async fn get_alarm_statistics(
    Query(params): Query<StatisticsQueryParams>,
    State(state): State<AppState>,
    _claims: Claims,
) -> Json<ApiResponse<AlarmStatisticsDto>> {
    let start = params
        .start_time
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));

    let end = params
        .end_time
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let time_range = TimeRange { start, end };

    match state.alarm_service.get_alarm_statistics(time_range).await {
        Ok(stats) => ApiResponseBuilder::success(AlarmStatisticsDto::from(stats)),
        Err(e) => ApiResponseBuilder::error(format!("获取统计失败: {}", e)),
    }
}
