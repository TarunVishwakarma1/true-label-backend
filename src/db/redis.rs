use redis::aio::ConnectionManager;

pub async fn create_connection(redis_url: &str) -> Result<ConnectionManager, redis::RedisError> {
    let client = redis::Client::open(redis_url)?;
    ConnectionManager::new(client).await
}

pub async fn check_redis_health(conn: &ConnectionManager) -> Result<(), redis::RedisError> {
    let mut conn = conn.clone();
    redis::cmd("PING").query_async::<String>(&mut conn).await?;
    Ok(())
}
