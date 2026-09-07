use anyhow::{Context, Result};
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
    Test,
}

impl FromStr for Environment {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "prod" | "production" => Environment::Production,
            "test" | "testing" => Environment::Test,
            _ => Environment::Development,
        })
    }
}

impl Environment {
    pub fn is_dev(&self) -> bool {
        matches!(self, Environment::Development)
    }

    pub fn is_prod(&self) -> bool {
        matches!(self, Environment::Production)
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => write!(f, "development"),
            Environment::Production => write!(f, "production"),
            Environment::Test => write!(f, "test"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Env {
    pub environment: Environment,
    pub server_host: String,
    pub server_port: u16,
    pub db_url: String,
    pub redis_url: String,
    pub db_max_connections: u32,
}

impl Env {
    pub fn load() -> Result<Self> {
        let environment = std::env::var("APP_ENV")
            .or_else(|_| std::env::var("ENVIRONMENT"))
            .map(|val| val.parse().unwrap_or(Environment::Development))
            .unwrap_or(Environment::Development);

        let server_host = std::env::var("SERVER_HOST")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "0.0.0.0".to_string());

        let server_port = std::env::var("SERVER_PORT")
            .or_else(|_| std::env::var("PORT"))
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8080);

        let db_max_connections = std::env::var("DB_MAX_CONNECTIONS")
            .ok()
            .and_then(|p| p.parse::<u32>().ok())
            .unwrap_or(10);

        Ok(Env {
            environment,
            server_host,
            server_port,
            db_url: load_database_url()?,
            redis_url: load_redis_url()?,
            db_max_connections,
        })
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }

    pub fn safe_db_url(&self) -> String {
        if let Ok(url) = url_cleaner(&self.db_url) {
            url
        } else {
            "[REDACTED_DB_URL]".to_string()
        }
    }
}

fn url_cleaner(raw_url: &str) -> Result<String> {
    if let Some((proto_user, host_path)) = raw_url.split_once('@')
        && let Some((proto, _user_pass)) = proto_user.split_once("://")
    {
        return Ok(format!("{proto}://***:***@{host_path}"));
    }
    Ok(raw_url.to_string())
}

fn load_database_url() -> Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL")
        && !url.trim().is_empty()
    {
        return Ok(url);
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
    if let Ok(url) = std::env::var("REDIS_URL")
        && !url.trim().is_empty()
    {
        return Ok(url);
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
            "Environment: {}, Server: {}, Database: {}, Redis: {}",
            self.environment,
            self.server_addr(),
            self.safe_db_url(),
            self.redis_url,
        )
    }
}

impl Default for Env {
    fn default() -> Self {
        Env {
            environment: Environment::Development,
            server_host: "0.0.0.0".to_string(),
            server_port: 8080,
            db_url: "postgresql://postgres:postgres@localhost:5432/postgres".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            db_max_connections: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_load_database_url_direct() {
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::set_var("DATABASE_URL", "postgresql://user:pass@remote:5432/testdb");
        }
        let url = load_database_url().unwrap();
        assert_eq!(url, "postgresql://user:pass@remote:5432/testdb");
    }

    #[test]
    fn test_load_database_url_from_fields() {
        let _guard = ENV_MUTEX.lock().unwrap();
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
        let _guard = ENV_MUTEX.lock().unwrap();
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
        let _guard = ENV_MUTEX.lock().unwrap();
        unsafe {
            std::env::remove_var("DATABASE_URL");
            std::env::remove_var("DB_HOST");
        }
        assert!(load_database_url().is_err());
    }

    #[test]
    fn test_safe_db_url() {
        let env = Env::default();
        assert_eq!(
            env.safe_db_url(),
            "postgresql://***:***@localhost:5432/postgres"
        );
    }
}
