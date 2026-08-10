//! 启动引导逻辑 —— 从 `main.rs` 拆出，main 只做组装（P5-Task25）。
//!
//! 包含：日志初始化、动态驱动重载、设备缓存预热。

use tracing::{info, warn};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::shared::{app_state::AppState, config};

/// Set up global panic handler to prevent crashes
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic message".to_string()
        };

        eprintln!("🚨 PANIC CAUGHT: {} at {}", message, location);
        eprintln!("Application will continue running...");

        // Log to tracing if available
        tracing::error!("PANIC: {} at {}", message, location);
    }));
}

/// Initialize the logging system based on configuration
pub async fn initialize_logging() -> std::io::Result<()> {
    let config = config::get();

    // Declare _guard variable to retain WorkerGuard for main function lifetime
    let _guard;

    // Create log directory if it doesn't exist
    if config.logging.file_enabled
        && let Some(parent) = config.log_file_path().parent()
    {
        std::fs::create_dir_all(parent)?;
    }

    if config.logging.file_enabled {
        info!(
            "File logging enabled (level: {}, path: {:?})",
            config.logging.level,
            config.log_file_path()
        );

        // Console log layer
        let console_layer = fmt::layer().with_ansi(true).with_writer(std::io::stderr);

        // Create rolling file appender
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("app")
            .filename_suffix("log")
            .max_log_files(config.logging.max_files as usize)
            .build(
                config
                    .log_file_path()
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("logs")),
            )
            .unwrap();

        // Create non-blocking writer
        let (non_blocking, guard) = non_blocking(file_appender);
        _guard = guard;

        // File log layer (disable ANSI colors)
        let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);

        // Create filter layer
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&config.logging.level))
            .expect("Cannot initialize log filter");

        // Register global subscriber
        tracing_subscriber::registry()
            .with(console_layer)
            .with(filter_layer)
            .with(file_layer)
            .init();
    } else {
        // Console logging only
        let filter_layer = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(&config.logging.level))
            .expect("Cannot initialize log filter");

        tracing_subscriber::fmt().with_env_filter(filter_layer).init();

        info!("Console logging only (level: {})", config.logging.level);
    }

    Ok(())
}

/// 重新加载已安装的动态驱动
pub async fn rehydrate_drivers(app_state: &AppState) {
    use tinyiothub_storage::DriverInstallationRepo;
    let repo = DriverInstallationRepo::new((*app_state.database).clone());
    match repo.find_all().await {
        Ok(installations) => {
            let registry = tinyiothub_runtime::driver_registry();
            for inst in installations {
                let path = std::path::PathBuf::from(&inst.file_path);
                match registry.write().load(&path, &inst.workspace_id) {
                    Ok(name) => {
                        info!("✅ Rehydrated driver '{}' for workspace {}", name, inst.workspace_id)
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to rehydrate driver {}: {}", inst.driver_name, e)
                    }
                }
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to load driver installations: {}", e);
        }
    }
}

/// 从数据库加载完整设备（含属性、指令）到缓存
pub async fn load_device_cache(app_state: &AppState) {
    use tinyiothub_core::models::device::DeviceQueryParams;
    match app_state
        .device_service
        .get_devices(&DeviceQueryParams::default())
        .await
    {
        Ok(devices) => {
            let device_ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
            let count = device_ids.len();
            match app_state.device_service.load_complete_devices(&device_ids).await {
                Ok(complete_devices) => {
                    for device in complete_devices {
                        app_state.device_cache.insert(device);
                    }
                    info!("✅ Loaded {} complete devices (with properties) into cache", count);
                }
                Err(e) => {
                    warn!("⚠️ Failed to load complete devices, falling back to basic: {}", e);
                    for device in devices {
                        app_state.device_cache.insert(device);
                    }
                }
            }
        }
        Err(e) => {
            warn!("⚠️ Failed to load devices into cache: {}", e);
        }
    }
}
