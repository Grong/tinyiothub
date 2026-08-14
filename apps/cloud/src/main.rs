use tinyiothub_cloud::{
    bootstrap, router,
    shared::{config, service_manager::ServiceManager},
};
use tokio::net::TcpListener;
use tracing::{error, info};

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
    bootstrap::install_panic_hook();

    // === 1. 初始化配置系统（G6：局部 ApplicationSettings，经 AppState 注入，无进程级全局）===
    let settings = match config::load_configuration() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize configuration: {}", e);
            std::process::exit(1);
        }
    };

    // JWT 机制服务（G2）：构造注入，无全局态。web 的 resolver
    // 缝在 AppState 之前注册，JwtService 无状态可安全共享。
    let jwt_service = std::sync::Arc::new(tinyiothub_authn::jwt::JwtService::new(
        tinyiothub_authn::jwt::JwtSettings {
            secret: settings.security.jwt.secret.clone(),
            harmonyos_enabled: settings.harmonyos.enabled,
        },
    ));

    // Initialize logging system
    bootstrap::initialize_logging(&settings).await?;

    // Register the tenant resolver (P4-Task15) — domain crates resolve
    // workspace/tenant scope via tinyiothub_web extractors without depending
    // on cloud's JWT implementation.
    let svc = jwt_service.clone();
    tinyiothub_web::middleware::workspace::set_tenant_resolver(Box::new(move |token| {
        svc.validate_jwt(token)
            .ok()
            .map(|c| tinyiothub_web::middleware::workspace::TenantClaims {
                user_id: c.user_id,
                tenant_id: c.tenant_id,
                workspace_id: c.workspace_id,
            })
    }));

    info!("🚀 TinyIoTHub Starting...");
    info!("Environment: {}", settings.environment());
    info!("Server: {}", settings.server_bind_address());
    info!("Database: {}", settings.database.url);
    info!("MQTT: {}", settings.mqtt_broker_address());
    info!("CPUs: {}", num_cpus::get());

    // === 2. 初始化数据库 ===
    use tinyiothub_storage::DatabaseConfig;
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
    let is_harmonyos = cfg!(target_env = "ohos") || settings.harmonyos.enabled;
    let db_pool = tinyiothub_storage::create_pool(&db_config, is_harmonyos)
        .await
        .expect("Failed to create DB pool");
    let device_cache = std::sync::Arc::new(tinyiothub_storage::cache::DeviceCache::new());
    info!("✅ Database pool & device cache initialized");

    // === 3. 创建 AppState（包含所有核心组件）===
    let mut app_state = tinyiothub_cloud::state::AppState::new(device_cache, db_pool, &settings);
    info!("✅ AppState created");

    // === 4. 驱动（静态编译，无需加载）+ 动态驱动重载 + 设备缓存预热 ===
    info!("✅ Drivers registered (static compilation)");
    bootstrap::rehydrate_drivers(&app_state).await;
    bootstrap::load_device_cache(&app_state).await;

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
        if let Err(e) = tinyiothub_cloud::shared::initialization::ensure_default_admin_user(&app_state).await {
            error!("Failed to ensure default admin user: {}", e);
        }
    }

    // === 7. 创建并启动 Web 服务器 ===
    info!("🌐 Starting web server");

    #[cfg(feature = "harmonyos")]
    let app = {
        // Initialize MCP tools with the mcp domain state slice (harmonyos)
        use std::sync::Arc;

        use axum::{Router, extract::FromRef};
        use tower_http::services::ServeDir;
        crate::domains::mcp::register_tools(Some(Arc::new(crate::domains::mcp::McpState::from_ref(&app_state)))).await;
        app_state
            .agent_pool
            .set_runtime_context(crate::domains::agent::host::tools::service::ToolRuntimeContext {
                device_cache: Some(app_state.device_cache.clone()),
                data_server: app_state.data_server.clone(),
                directive_sink: app_state.directive_sink.clone(),
                pending_actions: Some(app_state.pending_actions.clone()),
            })
            .await;
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
    let app = router::create_app_router(app_state).await;

    let bind_address = settings.server_bind_address();
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
        if let Err(e) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        {
            error!("Server error: {}", e);
        }
    }

    Ok(())
}
