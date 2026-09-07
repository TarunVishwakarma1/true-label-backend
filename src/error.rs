use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis cache error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, details) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone(), None),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone(), None),
            AppError::Database(err) => {
                tracing::error!(error = %err, "Database error encountered");
                match err {
                    sqlx::Error::RowNotFound => {
                        (StatusCode::NOT_FOUND, "Record not found".to_string(), None)
                    }
                    _ => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "A database error occurred".to_string(),
                        None,
                    ),
                }
            }
            AppError::Redis(err) => {
                tracing::error!(error = %err, "Redis error encountered");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "A cache error occurred".to_string(),
                    None,
                )
            }
            AppError::Internal(err) => {
                tracing::error!(error = %err, "Internal server error encountered");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An unexpected error occurred".to_string(),
                    None,
                )
            }
        };

        let body = Json(ErrorResponse {
            success: false,
            error: message,
            details,
        });

        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        let err_not_found = AppError::NotFound("Item not found".into());
        let res = err_not_found.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let err_bad_req = AppError::BadRequest("Invalid barcode".into());
        let res = err_bad_req.into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        let err_unauth = AppError::Unauthorized("Invalid token".into());
        let res = err_unauth.into_response();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}

