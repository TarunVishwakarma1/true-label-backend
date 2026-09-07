pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;

use anyhow::Result;
use axum::Router;
use config::env::Env;
use state::AppState;

pub async fn build_app_state(config: Env) -> Result<AppState> {
    let db = db::init_postgres_pool(&config).await?;
    let redis = db::init_redis_client(&config).await?;

    Ok(AppState::new(db, redis, config))
}

pub fn create_app(state: AppState) -> Router {
    routes::build_router(state)
}
