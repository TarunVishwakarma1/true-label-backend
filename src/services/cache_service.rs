use crate::error::{AppError, Result};
use redis::aio::ConnectionManager;

#[derive(Clone)]
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

    /// Fixed-window counter, incremented and expired in one round trip so a
    /// process dying mid-sequence cannot leave a key without a TTL — which
    /// would jam that subject out permanently.
    ///
    /// Fails **open**: if Redis is unreachable the request is allowed and the
    /// failure is logged. This limiter exists to blunt abuse, and taking the
    /// whole API down when the cache blinks is a worse outcome than letting a
    /// burst through.
    pub async fn allow(&self, bucket: &str, subject: &str, limit: u32, window_secs: u64) -> bool {
        const SCRIPT: &str = r"
            local count = redis.call('INCR', KEYS[1])
            if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
            return count
        ";

        let mut conn = self.conn.clone();
        let key = format!("rl:{bucket}:{subject}");
        let count: std::result::Result<u64, redis::RedisError> = redis::Script::new(SCRIPT)
            .key(&key)
            .arg(window_secs)
            .invoke_async(&mut conn)
            .await;

        match count {
            Ok(count) => {
                let allowed = count <= u64::from(limit);
                if !allowed {
                    tracing::warn!(bucket, subject, count, limit, "rate limit exceeded");
                }
                allowed
            }
            Err(e) => {
                tracing::error!(error = %e, bucket, "rate limiter unavailable, allowing request");
                true
            }
        }
    }

    pub fn cache_key(barcode: &str, country: &str) -> String {
        format!(
            "product:{}:{}",
            barcode.to_lowercase(),
            country.to_uppercase()
        )
    }
}
