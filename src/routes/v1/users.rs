use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, post, put},
};

pub fn users_router() -> Router<AppState> {
    Router::new()
        .route("/{device_id}/profile", get(crate::handlers::users::get_profile))
        .route("/{device_id}/profile", put(crate::handlers::users::update_profile))
        .route("/{device_id}/subscription", get(crate::handlers::users::get_subscription))
        .route("/{device_id}/subscription", post(crate::handlers::users::activate_subscription))
        .route("/{device_id}/subscription", delete(crate::handlers::users::cancel_subscription))
        .route("/{device_id}/link", post(crate::handlers::users::link_account))
        .route("/{device_id}/unlink", post(crate::handlers::users::unlink_account))
        // Required of any app that creates accounts: deletion, from inside
        // the app, not a support email.
        .route("/{device_id}", delete(crate::handlers::users::delete_account))
}
