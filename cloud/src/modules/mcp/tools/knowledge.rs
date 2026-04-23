// Knowledge Tools Module
// MCP tools for knowledge base querying (Phase 1)

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::modules::mcp::tool_registry::{InputSchema, PropertySchema, ToolError, ToolHandler};

/// 查询知识库
pub struct QueryKnowledgeBaseHandler;

#[async_trait]
impl ToolHandler for QueryKnowledgeBaseHandler {
    fn name(&self) -> &str {
        "query_knowledge_base"
    }

    fn description(&self) -> &str {
        "查询知识库，搜索设备配置、操作手册、故障排除指南"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "query".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("搜索关键词".to_string()),
            },
        );
        props.insert(
            "category".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("知识类别筛选: device, config, manual, troubleshooting".to_string()),
            },
        );
        props.insert(
            "limit".to_string(),
            PropertySchema {
                prop_type: "integer".to_string(),
                description: Some("返回结果数量限制".to_string()),
            },
        );
        InputSchema::object(vec!["query".to_string()], props)
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let category = args.get("category").and_then(|v| v.as_str());

        // Validate category
        if let Some(cat) = category {
            match cat {
                "device" | "config" | "manual" | "troubleshooting" => {}
                _ => return Err(ToolError::InvalidParams(format!(
                    "无效的 category: {}. 允许的值: device, config, manual, troubleshooting",
                    cat
                ))),
            }
        }

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .max(1)
            .min(100) as usize;

        // Phase 1: 返回固定的示例知识条目
        let entries = vec![
            serde_json::json!({
                "id": "kb-001",
                "title": "Modbus 设备配置指南",
                "category": "config",
                "content": "Modbus RTU 设备需要配置正确的波特率和数据位。推荐设置：9600bps, 8N1。",
                "tags": ["modbus", "configuration", "rs485"],
                "relevance": 0.95
            }),
            serde_json::json!({
                "id": "kb-002",
                "title": "ONVIF 摄像头故障排查",
                "category": "troubleshooting",
                "content": "如果 ONVIF 设备发现失败，检查：1) 设备支持 ONVIF 2) 网络连通性 3) 防火墙设置",
                "tags": ["onvif", "camera", "troubleshooting"],
                "relevance": 0.85
            }),
            serde_json::json!({
                "id": "kb-003",
                "title": "SNMP v2c 配置参考",
                "category": "manual",
                "content": "SNMP v2c 使用 community string 'public'。建议在生产环境修改为自定义字符串。",
                "tags": ["snmp", "configuration", "security"],
                "relevance": 0.80
            }),
            serde_json::json!({
                "id": "kb-004",
                "title": "MQTT 设备连接问题",
                "category": "troubleshooting",
                "content": "MQTT 连接失败常见原因：1) Broker 地址错误 2) 端口错误 3) TLS 配置问题 4) 认证信息错误",
                "tags": ["mqtt", "connection", "troubleshooting"],
                "relevance": 0.75
            }),
            serde_json::json!({
                "id": "kb-005",
                "title": "设备心跳监测原理",
                "category": "device",
                "content": "心跳监测通过定期 ping 检测设备在线状态。超时阈值默认 60 秒。",
                "tags": ["heartbeat", "monitoring", "device"],
                "relevance": 0.70
            }),
        ];

        // 简单过滤
        let filtered: Vec<_> = entries
            .into_iter()
            .filter(|e| {
                let title = e["title"].as_str().unwrap_or("").to_lowercase();
                let content = e["content"].as_str().unwrap_or("").to_lowercase();
                let tags = e["tags"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();

                let matches_query = query.is_empty()
                    || title.contains(&query)
                    || content.contains(&query)
                    || tags.iter().any(|t| t.contains(&query));

                let matches_category = category
                    .map(|c| e["category"].as_str().map(|cat| cat == c).unwrap_or(false))
                    .unwrap_or(true);

                matches_query && matches_category
            })
            .take(limit)
            .collect();

        Ok(serde_json::json!({
            "entries": filtered,
            "total": filtered.len(),
            "query": query,
            "category": category
        }))
    }
}

/// 添加知识条目 (Phase 1 stub)
pub struct AddKnowledgeEntryHandler;

#[async_trait]
impl ToolHandler for AddKnowledgeEntryHandler {
    fn name(&self) -> &str {
        "add_knowledge_entry"
    }

    fn description(&self) -> &str {
        "向知识库添加新的知识条目"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "title".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("知识条目标题".to_string()),
            },
        );
        props.insert(
            "content".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("知识条目内容".to_string()),
            },
        );
        props.insert(
            "category".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("知识类别: device, config, manual, troubleshooting".to_string()),
            },
        );
        props.insert(
            "tags".to_string(),
            PropertySchema {
                prop_type: "array".to_string(),
                description: Some("标签数组".to_string()),
            },
        );
        InputSchema::object(vec!["title".to_string(), "content".to_string(), "category".to_string()], props)
    }

    async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::NotImplemented(
            "Phase 2: 知识条目管理".to_string(),
        ))
    }
}

/// 获取设备手册 (Phase 1 stub)
pub struct GetDeviceManualHandler;

#[async_trait]
impl ToolHandler for GetDeviceManualHandler {
    fn name(&self) -> &str {
        "get_device_manual"
    }

    fn description(&self) -> &str {
        "获取指定设备的操作手册"
    }

    fn input_schema(&self) -> InputSchema {
        let mut props = HashMap::new();
        props.insert(
            "device_type".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("设备类型，如 modbus_rtu, onvif_camera, snmp_device".to_string()),
            },
        );
        props.insert(
            "language".to_string(),
            PropertySchema {
                prop_type: "string".to_string(),
                description: Some("手册语言".to_string()),
            },
        );
        InputSchema::object(vec!["device_type".to_string()], props)
    }

    async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
        Err(ToolError::NotImplemented(
            "Phase 2: 设备手册查询".to_string(),
        ))
    }
}

/// 注册所有知识库工具
pub fn register_knowledge_tools(registry: &mut crate::modules::mcp::tool_registry::HandlerRegistry) {
    registry.register(QueryKnowledgeBaseHandler);
    registry.register(AddKnowledgeEntryHandler);
    registry.register(GetDeviceManualHandler);
}