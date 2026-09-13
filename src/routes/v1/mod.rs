pub mod ocr;
pub mod products;
pub mod me;

use crate::state::AppState;
use axum::routing::Router;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .nest("/products", products::products_router())
        .nest("/auth", me::auth_router())
        .nest("/me", me::me_router())
        .nest("/ocr", ocr::ocr_router())
}
