// 禁用开发阶段的常见警告
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]

use axum::Router;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

use crate::application::{DataContext, ServiceManager};
use crate::infrastructure::config;

use tracing::{error, info};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod api;
mod application;
mod domain; // 恢复域模块
mod dto;
mod infrastructure;
mod shared;
mod utils;

#[cfg(feature = "harmonyos")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    main_impl().await
}

#[cfg(not(feature = "harmonyos"))]
#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() -> std::io::Result<()> {
    main_impl().await
}

async fn main_impl() -> std::io::Result<()> {
    // Set up global panic handler to prevent crashes
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

    // === 1. 初始化配置系统 ===
    if let Err(e) = config::initialize() {
        eprintln!("Failed to initialize configuration: {}", e);
        std::process::exit(1);
    }

    // Set default log level if not specified
    if std::env::var_os("RUST_LOG").is_none() {
        let log_level = config::get().logging.level.clone();
        std::env::set_var("RUST_LOG", log_level);
    }

    // Initialize logging system
    initialize_logging().await?;

    info!("🚀 TinyIoTHub Starting...");
    info!("Environment: {}", config::environment());
    info!("Server: {}", config::get().server_bind_address());
    info!("Database: {}", config::get().database.url);
    info!("MQTT: {}", config::get().mqtt_broker_address());
    info!("CPUs: {}", num_cpus::get());

    // === 2. 初始化数据库 ===
    use crate::infrastructure::persistence::DatabaseConfig;
    let db_config = DatabaseConfig::from_settings(config::get());
    let data_context = DataContext::new(db_config)
        .await
        .expect("Failed to initialize DataContext");
    info!("✅ DataContext initialized");

    // === 3. 创建 AppState（包含所有核心组件）===
    let mut app_state = crate::shared::app_state::AppState::new(data_context);
    info!("✅ AppState created");

    // === 4. 自动加载动态驱动 ===
    if config::get().device.drivers.auto_load_on_startup {
        let drivers_dir = &config::get().device.drivers.dynamic_drivers_dir;
        info!("🔌 Auto-loading drivers from: {}", drivers_dir);
        match crate::domain::device::driver::dynamic::auto_load_drivers(drivers_dir) {
            Ok(loaded) => {
                if loaded.is_empty() {
                    info!("No drivers found in directory");
                } else {
                    info!("✅ Loaded {} driver(s): {:?}", loaded.len(), loaded);
                }
            }
            Err(e) => {
                error!("Failed to auto-load drivers: {}", e);
            }
        }
    }

    // === 5. 启动后台服务 ===
    let mut service_manager = ServiceManager::new();
    if let Err(e) = service_manager.start_all(&mut app_state).await {
        error!("❌ Failed to start background services: {}", e);
        std::process::exit(1);
    }
    info!("✅ Background services started");

    // === 5. 确保默认管理员用户存在 ===
    #[cfg(not(feature = "harmonyos"))]
    {
        if let Err(e) = crate::api::system::ensure_default_admin_user(&app_state).await {
            error!("Failed to ensure default admin user: {}", e);
        }
    }

    // === 6. 设置优雅关闭处理 ===
    #[cfg(not(feature = "harmonyos"))]
    let shutdown_handle = tokio::spawn(async move {
        crate::application::service_manager::setup_graceful_shutdown().await;
    });

    // === 7. 创建并启动 Web 服务器 ===
    info!("🌐 Starting web server");

    #[cfg(feature = "harmonyos")]
    let app = {
        use tower_http::services::ServeDir;
        let api_router = crate::api::create_router();
        Router::new()
            .nest("/api", api_router)
            .nest_service("/", ServeDir::new("wwwroot"))
            .with_state(app_state)
    };

    #[cfg(not(feature = "harmonyos"))]
    let app = create_app_router(app_state);

    let bind_address = config::get().server_bind_address();
    info!("🚀 Server listening on {}", bind_address);

    let listener = TcpListener::bind(&bind_address).await?;

    // 启动服务器
    #[cfg(not(feature = "harmonyos"))]
    {
        tokio::select! {
            result = axum::serve(listener, app) => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
            }
            _ = shutdown_handle => {
                info!("Graceful shutdown completed");
            }
        }
    }

    #[cfg(feature = "harmonyos")]
    {
        if let Err(e) = axum::serve(listener, app).await {
            error!("Server error: {}", e);
        }
    }

    Ok(())
}

/// Initialize the logging system based on configuration
async fn initialize_logging() -> std::io::Result<()> {
    let config = config::get();

    // Declare _guard variable to retain WorkerGuard for main function lifetime
    let _guard;

    // Create log directory if it doesn't exist
    if config.logging.file_enabled {
        if let Some(parent) = config.log_file_path().parent() {
            std::fs::create_dir_all(parent)?;
        }
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

        tracing_subscriber::fmt()
            .with_env_filter(filter_layer)
            .init();

        info!("Console logging only (level: {})", config.logging.level);
    }

    Ok(())
}

/// Create the main application router
fn create_app_router(app_state: crate::shared::app_state::AppState) -> Router {
    use tower_http::cors::CorsLayer;

    tracing::info!("Creating CORS layer...");
    // 创建CORS层
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any);

    tracing::info!("Creating API router...");
    let api_router = crate::api::create_router();

    tracing::info!("Building main router...");
    let mut router = Router::new()
        // API路由
        .nest("/api", api_router);

    // 静态文件服务
    {
        use axum::http::{StatusCode, Uri};
        use axum::response::{IntoResponse, Response};
        use tower_http::services::{ServeDir, ServeFile};

        tracing::info!("Adding static file service from wwwroot/...");

        // SPA fallback: 所有未匹配的路由返回index.html
        let serve_dir = ServeDir::new("wwwroot")
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new("wwwroot/index.html"));

        router = router.fallback_service(serve_dir);
    }

    tracing::info!("Adding middleware layers...");
    router = router.layer(cors).layer(TraceLayer::new_for_http());

    tracing::info!("Adding application state...");
    router.with_state(app_state)
}
