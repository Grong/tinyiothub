use axum::Router;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use tinyiothub_cloud::{
    application::{DataContext, ServiceManager},
    infrastructure::config,
};

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

    // Initialize logging system
    initialize_logging().await?;

    info!("🚀 TinyIoTHub Starting...");
    info!("Environment: {}", config::environment());
    info!("Server: {}", config::get().server_bind_address());
    info!("Database: {}", config::get().database.url);
    info!("MQTT: {}", config::get().mqtt_broker_address());
    info!("CPUs: {}", num_cpus::get());

    // === 2. 初始化数据库 ===
    use tinyiothub_cloud::infrastructure::persistence::DatabaseConfig;
    let db_config = DatabaseConfig::from_settings(config::get());
    let data_context = DataContext::new(db_config).await.expect("Failed to initialize DataContext");
    info!("✅ DataContext initialized");

    // === 3. 创建 AppState（包含所有核心组件）===
    let mut app_state = tinyiothub_cloud::shared::app_state::AppState::new(data_context);
    info!("✅ AppState created");

    // === 4. 自动加载动态驱动 ===
    if config::get().device.drivers.auto_load_on_startup {
        let drivers_dir = &config::get().device.drivers.dynamic_drivers_dir;
        info!("🔌 Auto-loading drivers from: {}", drivers_dir);
        match tinyiothub_cloud::domain::device::driver::dynamic::auto_load_drivers(drivers_dir) {
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
        if let Err(e) = tinyiothub_cloud::api::system::ensure_default_admin_user(&app_state).await {
            error!("Failed to ensure default admin user: {}", e);
        }
    }

    // === 6. 设置优雅关闭处理 ===
    #[cfg(not(feature = "harmonyos"))]
    let shutdown_handle = tokio::spawn(async move {
        tinyiothub_cloud::application::service_manager::setup_graceful_shutdown().await;
    });

    // === 7. 创建并启动 Web 服务器 ===
    info!("🌐 Starting web server");

    #[cfg(feature = "harmonyos")]
    let app = {
        use tower_http::services::ServeDir;
        // Initialize MCP tools with AppState for harmonyos
        use std::sync::Arc;
        tinyiothub_cloud::api::mcp::init_app_state(Arc::new(app_state.clone()));
        tinyiothub_cloud::api::mcp::register_tools().await;
        // Refresh agent tools after MCP registration
        if let Err(e) = app_state.agent_runtime.refresh_tools().await {
            tracing::error!("Failed to refresh agent tools: {}", e);
        }
        let api_router = tinyiothub_cloud::api::create_router();
        Router::new()
            .nest("/api", api_router)
            .nest_service("/", ServeDir::new("wwwroot"))
            .with_state(app_state)
    };

    #[cfg(not(feature = "harmonyos"))]
    let app = create_app_router(app_state).await;

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
            .build(config.log_file_path().parent().unwrap_or_else(|| std::path::Path::new("logs")))
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

/// Create the main application router
async fn create_app_router(app_state: tinyiothub_cloud::shared::app_state::AppState) -> Router {
    use tower_http::cors::{AllowOrigin, CorsLayer};

    tracing::info!("Creating CORS layer...");

    // Initialize MCP tools with AppState
    tracing::info!("Initializing MCP tools...");
    use std::sync::Arc;
    tinyiothub_cloud::api::mcp::init_app_state(Arc::new(app_state.clone()));
    tinyiothub_cloud::api::mcp::register_tools().await;
    tracing::info!("MCP tools initialized");

    // Refresh agent tools after MCP registration
    if let Err(e) = app_state.agent_runtime.refresh_tools().await {
        tracing::error!("Failed to refresh agent tools: {}", e);
    }

    // Initialize self-healing state
    let db = app_state.database.clone();
    let _self_healing_state = tinyiothub_cloud::api::self_healing::init_self_healing_state(db);
    // 创建CORS层 - 使用配置中的origins，支持credentials
    let config = tinyiothub_cloud::infrastructure::config::get();
    let cors_origins = &config.server.cors_origins;

    let allowed_headers = [
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        axum::http::header::ACCEPT,
    ];
    let allowed_methods = [
        axum::http::Method::GET,
        axum::http::Method::POST,
        axum::http::Method::PUT,
        axum::http::Method::DELETE,
        axum::http::Method::OPTIONS,
    ];

    let allow_any = cors_origins.contains(&"*".to_string());
    let explicit_origins: Vec<axum::http::HeaderValue> = if allow_any {
        Vec::new()
    } else {
        cors_origins.iter().filter_map(|o| o.parse().ok()).collect()
    };

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            if allow_any {
                return true;
            }
            explicit_origins.iter().any(|o| o == origin)
        }))
        .allow_credentials(true)
        .allow_methods(allowed_methods)
        .allow_headers(allowed_headers);

    tracing::info!("Creating API router...");
    let api_router = tinyiothub_cloud::api::create_router();

    tracing::info!("Building main router...");

    // 静态文件服务 - SPA 模式
    use axum::{
        http::{StatusCode, Uri},
        response::{IntoResponse, Response},
    };

    tracing::info!("Serving static files from wwwroot/ (SPA mode)");

    // 创建一个处理器，对于所有非 API 请求返回对应的 HTML 文件
    async fn spa_handler(uri: Uri) -> Response {
        let path = uri.path();

        // 如果是 API 请求，不应该到这里（已被 nest 处理）
        if path.starts_with("/api/") {
            return (StatusCode::NOT_FOUND, "API endpoint not found").into_response();
        }

        // 尝试读取请求的文件
        let file_path = if path == "/" {
            "wwwroot/index.html".to_string()
        } else if path.ends_with('/') {
            format!("wwwroot{}", path.trim_end_matches('/'))
        } else {
            format!("wwwroot{}", path)
        };

        match tokio::fs::read(&file_path).await {
            Ok(content) => {
                // 根据文件扩展名设置 Content-Type
                let content_type = if file_path.ends_with(".html") {
                    "text/html"
                } else if file_path.ends_with(".js") {
                    "application/javascript"
                } else if file_path.ends_with(".css") {
                    "text/css"
                } else if file_path.ends_with(".json") {
                    "application/json"
                } else if file_path.ends_with(".png") {
                    "image/png"
                } else if file_path.ends_with(".jpg") || file_path.ends_with(".jpeg") {
                    "image/jpeg"
                } else if file_path.ends_with(".svg") {
                    "image/svg+xml"
                } else if file_path.ends_with(".ico") {
                    "image/x-icon"
                } else {
                    "application/octet-stream"
                };

                ([(axum::http::header::CONTENT_TYPE, content_type)], content).into_response()
            }
            Err(_) => {
                // 文件不存在，尝试 path.html
                let html_path = format!("{}.html", file_path);
                match tokio::fs::read(&html_path).await {
                    Ok(content) => {
                        ([(axum::http::header::CONTENT_TYPE, "text/html")], content).into_response()
                    }
                    Err(_) => {
                        // 还是找不到，返回 index.html（SPA 路由）
                        tracing::info!("Serving index.html for SPA route: {}", path);
                        match tokio::fs::read("wwwroot/index.html").await {
                            Ok(content) => {
                                ([(axum::http::header::CONTENT_TYPE, "text/html")], content)
                                    .into_response()
                            }
                            Err(_) => {
                                (StatusCode::NOT_FOUND, "index.html not found").into_response()
                            }
                        }
                    }
                }
            }
        }
    }

    let mut router = Router::new()
        // API路由优先
        .nest("/api", api_router)
        // 所有其他请求使用 SPA 处理器
        .fallback(spa_handler);

    tracing::info!("Adding middleware layers...");
    router = router.layer(cors).layer(TraceLayer::new_for_http());

    tracing::info!("Adding application state...");
    router.with_state(app_state)
}
