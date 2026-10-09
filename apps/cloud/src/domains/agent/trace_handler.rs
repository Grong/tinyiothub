//! AI 运行透明化 Task 5：GET /agent-runs/{run_id}/trace —— 实时进度轮询出口。
//!
//! ```text
//! GET /agent-runs/{run_id}/trace   workspace 成员鉴权（同 /brain-events 模式）
//! ```
//!
//! 三分支语义：
//! - `live:true`  —— run 在 ActiveRunRegistry（进行中）：读内存 trace。
//!   `read().await` 后立即 clone 并 drop guard，不持 guard 跨 await
//!   （外部声音评审 #9）。
//! - `live:false` —— run 已结束：读 `agent_runs.report`（JSON 列整体 serde
//!   反序列化取 trace，不写 SQL JSON 函数）。
//! - 404 —— 两源均未命中；跨 workspace 命中同样 404（不泄漏存在性）。
//!   report 反序列化失败按 500 + error log 处理。
//!
//! `budgetMax` 两分支同值 = runner 硬预算 [`MAX_TOOL_CALLS_PER_RUN`]。

use axum::{
    Json,
    extract::{Path, State},
    routing::get,
};
use serde::Serialize;

use tinyiothub_agent::runtime::thing_agent::ToolTraceEntry;
use tinyiothub_agent::runtime::thing_agent::runner::MAX_TOOL_CALLS_PER_RUN;
use tinyiothub_web::middleware::workspace::AuthClaims;
use tinyiothub_web::response::{ApiResponse, ApiResponseBuilder};

use crate::domains::agent::AgentState;

/// Trace 端点响应（camelCase）。entries 直接复用核心契约
/// [`ToolTraceEntry`]——字段 id/name/args/output 与线契约逐字一致
/// （output None → null = 未配对 ToolResult）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceResponse {
    pub live: bool,
    pub budget_max: u32,
    pub entries: Vec<ToolTraceEntry>,
}

/// `/agent-runs` 只读路由（G7：agent 域切片态萃取，组合层 nest）。
pub fn create_agent_runs_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    AgentState: axum::extract::FromRef<S>,
{
    axum::Router::new().route("/{run_id}/trace", get(get_run_trace))
}

async fn get_run_trace(
    Path(run_id): Path<String>,
    State(state): State<AgentState>,
    claims: AuthClaims,
) -> Json<ApiResponse<TraceResponse>> {
    let ws = &claims.0.workspace_id;

    // live 分支：注册表命中。跨 workspace 命中按 404 处理——不泄漏 run 存在性。
    if let Some(registry) = &state.active_runs
        && let Some(ctx) = registry.get(&run_id)
    {
        if ctx.workspace_id != *ws {
            return ApiResponseBuilder::error_with_code(404, "run 不存在");
        }
        // 读锁随本语句结束即 drop——clone 后不持 guard 跨 await（评审 #9）。
        let entries = ctx.inner.read().await.trace.clone();
        return ApiResponseBuilder::success(TraceResponse {
            live: true,
            budget_max: MAX_TOOL_CALLS_PER_RUN,
            entries,
        });
    }

    // ended 分支：report JSON 列整体反序列化取 trace。
    match state.db.find_agent_run_report_json(ws, &run_id).await {
        Ok(Some(json)) => match serde_json::from_str::<tinyiothub_core::agent_runs::RunReport>(&json) {
            Ok(report) => ApiResponseBuilder::success(TraceResponse {
                live: false,
                budget_max: MAX_TOOL_CALLS_PER_RUN,
                entries: report.trace,
            }),
            Err(e) => {
                tracing::error!(run_id = %run_id, error = %e, "agent_runs.report 反序列化失败");
                ApiResponseBuilder::error_with_code(500, "run report 数据损坏")
            }
        },
        Ok(None) => ApiResponseBuilder::error_with_code(404, "run 不存在"),
        Err(e) => ApiResponseBuilder::error(format!("查询失败: {e}")),
    }
}
