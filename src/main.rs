#[tokio::main]
async fn main() {
    // Recovery tool, not a server mode: prints an Argon2 hash for a
    // password read from stdin, then exits — no config load, no DB, no
    // listener. Only real use case is the sole-admin-locked-out manual
    // recovery in docs/deployment.mdx: `UPDATE dashboard_users SET
    // password_hash = '<this output>' WHERE email = '...'`. Uses the exact
    // same hash_password() a real register/reset call would, so the result
    // verifies identically — never a hand-rolled hash.
    if std::env::args().any(|a| a == "--hash-password") {
        use std::io::Read;
        let mut password = String::new();
        std::io::stdin().read_to_string(&mut password).expect("failed to read password from stdin");
        let hash = truelabel_backend::services::admin_service::hash_password(password.trim())
            .expect("failed to hash password");
        println!("{hash}");
        return;
    }

    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = truelabel_backend::config::Env::load().expect("Failed to load configuration");

    let addr = config.socket_addr();

    let app = truelabel_backend::build_app(config)
        .await
        .expect("Failed to build app");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to socket");

    tracing::info!("Server running on {}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down"),
    }
}
