use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub fn products_router() -> Router<AppState> {
    Router::new()
        .route("/search", get(crate::handlers::products::search_product))
        .route("/verify", post(crate::handlers::products::verify_product))
        .route("/alternatives", get(crate::handlers::products::alternatives))
        .route("/needs-verification", get(crate::handlers::products::needs_verification))
}
