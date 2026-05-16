use axum::Router;
use std::sync::Arc;
use tinyiothub_marketplace::{AppState, cache::SledCache, handler, service::SyncService};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn build_app(state: AppState) -> Router {
    Router::new().merge(handler::routes()).with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_path = std::env::var("LOCAL_DATA_PATH").unwrap_or_else(|_| "marketplace".into());

    let cache = Arc::new(SledCache::new(
        std::env::var("SLED_PATH").unwrap_or_else(|_| "/tmp/marketplace.sled".into()),
    )?);

    let sync_service = Arc::new(SyncService::new(
        Arc::clone(&cache),
        std::path::PathBuf::from(&data_path),
    ));

    if let Err(e) = sync_service.load_local_data().await {
        tracing::warn!("Initial data load failed: {}", e);
    }

    let state = AppState {
        cache,
        sync: sync_service,
    };

    let app = build_app(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".into()).parse::<u16>()?;

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Marketplace API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let shutdown_signal = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
        tracing::info!("Shutting down marketplace...");
    };

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    Ok(())
}
