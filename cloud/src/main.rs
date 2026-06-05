use tinyiothub_cloud::{
    server,
    shared::{config, service_manager::ServiceManager},
};
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_appender::{
    non_blocking,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

    // Register JWT validator with tinyiothub-web (so Claims extractor works)
    tinyiothub_web::security::set_jwt_validator(Box::new(|token| {
        tinyiothub_cloud::shared::security::jwt::validate_jwt(token)
            .map(tinyiothub_web::security::Claims::from)
    }));

    // Initialize global start time for uptime calculation (before any health checks)
    let _ = tinyiothub_cloud::modules::monitoring::handler::health::START_TIME
        .set(std::time::SystemTime::now());

    info!("🚀 TinyIoTHub Starting...");
    info!("Environment: {}", config::environment());
    info!("Server: {}", config::get().server_bind_address());
    info!("Database: {}", config::get().database.url);
    info!("MQTT: {}", config::get().mqtt_broker_address());
    info!("CPUs: {}", num_cpus::get());

    // === 2. 初始化数据库 ===
    use tinyiothub_cloud::shared::persistence::DatabaseConfig;
    let settings = config::get();
    let db_url = if settings.database.url.starts_with("sqlite:") {
        settings.database.url.clone()
    } else {
        format!("sqlite:{}", settings.database.url)
    };
    let db_config = DatabaseConfig {
        url: db_url,
        max_connections: settings.database.max_connections,
        min_connections: settings.database.min_connections,
        acquire_timeout_secs: settings.database.connect_timeout_secs,
        idle_timeout_secs: 600,
    };
    let db_pool = tinyiothub_cloud::shared::persistence::create_pool(&db_config)
        .await
        .expect("Failed to create DB pool");
    let device_cache = std::sync::Arc::new(tinyiothub_storage::cache::DeviceCache::new());
    info!("✅ Database pool & device cache initialized");

    // === 3. 创建 AppState（包含所有核心组件）===
    let mut app_state = tinyiothub_cloud::shared::app_state::AppState::new(device_cache, db_pool);
    app_state.agent_pool.set_workspace_service(app_state.workspace_service.clone()).await;
    app_state.agent_pool.set_knowledge_service(app_state.knowledge_service.clone()).await;
    info!("✅ AppState created");

    // === 4. 驱动（静态编译，无需加载） ===
    info!("✅ Drivers registered (static compilation)");

    // === 4.1 重新加载已安装的动态驱动 ===
    {
        use tinyiothub_cloud::shared::persistence::repositories::driver_installation::DriverInstallationRepo;
        let repo = DriverInstallationRepo::new((*app_state.database).clone());
        match repo.find_all().await {
            Ok(installations) => {
                let registry = tinyiothub_runtime::driver_registry();
                for inst in installations {
                    let path = std::path::PathBuf::from(&inst.file_path);
                    match registry.write().load(&path, &inst.workspace_id) {
                        Ok(name) => info!(
                            "✅ Rehydrated driver '{}' for workspace {}",
                            name, inst.workspace_id
                        ),
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

    // === 4.5 从数据库加载完整设备（含属性、指令）到缓存 ===
    {
        use tinyiothub_core::models::device::DeviceQueryParams;
        match app_state.device_service.get_devices(&DeviceQueryParams::default()).await {
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
        if let Err(e) =
            tinyiothub_cloud::modules::system::handler::ensure_default_admin_user(&app_state).await
        {
            error!("Failed to ensure default admin user: {}", e);
        }
    }

    // === 7. 创建并启动 Web 服务器 ===
    info!("🌐 Starting web server");

    #[cfg(feature = "harmonyos")]
    let app = {
        // Initialize MCP tools with AppState for harmonyos
        use std::sync::Arc;

        use axum::Router;
        use tower_http::services::ServeDir;
        tinyiothub_cloud::api::mcp::init_app_state(Arc::new(app_state.clone()));
        tinyiothub_cloud::api::mcp::register_tools().await;
        // Refresh agent tools after MCP registration
        if let Err(e) = app_state.agent_pool.refresh_tools().await {
            tracing::error!("Failed to refresh agent tools: {}", e);
        }
        let api_router = tinyiothub_cloud::api::create_router();
        Router::new()
            .nest("/api", api_router)
            .nest_service("/", ServeDir::new("wwwroot"))
            .with_state(app_state)
    };

    #[cfg(not(feature = "harmonyos"))]
    let app = server::create_app_router(app_state).await;

    let bind_address = config::get().server_bind_address();
    info!("🚀 Server listening on {}", bind_address);

    let listener = TcpListener::bind(&bind_address).await?;

    // 启动服务器
    #[cfg(not(feature = "harmonyos"))]
    {
        tokio::select! {
            result = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()) => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
                if let Err(e) = service_manager.shutdown().await {
                    error!("Service shutdown error: {}", e);
                }
            }
        }
    }

    #[cfg(feature = "harmonyos")]
    {
        if let Err(e) =
            axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await
        {
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
