use crate::db::postgres::check_postgres_health;
use crate::db::redis::check_redis_health;
use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/live", get(health_check))
        .route("/health/ready", get(readiness_check))
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    environment: String,
    version: &'static str,
}

#[derive(Serialize)]
struct ReadinessResponse {
    status: &'static str,
    database: &'static str,
    redis: &'static str,
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        environment: state.config.environment.to_string(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn readiness_check(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = check_postgres_health(&state.db).await.is_ok();
    let redis_ok = check_redis_health(&state.redis).await.is_ok();

    let all_healthy = db_ok && redis_ok;
    let status_code = if all_healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            status: if all_healthy { "ready" } else { "degraded" },
            database: if db_ok { "connected" } else { "unavailable" },
            redis: if redis_ok { "connected" } else { "unavailable" },
        }),
    )
}
