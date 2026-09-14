use crate::state::AppState;
use axum::{Router, routing::post};

/// Third-party callbacks — no bearer token, verified by signature instead.
/// See `handlers::webhooks` for why this can never be the thing an
/// outbound GitHub call itself triggers.
pub fn webhooks_router() -> Router<AppState> {
    Router::new().route("/github", post(crate::handlers::webhooks::github))
}
