//! Who changed what. Free functions, not a service — every call site
//! already holds a `&PgPool` (via `AppState.db` or a service's own `db`
//! field), so there's nothing to wire into `AppState`.

use crate::{
    error::{AppError, Result},
    models::{AuditLogEntry, AuditLogPage, ListAuditLogQuery},
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Best-effort: logs and swallows its own failure so every call site is one
/// bare `audit::record(...).await;` instead of repeating log-and-ignore at
/// each of the handful of places this gets called. An audit-log write
/// failing must never fail the action it's recording.
pub async fn record(
    db: &PgPool,
    actor_id: Option<Uuid>,
    actor_name: &str,
    action: &str,
    target_type: &str,
    target_id: Uuid,
    metadata: Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_log (actor_id, actor_name, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_id)
    .bind(actor_name)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(&metadata)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::error!(error = %e, action, target_type, %target_id, "failed to record audit log entry");
    }
}

#[tracing::instrument(skip_all)]
pub async fn list(db: &PgPool, query: &ListAuditLogQuery) -> Result<AuditLogPage> {
    let limit = query.limit.clamp(1, 200);
    let offset = query.offset.max(0);
    let action = query.action.as_deref();
    let target_type = query.target_type.as_deref();

    let items = sqlx::query_as::<_, AuditLogEntry>(
        "SELECT * FROM audit_log
         WHERE ($1::text IS NULL OR action = $1)
           AND ($2::text IS NULL OR target_type = $2)
         ORDER BY created_at DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(action)
    .bind(target_type)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE ($1::text IS NULL OR action = $1)
           AND ($2::text IS NULL OR target_type = $2)",
    )
    .bind(action)
    .bind(target_type)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(AuditLogPage { items, total })
}
