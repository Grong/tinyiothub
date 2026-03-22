//! 用户相关 MCP Tools 定义

use crate::tools::ToolMeta;

/// get_current_user - 获取当前用户信息
pub fn get_current_user() -> ToolMeta {
    ToolMeta {
        name: "get_current_user".to_string(),
        description: "获取当前登录用户的信息，包括用户ID、名称、角色和权限".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "include_permissions": {
                    "type": "boolean",
                    "description": "是否包含用户权限列表",
                    "default": true
                },
                "include_roles": {
                    "type": "boolean",
                    "description": "是否包含用户角色列表",
                    "default": true
                }
            }
        }),
    }
}
