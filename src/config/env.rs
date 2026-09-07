use anyhow::{Context, Result};
use std::fmt::Display;

#[derive(Debug)]
pub struct Env {
    pub db_url: String,
    pub redis_url: String,
}

impl Env {
    pub fn load() -> Result<Self> {
        Ok(Env {
            db_url: std::env::var("DATABASE_URL")
                .context("Environment variable DATABASE_URL is not set")?,
            redis_url: std::env::var("REDIS_URL")
                .context("Environment variable REDIS_URL is not set")?,
        })
    }
}

impl Display for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Database URL: {}, \nRedis URL: {}",
            self.db_url, self.redis_url,
        )
    }
}

impl Default for Env {
    fn default() -> Self {
        Env {
            db_url: "postgresql://postgres:postgres@localhost:5432/postgres".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
        }
    }
}
