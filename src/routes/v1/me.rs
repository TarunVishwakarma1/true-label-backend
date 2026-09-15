use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, post, put},
};

/// Everything about the calling device. There is no id in any of these
/// paths on purpose: the bearer token is the identity, so a caller has no
/// way to name anybody but itself.
pub fn me_router() -> Router<AppState> {
    Router::new()
        .route("/profile", get(crate::handlers::users::get_profile))
        .route("/profile", put(crate::handlers::users::update_profile))
        .route("/stats", get(crate::handlers::users::get_stats))
        .route("/subscription", get(crate::handlers::users::get_subscription))
        .route("/subscription", post(crate::handlers::users::activate_subscription))
        .route("/subscription", delete(crate::handlers::users::cancel_subscription))
        .route("/link", post(crate::handlers::users::link_account))
        .route("/link/google", post(crate::handlers::users::link_google_account))
        .route("/unlink", post(crate::handlers::users::unlink_account))
        // Required of any app that creates accounts: deletion, from inside
        // the app, not a support email.
        .route("/account", delete(crate::handlers::users::delete_account))
}

pub fn auth_router() -> Router<AppState> {
    Router::new().route("/device", post(crate::handlers::users::register_device))
}
