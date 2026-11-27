// 5. read_property — current property value from device_cache

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_runtime::access::DemandRecorder;

use super::tool_ok;
use crate::domains::thing::service::ThingService;
use crate::domains::thing::types::{ThingProperties, ThingPropertyResponse};

pub struct ReadPropertyTool {
    pub(super) thing_service: Arc<ThingService>,
    pub(super) workspace_id: String,
    pub(super) device_cache: Option<Arc<tinyiothub_storage::cache::ThingCache>>,
    /// E4b：成功读取后写入 DemandTracker（None = 未接线）。
    pub(super) demand: Option<Arc<dyn DemandRecorder>>,
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
        "读取设备上一组属性的完整定义和当前值。\
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
                "properties": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "属性名称列表（必需）"
                }
            },
            "required": ["thingId", "properties"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum PropertyNames {
            One(String),
            Many(Vec<String>),
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "thing_id")]
            thing_id: String,
            #[serde(alias = "property_name", alias = "propertyName")]
            properties: PropertyNames,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;
        let properties = match input.properties {
            PropertyNames::One(name) => vec![name],
            PropertyNames::Many(names) => names,
        };
        if properties.is_empty() {
            return Err(anyhow::anyhow!("properties 不能为空"));
        }

        let mut thing = self
            .thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;
        thing.knowledge_docs = None;

        let definitions = self.thing_service.load_property_models(&input.thing_id).await;
        let live = self.device_cache.as_ref().and_then(|cache| cache.get(&input.thing_id));
        let mut response = Vec::with_capacity(properties.len());
        for name in &properties {
            let definition = definitions
                .iter()
                .find(|property| &property.name == name)
                .ok_or_else(|| anyhow::anyhow!("属性 '{}' 在物 {} 上未找到", name, input.thing_id))?;
            let mut property = ThingPropertyResponse::from(definition.clone());
            property.current_value = live
                .as_ref()
                .and_then(|device| device.properties.as_ref())
                .and_then(|properties| {
                    properties.iter().find(|property| {
                        &property.name == name || property.display_name.as_deref() == Some(name.as_str())
                    })
                })
                .and_then(|property| property.current_value.clone());
            if let Some(demand) = &self.demand {
                demand.record_read(&self.workspace_id, &input.thing_id, name);
            }
            response.push(property);
        }

        tool_ok(serde_json::to_value(ThingProperties {
            thing,
            properties: response,
        })?)
    }
}
