use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, patch, post},
};

/// Registration, login, and the signed-in account itself — for
/// `web/apps/dashboard`, not for the consumer app.
pub fn admin_auth_router() -> Router<AppState> {
    Router::new()
        .route("/register", post(crate::handlers::admin_auth::register))
        .route("/login", post(crate::handlers::admin_auth::login))
        .route("/me", get(crate::handlers::admin_auth::me))
        .route("/logout", post(crate::handlers::admin_auth::logout))
        .route(
            "/password",
            patch(crate::handlers::admin_auth::change_password),
        )
        .route(
            "/request-access",
            post(crate::handlers::admin_auth::request_access),
        )
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
        .route(
            "/{id}/github-issue",
            post(crate::handlers::crash_reports::publish_to_github),
        )
}

/// Who's on the team. Changing a role, resetting a password, granting
/// product-edit rights, and removing a member are all admin-gated in the
/// handler.
pub fn admin_team_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::admin_auth::list_team))
        .route(
            "/{id}/role",
            patch(crate::handlers::admin_auth::update_role),
        )
        .route(
            "/{id}/password",
            patch(crate::handlers::admin_auth::reset_password),
        )
        .route(
            "/{id}/permissions",
            patch(crate::handlers::admin_auth::update_permissions),
        )
        .route("/{id}", delete(crate::handlers::admin_auth::remove_member))
}

/// Who changed what. Admin-only — tighter than crash-reports' "any
/// signed-in staff," since this surfaces password resets and role changes.
pub fn admin_activity_router() -> Router<AppState> {
    Router::new().route("/", get(crate::handlers::admin_auth::list_activity))
}

/// Browsing is for any signed-in staff account, same posture as crash
/// reports; editing needs `require_product_edit()` (role admin, or the
/// `can_edit_products` flag), verifying stays `require_admin()` — both
/// checked in the handler, not here.
pub fn admin_products_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::admin_products::list))
        .route("/{id}", get(crate::handlers::admin_products::get))
        .route("/{id}", patch(crate::handlers::admin_products::update))
        .route(
            "/{id}/verify",
            post(crate::handlers::admin_products::verify),
        )
}

/// Read-only — any signed-in staff account. Closes the gap where "why can't
/// this person restore Plus" meant a direct database session; no mutation
/// endpoints here since that gap was about visibility, not editing.
pub fn admin_users_router() -> Router<AppState> {
    Router::new()
        .route("/", get(crate::handlers::admin_users::list))
        .route("/{device_id}", get(crate::handlers::admin_users::get))
}

/// In-app notification center for dashboard staff accounts.
pub fn admin_notifications_router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(crate::handlers::admin_notifications::list_notifications),
        )
        .route(
            "/{id}/read",
            patch(crate::handlers::admin_notifications::mark_notification_as_read),
        )
        .route(
            "/read-all",
            post(crate::handlers::admin_notifications::mark_all_notifications_as_read),
        )
}
