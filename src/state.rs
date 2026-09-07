use crate::config::env::Env;
use crate::error::AppError;
use redis::aio::MultiplexedConnection;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: redis::Client,
    pub config: Arc<Env>,
}

impl AppState {
    pub fn new(db: PgPool, redis: redis::Client, config: Env) -> Self {
        Self {
            db,
            redis,
            config: Arc::new(config),
        }
    }

    pub async fn redis_conn(&self) -> Result<MultiplexedConnection, AppError> {
        self.redis
            .get_multiplexed_async_connection()
            .await
            .map_err(AppError::Redis)
    }
}
