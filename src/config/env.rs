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
    /// Audience a Google identity token must carry — the **Web
    /// application** OAuth client ID from Google Cloud Console (not the
    /// separate Android-type client, which has no secret and is never sent
    /// here). Absent means Google sign-in refuses cleanly at the one
    /// endpoint that needs it, same posture as `github_token`.
    pub google_oauth_client_id: Option<String>,
    /// How many proxies sit in front of this process. `X-Forwarded-For` is
    /// only meaningful if you know this: everything left of what your own
    /// proxies appended is written by the caller. Zero ignores the header.
    pub trusted_proxy_hops: usize,
    /// Browser origins allowed to call this API. Empty means none — a native
    /// client sends no `Origin`, so an empty list costs the app nothing and
    /// stops the API being used as somebody else's free backend.
    pub allowed_origins: Vec<String>,
    /// A GitHub personal access token with `repo` scope (or fine-grained
    /// "Issues: write" on `github_repo` alone), used only by the dashboard's
    /// "publish as GitHub issue" button. Absent means that one endpoint
    /// fails with a clear error rather than the whole app refusing to boot.
    pub github_token: Option<String>,
    /// `owner/repo` the dashboard files crash-report issues against.
    pub github_repo: Option<String>,
    /// The dashboard's own public URL, e.g. `https://dashboard.truelabel.fun`
    /// — used only to link a filed GitHub issue back to the crash report it
    /// came from. Absent just means that one link is omitted, not an error.
    pub dashboard_url: Option<String>,
    /// The shared secret configured on the GitHub webhook (Settings →
    /// Webhooks) that lets `POST /api/v1/webhooks/github` verify a delivery
    /// actually came from GitHub, via `X-Hub-Signature-256`. Absent means
    /// that one endpoint refuses every request — there's no way to trust an
    /// unsigned delivery, so "unset" has to mean "closed," not "open."
    pub github_webhook_secret: Option<String>,
    /// Optional webhook URL (Slack / Discord / Custom endpoint) where instant
    /// notifications are dispatched for key events: new user registration,
    /// access requests, role updates, and new crash reports.
    pub notification_webhook_url: Option<String>,
    /// `warn_about_exposure`'s loopback check assumes this process's own
    /// bind address is what stands between it and the public internet —
    /// true for a droplet (backend + nginx, same host), false inside a
    /// container: a Kubernetes Service decides reachability, not the
    /// process's interface, and `0.0.0.0` there is mandatory plumbing, not
    /// exposure. Set when something *outside this process* — a Service
    /// typed ClusterIP, a security group — already does that job.
    pub trust_container_network: bool,
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

        let google_oauth_client_id =
            std::env::var("GOOGLE_OAUTH_CLIENT_ID").ok().filter(|v| !v.is_empty());

        let trusted_proxy_hops = std::env::var("TRUSTED_PROXY_HOPS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);

        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|o| !o.is_empty())
            .map(String::from)
            .collect();

        let github_token = std::env::var("GITHUB_TOKEN").ok().filter(|v| !v.is_empty());
        let github_repo = std::env::var("GITHUB_REPO").ok().filter(|v| !v.is_empty());
        // Trailing slash trimmed so issue_body's `{dashboard_url}/dashboard/...`
        // never produces a doubled `//`.
        let dashboard_url = std::env::var("DASHBOARD_URL")
            .ok()
            .map(|v| v.trim().trim_end_matches('/').to_string())
            .filter(|v| !v.is_empty());

        let github_webhook_secret =
            std::env::var("GITHUB_WEBHOOK_SECRET").ok().filter(|v| !v.is_empty());

        let notification_webhook_url =
            std::env::var("NOTIFICATION_WEBHOOK_URL").ok().filter(|v| !v.is_empty());

        let trust_container_network = std::env::var("TRUST_CONTAINER_NETWORK")
            .ok()
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));

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
            google_oauth_client_id,
            trusted_proxy_hops,
            allowed_origins,
            github_token,
            github_repo,
            dashboard_url,
            github_webhook_secret,
            notification_webhook_url,
            trust_container_network,
        })
    }

    /// Shouted at boot, because both halves of this are silent failures.
    ///
    /// Behind a reverse proxy every request arrives from the proxy, so the
    /// peer address is the same for everybody. With `TRUSTED_PROXY_HOPS` left
    /// at 0 the whole internet then shares one rate-limit bucket and the API
    /// starts refusing everyone within minutes of any real traffic.
    ///
    /// The opposite pairing is worse. A non-zero hop count says "a proxy in
    /// front appended the last entry of X-Forwarded-For". If the process also
    /// listens on a public interface, anyone reaching that port directly
    /// writes the whole header themselves and picks their own bucket — the
    /// rate limits stop meaning anything. Bind to loopback, make sure a
    /// firewall does the same job, or set `trust_container_network` when
    /// something else already guarantees that (a ClusterIP Service).
    pub fn warn_about_exposure(&self) {
        let loopback = self
            .server_host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);

        if self.trusted_proxy_hops > 0 && !loopback && !self.trust_container_network {
            tracing::error!(
                host = %self.server_host,
                hops = self.trusted_proxy_hops,
                "TRUSTED_PROXY_HOPS is set but this process is listening on a public                  interface. Anyone who reaches it directly can forge X-Forwarded-For and                  bypass every rate limit. Bind SERVER_HOST to 127.0.0.1, or firewall the port."
            );
        }

        if self.trusted_proxy_hops == 0 && loopback {
            tracing::error!(
                "Listening on loopback means a reverse proxy is in front, but                  TRUSTED_PROXY_HOPS is 0 — so every request looks like it came from the                  proxy and the whole internet shares one rate-limit bucket. Set it to the                  number of proxies that append to X-Forwarded-For (nginx alone: 1)."
            );
        }
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
