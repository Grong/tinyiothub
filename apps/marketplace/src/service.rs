use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

use crate::cache::SledCache;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("sync failed: {0}")]
    Failed(String),
}

pub struct SyncService {
    cache: Arc<SledCache>,
    data_path: PathBuf,
}

impl SyncService {
    pub fn new(cache: Arc<SledCache>, data_path: PathBuf) -> Self {
        Self { cache, data_path }
    }

    pub async fn load_local_data(&self) -> Result<(), SyncError> {
        info!("Loading local data from {:?}", self.data_path);

        let mut all_templates: Vec<Value> = Vec::new();
        let mut all_drivers: Vec<Value> = Vec::new();

        let templates_dir = self.data_path.join("templates");
        let mut templates_ok = false;
        if templates_dir.is_dir() {
            match tokio::fs::read_dir(&templates_dir).await {
                Ok(mut entries) => {
                    templates_ok = true;
                    while let Some(entry) = entries
                        .next_entry()
                        .await
                        .map_err(|e| SyncError::Failed(e.to_string()))?
                    {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            match tokio::fs::read_to_string(&path).await {
                                Ok(content) => match serde_json::from_str::<Value>(&content) {
                                    // 加载期即校验 schema；不合规文件【不入缓存】——
                                    // handler 无法反序列化它，入缓存只会让 total 虚增、
                                    // 并干扰下游翻页的末页判断
                                    Ok(item) => match serde_json::from_value::<crate::types::Template>(item.clone()) {
                                        Ok(_) => all_templates.push(item),
                                        Err(e) => warn!("Template {:?} failed schema validation, dropped: {}", path, e),
                                    },
                                    Err(e) => warn!("Failed to parse template {:?}: {}", path, e),
                                },
                                Err(e) => warn!("Failed to read template {:?}: {}", path, e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to read templates directory: {}", e),
            }
        }
        // 目录缺失/不可读 = 该资源数据源不可用：不写对应 index（保持缺失/旧值），
        // 让 per-resource 冷语义（503 + X-Cache-Stale + health degraded）如实暴露，
        // 而不是写入一个"健康的空目录"
        if !templates_ok {
            warn!(
                "templates index not written, source dir unavailable: {:?}",
                templates_dir
            );
        }

        let drivers_dir = self.data_path.join("drivers");
        let mut drivers_ok = false;
        if drivers_dir.is_dir() {
            match tokio::fs::read_dir(&drivers_dir).await {
                Ok(mut entries) => {
                    drivers_ok = true;
                    while let Some(entry) = entries
                        .next_entry()
                        .await
                        .map_err(|e| SyncError::Failed(e.to_string()))?
                    {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            match tokio::fs::read_to_string(&path).await {
                                Ok(content) => match serde_json::from_str::<Value>(&content) {
                                    Ok(item) => match serde_json::from_value::<crate::types::Driver>(item.clone()) {
                                        Ok(_) => all_drivers.push(item),
                                        Err(e) => warn!("Driver {:?} failed schema validation, dropped: {}", path, e),
                                    },
                                    Err(e) => warn!("Failed to parse driver {:?}: {}", path, e),
                                },
                                Err(e) => warn!("Failed to read driver {:?}: {}", path, e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Failed to read drivers directory: {}", e),
            }
        }
        if !drivers_ok {
            warn!("drivers index not written, source dir unavailable: {:?}", drivers_dir);
        }

        // 两个数据源都不可用 = 整体配置错误（如 LOCAL_DATA_PATH 指错），响亮失败
        if !templates_ok && !drivers_ok {
            return Err(SyncError::Failed(format!(
                "no data directories available under {:?}",
                self.data_path
            )));
        }

        if templates_ok {
            // 同名去重 + 按名称排序：read_dir 顺序未定义，分页必须建立在稳定顺序上
            dedup_and_sort(&mut all_templates, "name", "template");
            self.cache
                .set_templates(&all_templates)
                .map_err(|e| SyncError::Failed(format!("Failed to write templates to cache: {}", e)))?;
        }
        if drivers_ok {
            dedup_and_sort(&mut all_drivers, "id", "driver");
            self.cache
                .set_drivers(&all_drivers)
                .map_err(|e| SyncError::Failed(format!("Failed to write drivers to cache: {}", e)))?;
        }

        let now = chrono::Utc::now().timestamp();
        self.cache
            .set_last_sync(now)
            .map_err(|e| SyncError::Failed(format!("Failed to update last_sync: {}", e)))?;

        // Batch flush after all writes complete
        self.cache
            .flush()
            .map_err(|e| SyncError::Failed(format!("Failed to flush cache: {}", e)))?;

        info!(
            "Local data load completed: {} templates, {} drivers",
            all_templates.len(),
            all_drivers.len()
        );
        Ok(())
    }
}

/// 按 `key` 字段去重并按名称排序，保证 list 分页顺序稳定、同名文件不产生重复条目。
/// 注意：sort_by 是稳定排序（相等键保留 read_dir 输入序），但 read_dir 顺序本身未定义，
/// 所以同名冲突时保留者是任意一个——同名文件内容应一致；若不一致，以 warn 日志为准排查。
fn dedup_and_sort(items: &mut Vec<Value>, key: &str, kind: &str) {
    items.sort_by(|a, b| {
        let ka = a.get(key).and_then(|v| v.as_str()).unwrap_or("");
        let kb = b.get(key).and_then(|v| v.as_str()).unwrap_or("");
        ka.cmp(kb)
    });
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| {
        let name = item.get(key).and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            warn!(
                "{} entry missing '{}' field, dropped: {:?}",
                kind,
                key,
                item.get("display_name")
            );
            return false;
        }
        if !seen.insert(name.to_string()) {
            warn!(
                "Duplicate {} name '{}', keeping one arbitrary entry (read_dir order is undefined)",
                kind, name
            );
            return false;
        }
        true
    });
}
