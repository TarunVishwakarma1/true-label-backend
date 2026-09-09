use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Verification {
    pub id: Uuid,
    pub product_id: Uuid,
    pub barcode: String,
    pub country: String,
    pub device_id: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyProductRequest {
    pub barcode: String,
    pub country: String,
    pub device_id: Option<String>,
}
