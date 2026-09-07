use crate::config::env::Env;
use anyhow::{Context, Result};
use redis::aio::MultiplexedConnection;

pub async fn init_redis_client(config: &Env) -> Result<redis::Client> {
    tracing::info!(redis_url = %config.redis_url, "Connecting to Redis");

    let client = redis::Client::open(config.redis_url.clone())
        .context("Failed to create Redis client from URL")?;

    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .context("Failed to establish initial Redis multiplexed connection")?;

    let _: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .context("Redis PING check failed during initialization")?;

    tracing::info!("Redis connection established successfully");
    Ok(client)
}

pub async fn get_redis_connection(client: &redis::Client) -> Result<MultiplexedConnection> {
    client
        .get_multiplexed_async_connection()
        .await
        .context("Failed to get async multiplexed Redis connection")
}

pub async fn check_redis_health(client: &redis::Client) -> Result<()> {
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .context("Failed to connect to Redis during health check")?;

    let pong: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .context("Redis PING failed")?;

    if pong == "PONG" || !pong.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("Unexpected response from Redis PING")
    }
}
