pub mod health;
pub mod v1;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { "TrueLabel - Crowdsourced Nutrition & Barcode API" }),
        )
        .merge(health::router())
        .nest("/api/v1", v1::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
