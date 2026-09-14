use crate::state::AppState;
use axum::{
    Router,
    routing::{get, patch, post},
};

/// Registration, login, and the signed-in account itself — for
/// `web/apps/dashboard`, not for the consumer app.
pub fn admin_auth_router() -> Router<AppState> {
    Router::new()
        .route("/register", post(crate::handlers::admin_auth::register))
        .route("/login", post(crate::handlers::admin_auth::login))
        .route("/me", get(crate::handlers::admin_auth::me))
        .route("/logout", post(crate::handlers::admin_auth::logout))
        .route("/password", patch(crate::handlers::admin_auth::change_password))
}

/// Reading and triaging reports is for any signed-in staff account;
/// publishing to GitHub is gated to the `admin` role inside the handler
/// itself, since it is the one action here that reaches a third party.
pub fn admin_crash_reports_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::crash_reports::list))
        .route("/", post(crate::handlers::crash_reports::create_manual))
        .route("/{id}", get(crate::handlers::crash_reports::get))
        .route("/{id}", patch(crate::handlers::crash_reports::update))
        .route("/{id}/github-issue", post(crate::handlers::crash_reports::publish_to_github))
}

/// Who's on the team. Changing a role is admin-gated in the handler.
pub fn admin_team_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::admin_auth::list_team))
        .route("/{id}/role", patch(crate::handlers::admin_auth::update_role))
        .route("/{id}/password", patch(crate::handlers::admin_auth::reset_password))
}
