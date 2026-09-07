pub mod products;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new().nest("/products", products::router())
}
