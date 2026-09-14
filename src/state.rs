use crate::config::Env;
use crate::services::{
    AdminService, AppleAuth, CacheService, CrashReportService, GitHubService, OcrService,
    ProductService, UserService,
};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub cache: CacheService,
    pub product_service: Arc<ProductService>,
    pub ocr_service: Arc<OcrService>,
    pub user_service: Arc<UserService>,
    pub admin_service: Arc<AdminService>,
    pub crash_report_service: Arc<CrashReportService>,
    pub config: Arc<Env>,
}

impl AppState {
    pub async fn new(db: PgPool, redis: ConnectionManager, config: Env) -> Self {
        let cache = CacheService::new(redis.clone());
        let product_service = Arc::new(ProductService::new(db.clone(), cache.clone()));
        let ocr_service = Arc::new(OcrService::new(db.clone(), CacheService::new(redis.clone())));
        let user_service = Arc::new(UserService::new(
            db.clone(),
            AppleAuth::new(config.apple_bundle_id.clone()),
        ));
        let admin_service = Arc::new(AdminService::new(db.clone()));
        let github = GitHubService::new(config.github_token.clone(), config.github_repo.clone());
        let crash_report_service = Arc::new(CrashReportService::new(
            db.clone(),
            github,
            config.dashboard_url.clone(),
        ));

        Self {
            db,
            redis,
            cache,
            product_service,
            ocr_service,
            user_service,
            admin_service,
            crash_report_service,
            config: Arc::new(config),
        }
    }

    /// One place to ask "has this subject had enough for now". Returns the
    /// 429 rather than a bool so handlers read as a guard clause.
    pub async fn limit(
        &self,
        bucket: &str,
        subject: &str,
        limit: u32,
        window_secs: u64,
    ) -> crate::error::Result<()> {
        if self.cache.allow(bucket, subject, limit, window_secs).await {
            Ok(())
        } else {
            Err(crate::error::AppError::TooManyRequests)
        }
    }
}
