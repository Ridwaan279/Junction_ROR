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
    let bind = std::env::var("BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!(
        "EmberWatch synthetic demo: http://{bind} (storage: {})",
        if store.is_some() {
            "postgres"
        } else {
            "memory"
        }
    );
    axum::serve(listener, emberwatch_server::router(engine, store))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
