//! MCP Server 配置

use anyhow::Result;
use serde::Deserialize;
use std::fs;

/// MCP 配置
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    pub mcp: McpSettings,
    pub tinyiothub: TinyIoTHubSettings,
}

/// MCP 服务器设置
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct McpSettings {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// TinyIoTHub 连接设置
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TinyIoTHubSettings {
    /// API 地址
    pub api_url: String,
    /// API 密钥
    pub api_key: String,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            mcp: McpSettings {
                name: Some("tinyiothub".to_string()),
                version: Some("1.0.0".to_string()),
            },
            tinyiothub: TinyIoTHubSettings {
                api_url: "http://localhost:3002".to_string(),
                api_key: std::env::var("TINYIOTHUB_API_KEY")
                    .unwrap_or_else(|_| String::new()),
            },
        }
    }
}

/// 加载配置文件
pub fn load_config() -> Result<McpConfig> {
    // 1. 首先尝试从环境变量加载
    if let (Ok(api_url), Ok(api_key)) = (
        std::env::var("TINYIOTHUB_API_URL"),
        std::env::var("TINYIOTHUB_API_KEY"),
    ) {
        return Ok(McpConfig {
            mcp: McpSettings {
                name: Some("tinyiothub".to_string()),
                version: Some("1.0.0".to_string()),
            },
            tinyiothub: TinyIoTHubSettings { api_url, api_key },
        });
    }
    
    // 2. 尝试从配置文件加载
    let config_paths = [
        "mcp_settings.toml",
        "mcp/mcp_settings.toml",
        "../mcp_settings.toml",
        "../../mcp_settings.toml",
    ];
    
    for path in &config_paths {
        if let Ok(config) = load_config_from_file(path) {
            return Ok(config);
        }
    }
    
    // 3. 使用默认配置
    Ok(McpConfig::default())
}

/// 从文件加载配置
fn load_config_from_file(path: &str) -> Result<McpConfig> {
    let content = fs::read_to_string(path)?;
    let mut config: McpConfig = toml::from_str(&content)?;
    
    // 从环境变量覆盖配置
    if let Ok(api_url) = std::env::var("TINYIOTHUB_API_URL") {
        config.tinyiothub.api_url = api_url;
    }
    
    if let Ok(api_key) = std::env::var("TINYIOTHUB_API_KEY") {
        config.tinyiothub.api_key = api_key;
    }
    
    Ok(config)
}
