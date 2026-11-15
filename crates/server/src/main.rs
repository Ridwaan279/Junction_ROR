use emberwatch_platform::{Engine, persistence::PgStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let store = match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgStore::connect(&url).await?),
        Err(_) => None,
    };
    let mut engine = Engine::synthetic().map_err(std::io::Error::other)?;
    if let Some(store) = &store {
        if let Some(saved) = store.load(&engine.scenario.run_id).await? {
            engine = saved;
            engine.scenario.playing = false;
        }
        store.save(&engine).await?;
    }
    let bind = std::env::var("BIND_ADDRESS")
        .or_else(|_| std::env::var("BIND_ADDR"))
        .unwrap_or_else(|_| "127.0.0.1:8080".into());
    let (platform, access) = emberwatch_server::platform_api::initialize(&bind).await?;
    let platform_storage = platform.storage_kind();
    let firecast = emberwatch_server::firecast_api::FirecastOptions::from_env();
    println!(
        "Fire forecasts: auto={} network={} weather={} context_dev={}",
        firecast.auto,
        firecast.network,
        if firecast.weather_synthetic {
            "synthetic"
        } else {
            "live"
        },
        std::env::var("CONTEXT_DEV_API_KEY").is_ok()
    );
    let app = emberwatch_server::router_with_live_options(
        engine,
        store,
        firecast,
        emberwatch_server::live_api::LiveOptions::from_env(),
    )
    .nest(
        "/platform",
        emberwatch_server::platform_api::integrated_router(platform, access),
    );
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!(
        "EmberWatch: http://{bind}; platform /platform (storage: {platform_storage}); browser adapter /platform/ui"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
