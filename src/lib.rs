pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod services;
pub mod state;
pub mod utils;

use crate::{
    config::Env,
    db::{create_connection, create_pool, run_migrations},
    routes::create_router,
    state::AppState,
};
use axum::Router;

pub async fn build_app(config: Env) -> Result<Router, Box<dyn std::error::Error>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let db = create_pool(&config.database_url, config.max_db_connections).await?;
    run_migrations(&db).await?;

    let redis = create_connection(&config.redis_url).await?;

    let state = AppState::new(db, redis, config).await;

    Ok(create_router(state))
}
