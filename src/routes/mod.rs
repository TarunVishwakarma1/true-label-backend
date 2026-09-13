pub mod health;
pub mod v1;

use crate::middleware::logging_layer;
use crate::state::AppState;
use axum::routing::{Router, get};
use axum::extract::DefaultBodyLimit;
use tower_http::{compression::CompressionLayer, cors::CorsLayer};

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "TrueLabel API v0.1.0" }))
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .nest("/api/v1", v1::v1_router())
        .with_state(state)
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(logging_layer())
}
