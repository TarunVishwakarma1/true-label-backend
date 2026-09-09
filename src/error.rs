use axum::{Json, http::StatusCode, response::IntoResponse, response::Response};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Product not found")]
    ProductNotFound,

    #[error("Invalid barcode format")]
    InvalidBarcode,

    #[error("Invalid country code")]
    InvalidCountry,

    #[error("Database error: {0}")]
    Database(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("OCR processing failed: {0}")]
    OcrFailed(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Internal server error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::ProductNotFound => (StatusCode::NOT_FOUND, "Product not found"),
            AppError::InvalidBarcode => (
                StatusCode::BAD_REQUEST,
                "Invalid barcode format (8-14 digits)",
            ),
            AppError::InvalidCountry => (StatusCode::BAD_REQUEST, "Invalid country code"),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            AppError::Cache(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Cache error"),
            AppError::ExternalApi(_) => (StatusCode::BAD_GATEWAY, "External API error"),
            AppError::OcrFailed(_) => (StatusCode::BAD_REQUEST, "OCR processing failed"),
            AppError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "Invalid request"),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };

        let body = Json(json!({
            "status": "error",
            "error": error_message,
            "timestamp": chrono::Utc::now()
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_error_variants_to_expected_status_codes() {
        assert_eq!(
            AppError::ProductNotFound.into_response().status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::InvalidBarcode.into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::ExternalApi("timeout".into())
                .into_response()
                .status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            AppError::Internal.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
