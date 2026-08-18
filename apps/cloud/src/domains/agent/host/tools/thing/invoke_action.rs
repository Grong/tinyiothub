// 数据实现，留 cloud（D2）
// 6. invoke_action — execute a device action (type='device' guard)

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::{PendingActionStore, store_pending_action, tool_err, tool_ok, validate_action_params};
use crate::domains::thing::service::ThingService;

pub struct InvokeActionTool {
    // pub(crate) so the T11 autonomous variant (tools/autonomous_invoke.rs,
    // O18 thin wrapper) can construct it — no logic change.
    pub(crate) thing_service: Arc<ThingService>,
    pub(crate) pool: SqlitePool,
    pub(crate) workspace_id: String,
    pub(crate) data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub(crate) pending_actions: Arc<PendingActionStore>,
}

impl Attributable for InvokeActionTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for InvokeActionTool {
    fn name(&self) -> &str {
        "invoke_action"
    }

    fn description(&self) -> &str {
        "对物执行操作。仅 thingType='device' 的物支持此操作。\
         如果工作空间启用了 require_action_confirm，则需要确认才能执行。\
         当你需要控制设备（如开关、重启、设置参数）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "actionName": {
                    "type": "string",
                    "description": "操作名称（必需）"
                },
                "params": {
                    "type": "object",
                    "description": "操作参数（可选，JSON 键值对）"
                }
            },
            "required": ["thingId", "actionName"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            action_name: String,
            params: Option<Value>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        // 1. Verify thing exists and check type
        let thing = self
            .thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;

        if thing.thing_type != "device" {
            return tool_err(format!(
                "操作不支持: 物类型为 '{}'，仅 'device' 类型物支持 invoke_action。\
                 对于 space/line/building 类型，请使用其属性或知识库。",
                thing.thing_type
            ));
        }

        // 2. Check require_action_confirm from workspace
        let require_confirm: bool = sqlx::query_scalar("SELECT require_action_confirm FROM workspaces WHERE id = ?")
                .bind(&self.workspace_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
                // Fail CLOSED (eng-review T7): a missing workspace row means
                // something is wrong — require confirmation (design default ON)
                .unwrap_or(1i32)
            != 0;

        // 3. Check if action exists in device_commands table
        let command_exists: bool =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM thing_actions WHERE device_id = ? AND name = ?")
                .bind(&input.thing_id)
                .bind(&input.action_name)
                .fetch_one(&self.pool)
                .await
                .map(|c| c > 0)
                .unwrap_or(false);

        if !command_exists {
            return tool_err(format!(
                "操作 '{}' 未在物 {} 上注册。请检查可用的操作列表。",
                input.action_name, input.thing_id
            ));
        }

        // 3b. Validate params against the action's parameter schema (design 三;
        // eng-review T7)
        let params_schema: Option<String> =
            sqlx::query_scalar("SELECT parameters FROM thing_actions WHERE device_id = ? AND name = ?")
                .bind(&input.thing_id)
                .bind(&input.action_name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
                .flatten();
        if let Some(ref schema_json) = params_schema
            && let Err(msg) = validate_action_params(schema_json, input.params.as_ref())
        {
            return tool_err(msg);
        }

        // 4. Execute or request confirmation
        if require_confirm {
            let token = store_pending_action(
                &self.pending_actions,
                input.thing_id.clone(),
                input.action_name.clone(),
                input.params.clone(),
                self.workspace_id.clone(),
            );
            return tool_ok(json!({
                "thingId": input.thing_id,
                "actionName": input.action_name,
                "status": "confirmation_required",
                "token": token,
                "message": "该操作需要用户确认后才能执行。请使用确认接口提交 token。",
                "requireConfirm": true
            }));
        }

        // Execute via DataServer if available.
        // NOTE: this dispatch tail (through the closing brace of this match)
        // is mirrored by `dispatch_command` in autonomous_invoke.rs — if you
        // change it here, keep the mirror in sync.
        match self.data_server.clone() {
            Some(data_server) => {
                let cmd = tinyiothub_core::models::device_command::DeviceCommand {
                    id: uuid::Uuid::new_v4().to_string(),
                    device_id: input.thing_id.clone(),
                    name: input.action_name.clone(),
                    display_name: None,
                    description: None,
                    parameters: input.params.as_ref().map(|p| p.to_string()),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };
                match data_server.execute_command(cmd) {
                    Ok(()) => tool_ok(json!({
                        "thingId": input.thing_id,
                        "actionName": input.action_name,
                        "status": "executed",
                        "message": "操作已下发执行"
                    })),
                    Err(e) => tool_err(format!("操作执行失败: {}", e)),
                }
            }
            None => {
                tracing::warn!("DataServer not available, action execution is simulated");
                tool_ok(json!({
                    "thingId": input.thing_id,
                    "actionName": input.action_name,
                    "status": "simulated",
                    "message": "操作已记录（DataServer 未就绪，实际执行已模拟）"
                }))
            }
        }
    }
}
