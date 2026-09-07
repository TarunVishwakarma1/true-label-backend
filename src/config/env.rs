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
            db_url: load_database_url()?,
            redis_url: load_redis_url()?,
        })
    }
}

fn load_database_url() -> Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        if !url.trim().is_empty() {
            return Ok(url);
        }
    }

    let host = std::env::var("DB_HOST").context("Neither DATABASE_URL nor DB_HOST is set")?;
    let port =
        std::env::var("DB_PORT").context("DATABASE_URL is not set, and DB_PORT is missing")?;
    let user =
        std::env::var("DB_USER").context("DATABASE_URL is not set, and DB_USER is missing")?;
    let password = std::env::var("DB_PASSWORD")
        .or_else(|_| std::env::var("DB_PASS"))
        .context("DATABASE_URL is not set, and DB_PASSWORD is missing")?;
    let db_name =
        std::env::var("DB_NAME").context("DATABASE_URL is not set, and DB_NAME is missing")?;

    Ok(format!(
        "postgresql://{user}:{password}@{host}:{port}/{db_name}"
    ))
}

fn load_redis_url() -> Result<String> {
    if let Ok(url) = std::env::var("REDIS_URL") {
        if !url.trim().is_empty() {
            return Ok(url);
        }
    }

    let host = std::env::var("REDIS_HOST").context("Neither REDIS_URL nor REDIS_HOST is set")?;
    let port =
        std::env::var("REDIS_PORT").context("REDIS_URL is not set, and REDIS_PORT is missing")?;

    Ok(format!("redis://{host}:{port}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_database_url_direct() {
        unsafe {
            std::env::set_var("DATABASE_URL", "postgresql://user:pass@remote:5432/testdb");
        }
        let url = load_database_url().unwrap();
        assert_eq!(url, "postgresql://user:pass@remote:5432/testdb");
    }

    #[test]
    fn test_load_database_url_from_fields() {
        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::set_var("DB_HOST", "my-host");
            std::env::set_var("DB_PORT", "5432");
            std::env::set_var("DB_USER", "my-user");
            std::env::set_var("DB_PASSWORD", "my-secret");
            std::env::set_var("DB_NAME", "my-db");
        }

        let url = load_database_url().unwrap();
        assert_eq!(url, "postgresql://my-user:my-secret@my-host:5432/my-db");
    }

    #[test]
    fn test_load_redis_url_from_fields() {
        unsafe {
            std::env::remove_var("REDIS_URL");
            std::env::set_var("REDIS_HOST", "my-redis");
            std::env::set_var("REDIS_PORT", "6380");
        }

        let url = load_redis_url().unwrap();
        assert_eq!(url, "redis://my-redis:6380");
    }

    #[test]
    fn test_load_missing_field_fails() {
        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::remove_var("DB_HOST");
        }
        assert!(load_database_url().is_err());
    }
}
