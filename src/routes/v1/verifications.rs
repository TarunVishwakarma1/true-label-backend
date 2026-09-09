use crate::state::AppState;
use axum::{Router, routing::post};

pub fn verifications_router() -> Router<AppState> {
    Router::new().route(
        "/submit",
        post(crate::handlers::verifications::submit_verification),
    )
}
