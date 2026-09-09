pub mod ocr;
pub mod products;
pub mod verifications;

use crate::state::AppState;
use axum::routing::Router;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .nest("/products", products::products_router())
        .nest("/verifications", verifications::verifications_router())
        .nest("/ocr", ocr::ocr_router())
}
