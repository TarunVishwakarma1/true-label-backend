use crate::models::{HealthResponse, ReadinessResponse, ServiceStatus};
use axum::{Json, extract::State, http::StatusCode};

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: std::env::var("APP_ENV").unwrap_or_else(|_| "unknown".to_string()),
        timestamp: chrono::Utc::now(),
    })
}

pub async fn liveness() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "alive".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: std::env::var("APP_ENV").unwrap_or_else(|_| "unknown".to_string()),
        timestamp: chrono::Utc::now(),
    })
}

pub async fn readiness(
    State(state): State<crate::state::AppState>,
) -> (StatusCode, Json<ReadinessResponse>) {
    let db_healthy = crate::db::check_postgres_health(&state.db).await.is_ok();
    let redis_healthy = crate::db::check_redis_health(&state.redis).await.is_ok();

    let status = if db_healthy && redis_healthy {
        "ready"
    } else {
        "not_ready"
    };
    let code = if db_healthy && redis_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        code,
        Json(ReadinessResponse {
            status: status.to_string(),
            database: ServiceStatus {
                healthy: db_healthy,
                message: if !db_healthy {
                    Some("Database connection failed".to_string())
                } else {
                    None
                },
            },
            redis: ServiceStatus {
                healthy: redis_healthy,
                message: if !redis_healthy {
                    Some("Redis connection failed".to_string())
                } else {
                    None
                },
            },
            timestamp: chrono::Utc::now(),
        }),
    )
}
