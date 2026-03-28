//! 统一插件注册表
//!
//! 所有插件（协议驱动、通知渠道、定时任务、存储后端、集成）都通过此注册表管理。

use std::{
    any::Any,
    path::{Path, PathBuf},
    sync::Arc,
};

use dashmap::DashMap;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::shared::error::Error;

/// 插件类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Protocol,
    Notification,
    Scheduler,
    Storage,
    Integration,
}

impl PluginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Notification => "notification",
            Self::Scheduler => "scheduler",
            Self::Storage => "storage",
            Self::Integration => "integration",
        }
    }
}

/// 插件清单（TOML 文件的 [plugin] 部分）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: Option<String>,
    #[serde(rename = "type")]
    pub plugin_type: PluginType,
    pub description: Option<String>,
}

/// 插件条目
pub enum PluginEntry {
    /// 静态插件（编译时注册）
    Static {
        manifest: PluginManifest,
        factory: PluginFactory,
    },
    /// 动态插件（.dll/.so 路径）
    Dynamic {
        manifest: PluginManifest,
        path: PathBuf,
    },
    /// 配置插件（TOML 配置）
    Config {
        manifest: PluginManifest,
        config: toml::Value,
    },
}

/// 插件工厂函数
pub type PluginFactory =
    Box<dyn Fn(Arc<crate::application::AppContext>) -> Result<Box<dyn PluginHandler>, Error> + Send + Sync>;

/// 插件处理器接口（所有插件的共同接口）
pub trait PluginHandler: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn manifest(&self) -> &PluginManifest;
    fn plugin_type(&self) -> PluginType;
}

/// 应用上下文别名（所有插件共享）
pub use crate::application::AppContext;

/// 统一插件注册表
pub struct PluginRegistry {
    entries: DashMap<String, PluginEntry>,
    handlers: DashMap<String, Arc<dyn PluginHandler>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            handlers: DashMap::new(),
        }
    }

    /// 注册静态插件
    pub fn register_static(&self, manifest: PluginManifest, factory: PluginFactory) {
        debug!("Registering static plugin: {} ({})", manifest.name, manifest.plugin_type.as_str());
        self.entries.insert(manifest.name.clone(), PluginEntry::Static { manifest, factory });
    }

    /// 注册配置插件
    pub fn register_config(&self, manifest: PluginManifest, config: toml::Value) {
        info!("Registering config plugin: {} ({})", manifest.name, manifest.plugin_type.as_str());
        self.entries.insert(manifest.name.clone(), PluginEntry::Config { manifest, config });
    }

    /// 创建插件实例
    pub fn create_plugin(&self, name: &str, context: Arc<AppContext>) -> Result<Arc<dyn PluginHandler>, Error> {
        if let Some(handler) = self.handlers.get(name) {
            return Ok(handler.value().clone());
        }

        let entry = self.entries.get(name)
            .ok_or_else(|| Error::Unsupported(format!("Plugin not found: {}", name)))?;

        let handler: Box<dyn PluginHandler> = match entry.value() {
            PluginEntry::Static { manifest: _, factory } => {
                debug!("Creating static plugin: {}", name);
                factory(context)?
            }
            PluginEntry::Dynamic { manifest: _, path } => {
                debug!("Creating dynamic plugin: {} from {:?}", name, path);
                self.create_dynamic_plugin(name, path)?
            }
            PluginEntry::Config { manifest, config } => {
                debug!("Creating config plugin: {}", name);
                self.create_config_plugin(manifest, config, context)?
            }
        };

        let arc_handler: Arc<dyn PluginHandler> = Arc::from(handler);
        self.handlers.insert(name.to_string(), arc_handler.clone());
        Ok(arc_handler)
    }

    fn create_dynamic_plugin(&self, name: &str, path: &Path) -> Result<Box<dyn PluginHandler>, Error> {
        Err(Error::Unsupported(format!("Dynamic plugin loading not implemented yet: {}", name)))
    }

    fn create_config_plugin(
        &self,
        manifest: &PluginManifest,
        config: &toml::Value,
        context: Arc<AppContext>,
    ) -> Result<Box<dyn PluginHandler>, Error> {
        match manifest.plugin_type {
            PluginType::Protocol => {
                let handler = crate::domain::plugin::protocol::create_handler(config)?;
                Ok(handler)
            }
            PluginType::Notification => {
                let handler = crate::domain::plugin::notification::create_handler(config, context)?;
                Ok(handler)
            }
            PluginType::Scheduler => {
                let handler = crate::domain::plugin::scheduler::create_handler(config)?;
                Ok(handler)
            }
            PluginType::Storage => {
                let handler = crate::domain::plugin::storage::create_handler(config, context)?;
                Ok(handler)
            }
            PluginType::Integration => {
                let handler = crate::domain::plugin::integration::create_handler(config, context)?;
                Ok(handler)
            }
        }
    }

    pub fn has_plugin(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn plugin_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }

    pub fn plugins_by_type(&self, plugin_type: PluginType) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| match e.value() {
                PluginEntry::Static { manifest, .. } => manifest.plugin_type == plugin_type,
                PluginEntry::Dynamic { manifest, .. } => manifest.plugin_type == plugin_type,
                PluginEntry::Config { manifest, .. } => manifest.plugin_type == plugin_type,
            })
            .map(|e| e.key().clone())
            .collect()
    }

    pub fn load_from_dir<P: AsRef<Path>>(&self, dir: P) -> Result<Vec<String>, Error> {
        let dir = dir.as_ref();
        if !dir.exists() {
            warn!("Plugin directory does not exist: {:?}", dir);
            return Ok(vec![]);
        }

        let mut loaded = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if ext_str == "toml" {
                    if let Ok(name) = self.load_toml_plugin(&path) {
                        loaded.push(name);
                    }
                }
            }
        }

        info!("Auto-loaded {} plugins from {:?}", loaded.len(), dir);
        Ok(loaded)
    }

    fn load_toml_plugin(&self, path: &Path) -> Result<String, Error> {
        let content = std::fs::read_to_string(path)?;
        let value: toml::Value = toml::from_str(&content)
            .map_err(|e| Error::ValidationError(format!("Invalid TOML: {}", e)))?;

        let manifest: PluginManifest = value.get("plugin")
            .ok_or_else(|| Error::ValidationError("Missing [plugin] section".to_string()))?
            .clone()
            .try_into()
            .map_err(|e| Error::ValidationError(format!("Invalid plugin manifest: {}", e)))?;

        let name = manifest.name.clone();
        self.register_config(manifest, value);
        Ok(name)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self { Self::new() }
}

static GLOBAL_REGISTRY: once_cell::sync::Lazy<PluginRegistry> =
    once_cell::sync::Lazy::new(PluginRegistry::new);

pub fn get_global_registry() -> &'static PluginRegistry {
    &GLOBAL_REGISTRY
}
