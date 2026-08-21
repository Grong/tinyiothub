// 5. read_property — current property value from device_cache

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use zeroclaw::tools::{Tool, ToolResult};
use zeroclaw_api::attribution::{Attributable, Role, ToolKind};

use super::tool_ok;
use crate::domains::thing::service::ThingService;

pub struct ReadPropertyTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
    pub(super) device_cache: Option<Arc<tinyiothub_storage::cache::DeviceCache>>,
}

impl Attributable for ReadPropertyTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ReadPropertyTool {
    fn name(&self) -> &str {
        "read_property"
    }

    fn description(&self) -> &str {
        "读取设备上某个属性的当前值和时间戳。\
         当你需要查询设备的最新数据（如温度、湿度、开关状态）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "propertyName": {
                    "type": "string",
                    "description": "属性名称（必需）"
                }
            },
            "required": ["thingId", "propertyName"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            thing_id: String,
            property_name: String,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        // Verify thing exists
        self.thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;

        // Query property definition from DB
        let prop = tinyiothub_storage::Db::new(self.pool.clone())
            .find_thing_property(&input.thing_id, &input.property_name)
            .await
            .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("属性 '{}' 在物 {} 上未找到", input.property_name, input.thing_id))?;

        // Try device_cache for live value; design 六: no cache → null + hint
        let cached = self
            .device_cache
            .as_ref()
            .and_then(|cache| cache.get(&input.thing_id))
            .and_then(|d| {
                let val = d
                    .properties
                    .as_ref()
                    .and_then(|props| props.iter().find(|p| p.name == input.property_name))
                    .and_then(|p| p.current_value.clone());
                let ts = d.last_heartbeat.clone();
                val.map(|v| (v, ts))
            });

        let (current_value, last_heartbeat, hint) = match cached {
            Some((v, ts)) => (json!(v), json!(ts), Value::Null),
            None => (Value::Null, Value::Null, json!("该属性暂无上报数据")),
        };

        tool_ok(json!({
            "thingId": input.thing_id,
            "propertyName": prop.name,
            "displayName": prop.display_name,
            "description": prop.description,
            "dataType": prop.data_type,
            "unit": prop.unit,
            "minValue": prop.min_value,
            "maxValue": prop.max_value,
            "defaultValue": prop.default_value,
            "isReadOnly": prop.is_read_only,
            "currentValue": current_value,
            "lastHeartbeat": last_heartbeat,
            "hint": hint,
        }))
    }
}
