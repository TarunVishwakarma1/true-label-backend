use crate::config::Env;
use crate::services::{CacheService, OcrService, ProductService};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub product_service: Arc<ProductService>,
    pub ocr_service: Arc<OcrService>,
    pub config: Arc<Env>,
}

impl AppState {
    pub async fn new(db: PgPool, redis: ConnectionManager, config: Env) -> Self {
        let cache = CacheService::new(redis.clone());
        let product_service = Arc::new(ProductService::new(db.clone(), cache));
        let ocr_service = Arc::new(OcrService::new(db.clone()));

        Self {
            db,
            redis,
            product_service,
            ocr_service,
            config: Arc::new(config),
        }
    }
}
