use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub status: String,
    pub data: Option<T>,
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T, cached: bool) -> Self {
        Self {
            status: "success".to_string(),
            data: Some(data),
            cached,
            error: None,
            timestamp: Utc::now(),
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            cached: false,
            error: Some(error),
            timestamp: Utc::now(),
        }
    }

    pub fn not_found() -> Self {
        Self::error("Product not found".to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub environment: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: ServiceStatus,
    pub redis: ServiceStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
