use axum::http::StatusCode;
use axum_test::TestServer;
use truelabel_backend::{build_app, config::Env};

fn test_config() -> Env {
    Env {
        database_url: "postgresql://postgres:postgres@localhost:5432/truelabel".to_string(),
        redis_url: "redis://localhost:6379".to_string(),
        server_host: "0.0.0.0".to_string(),
        server_port: 8080,
        app_env: truelabel_backend::config::AppEnvironment::Test,
        max_db_connections: 5,
        max_redis_connections: 5,
        rust_log: "info".to_string(),
        apple_bundle_id: "com.tarun.truelable".to_string(),
        trusted_proxy_hops: 0,
        allowed_origins: Vec::new(),
        github_token: None,
        github_repo: None,
    }
}

#[tokio::test]
async fn test_health_endpoint() {
    let app = build_app(test_config()).await.expect("Failed to build app");

    let server = TestServer::new(app);
    let response = server.get("/health").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_invalid_barcode() {
    let app = build_app(test_config()).await.expect("Failed to build app");

    let server = TestServer::new(app);
    let response = server
        .get("/api/v1/products/search?barcode=invalid&country=IN")
        .await;

    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_readiness_reports_healthy_dependencies() {
    let app = build_app(test_config()).await.expect("Failed to build app");

    let server = TestServer::new(app);
    let response = server.get("/health/ready").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}
