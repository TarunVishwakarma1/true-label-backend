use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Env {
    pub database_url: String,
    pub redis_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub app_env: AppEnvironment,
    pub max_db_connections: u32,
    pub max_redis_connections: u32,
    pub rust_log: String,
    /// Audience an Apple identity token must carry — this app's bundle id.
    pub apple_bundle_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEnvironment {
    Development,
    Production,
    Docker,
    Test,
}

impl AppEnvironment {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "production" => Self::Production,
            "docker" => Self::Docker,
            "test" => Self::Test,
            _ => Self::Development,
        }
    }
}

impl Env {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/truelabel".to_string()
        });

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let server_port = std::env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        let app_env = std::env::var("APP_ENV")
            .map(|e| AppEnvironment::parse(&e))
            .unwrap_or(AppEnvironment::Development);

        let max_db_connections = std::env::var("MAX_DB_CONNECTIONS")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(10);

        let max_redis_connections = std::env::var("MAX_REDIS_CONNECTIONS")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(10);

        let rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

        let apple_bundle_id = std::env::var("APPLE_BUNDLE_ID")
            .unwrap_or_else(|_| "com.tarun.truelable".to_string());

        Ok(Self {
            database_url,
            redis_url,
            server_host,
            server_port,
            app_env,
            max_db_connections,
            max_redis_connections,
            rust_log,
            apple_bundle_id,
        })
    }

    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.server_host, self.server_port)
            .parse()
            .expect("Invalid socket address")
    }

    pub fn is_production(&self) -> bool {
        self.app_env == AppEnvironment::Production
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_environment_from_str_defaults_to_development() {
        assert_eq!(
            AppEnvironment::parse("production"),
            AppEnvironment::Production
        );
        assert_eq!(AppEnvironment::parse("Docker"), AppEnvironment::Docker);
        assert_eq!(
            AppEnvironment::parse("nonsense"),
            AppEnvironment::Development
        );
    }

    #[test]
    fn load_falls_back_to_defaults_when_env_unset() {
        let env = Env::load().expect("load should never fail, only fall back to defaults");
        assert!(env.server_port > 0);
        assert!(!env.database_url.is_empty());
        assert!(!env.redis_url.is_empty());
    }
}
