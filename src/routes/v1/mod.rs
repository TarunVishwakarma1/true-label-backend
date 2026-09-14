pub mod admin;
pub mod crash_reports;
pub mod ocr;
pub mod products;
pub mod me;
pub mod webhooks;

use crate::state::AppState;
use axum::routing::Router;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .nest("/products", products::products_router())
        .nest("/auth", me::auth_router())
        .nest("/me", me::me_router())
        .nest("/ocr", ocr::ocr_router())
        .nest("/crash-reports", crash_reports::crash_reports_router())
        .nest("/admin/auth", admin::admin_auth_router())
        .nest("/admin/crash-reports", admin::admin_crash_reports_router())
        .nest("/admin/team", admin::admin_team_router())
        .nest("/admin/activity", admin::admin_activity_router())
        .nest("/admin/products", admin::admin_products_router())
        .nest("/webhooks", webhooks::webhooks_router())
}
