//! 动态驱动自动加载器

use std::path::Path;
use tracing::{error, info, warn};

use crate::shared::error::Error;

/// 自动加载指定目录下的所有动态驱动
pub fn auto_load_drivers<P: AsRef<Path>>(dir: P) -> Result<Vec<String>, Error> {
    let dir = dir.as_ref();

    if !dir.exists() {
        warn!("Driver directory does not exist: {:?}", dir);
        return Ok(vec![]);
    }

    if !dir.is_dir() {
        return Err(Error::Unsupported(format!(
            "Path is not a directory: {:?}",
            dir
        )));
    }

    info!("Auto-loading drivers from: {:?}", dir);
    let mut loaded_drivers = Vec::new();

    let entries = std::fs::read_dir(dir)
        .map_err(|e| Error::Unsupported(format!("Failed to read driver directory: {}", e)))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();

        // 只加载 .dll (Windows) 或 .so (Linux) 文件
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy();
            if ext_str != "dll" && ext_str != "so" && ext_str != "dylib" {
                continue;
            }
        } else {
            continue;
        }

        info!("Loading driver from: {:?}", path);

        match super::super::load_dynamic_driver(&path) {
            Ok(driver_name) => {
                info!("Successfully loaded driver: {}", driver_name);
                loaded_drivers.push(driver_name);
            }
            Err(e) => {
                error!("Failed to load driver from {:?}: {}", path, e);
            }
        }
    }

    info!("Auto-loaded {} drivers", loaded_drivers.len());
    Ok(loaded_drivers)
}
