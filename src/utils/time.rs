use chrono::{DateTime, Duration, Utc};

pub fn cache_ttl_seconds(seconds: i64) -> Duration {
    Duration::seconds(seconds)
}

pub fn now() -> DateTime<Utc> {
    Utc::now()
}
