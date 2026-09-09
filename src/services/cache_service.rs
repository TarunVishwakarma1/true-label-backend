use crate::error::{AppError, Result};
use redis::aio::ConnectionManager;

pub struct CacheService {
    conn: ConnectionManager,
}

impl CacheService {
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.conn.clone();
        redis::cmd("GET")
            .arg(key)
            .query_async::<Option<String>>(&mut conn)
            .await
            .map_err(|e| AppError::Cache(e.to_string()))
    }

    pub async fn set(&self, key: &str, value: &str, ttl_secs: usize) -> Result<()> {
        let mut conn = self.conn.clone();
        redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(value)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| AppError::Cache(e.to_string()))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| AppError::Cache(e.to_string()))
    }

    pub fn cache_key(barcode: &str, country: &str) -> String {
        format!(
            "product:{}:{}",
            barcode.to_lowercase(),
            country.to_uppercase()
        )
    }
}
