use crate::dto::entity::device_command::{find_device_command_by_id, DeviceCommand};
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router
};
use serde::{Deserialize, Serialize};

use crate::{
    dto::{
        response::{builder::ApiResponseBuilder, ApiResponse}
    },
    shared::{app_state::AppState, security::jwt::Claims}
};

#[derive(Debug, Deserialize)]
pub struct ExecuteCommandRequest {
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct CommandExecution {
    pub id: String,
    pub device_id: String,
    pub command_id: String,
    pub command_name: String,
    pub parameters: Option<serde_json::Value>,
    pub status: String, // "pending", "executing", "success", "failed"
    pub result: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub executed_at: String,
    pub completed_at: Option<String>,
}

pub fn create_router() -> Router<AppState> {
    Router::new().route("/{device_id}/commands/{command_id}/execute", post(execute_device_command))
}

/// 执行设备指令
async fn execute_device_command(
    State(state): State<AppState>,
    Path((device_id, command_id)): Path<(String, String)>,
    claims: Claims,
    Json(req): Json<ExecuteCommandRequest>,
) -> Json<ApiResponse<CommandExecution>> {
    tracing::info!(
        "Executing command {} for device {} with parameters: {:?}",
        command_id,
        device_id,
        req.parameters
    );

    // 验证设备存在且属于当前租户
    if let Err(e) = super::verify_device_tenant(&state, &device_id, &claims.tenant_id).await {
        return match e {
            crate::shared::error::Error::NotFound => ApiResponseBuilder::error("设备不存在"),
            _ => ApiResponseBuilder::error("查询设备失败")
};
    }

    // 验证指令是否存在
    let command = match find_device_command_by_id(state.database(), &command_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return ApiResponseBuilder::error("指令不存在"),
        Err(e) => {
            tracing::error!("Failed to find command {}: {}", command_id, e);
            return ApiResponseBuilder::error("查询指令失败");
        }
    };

    // 验证指令是否属于该设备
    if command.device_id != device_id {
        return ApiResponseBuilder::error("指令不属于该设备");
    }

    // 创建指令执行记录
    let execution_id = uuid::Uuid::new_v4().to_string();
    let executed_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // 保存参数
    let params_for_execution = req.parameters.clone();

    // 构建设备命令对象用于执行
    let mut device_command = command.clone();
    if let Some(params) = params_for_execution.clone() {
        device_command.parameters = Some(params.to_string());
    }

    // 通过 DataServer 执行命令
    let execution_status = if let Some(data_server) = state.data_server() {
        match data_server.execute_command(device_command) {
            Ok(_) => {
                tracing::info!(
                    "Command queued for execution: device={}, command={}",
                    device_id,
                    command_id
                );
                "pending" // 命令已加入队列，等待执行
            }
            Err(e) => {
                tracing::error!(
                    "Failed to queue command: device={}, command={}, error={}",
                    device_id,
                    command_id,
                    e
                );
                "failed" // 命令提交失败
            }
        }
    } else {
        tracing::warn!("DataServer not available, command cannot be executed");
        "unavailable" // DataServer 不可用
    };

    // 命令提交成功，返回执行记录
    let execution = CommandExecution {
        id: execution_id.clone(),
        device_id: device_id.clone(),
        command_id: command_id.clone(),
        command_name: command.name.clone(),
        parameters: params_for_execution.clone(),
        status: execution_status.to_string(),
        result: None,
        error_message: if execution_status == "failed" || execution_status == "unavailable" {
            Some(format!("Command submission failed: {}", execution_status))
        } else {
            None
        },
        executed_at: executed_at.clone(),
        completed_at: None
};

    tracing::info!(
        "Command submitted: device={}, command={}, execution_id={}",
        device_id,
        command_id,
        execution.id
    );

    ApiResponseBuilder::success(execution)
}
