use crate::config::env::Env;
use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub async fn init_postgres_pool(config: &Env) -> Result<PgPool> {
    tracing::info!(
        max_connections = config.db_max_connections,
        db = %config.safe_db_url(),
        "Connecting to PostgreSQL"
    );

    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.db_url)
        .await
        .context("Failed to connect to PostgreSQL database")?;

    tracing::info!("PostgreSQL connection pool established");
    Ok(pool)
}

pub async fn check_postgres_health(pool: &PgPool) -> Result<()> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .context("PostgreSQL health check failed")?;
    Ok(())
}
