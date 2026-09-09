use crate::state::AppState;
use axum::{Router, routing::post};

pub fn ocr_router() -> Router<AppState> {
    Router::new().route("/submit", post(crate::handlers::ocr::submit_label))
}
