use crate::state::AppState;
use axum::{Router, routing::post};

/// The one crash-report route a client (the iOS app, eventually) calls
/// directly, unauthenticated. Everything that reads or manages reports
/// lives under `/admin/crash-reports` instead — see `routes::v1::admin`.
pub fn crash_reports_router() -> Router<AppState> {
    Router::new().route("/", post(crate::handlers::crash_reports::submit))
}
