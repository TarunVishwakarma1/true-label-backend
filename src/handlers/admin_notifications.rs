use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use uuid::Uuid;

use crate::{auth::AdminUser, error::Result, models::ApiResponse, state::AppState};

/// Lists recent in-app notifications for the authenticated user and their role.
pub async fn list_notifications(
    admin: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let result = state
        .notification_service
        .list_for_user(admin.id, &admin.role, 30)
        .await?;

    Ok(Json(ApiResponse::success(result, false)))
}

/// Marks a specific notification as read.
pub async fn mark_notification_as_read(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    state
        .notification_service
        .mark_as_read(id, admin.id, &admin.role)
        .await?;

    Ok(Json(ApiResponse::success(
        serde_json::json!({ "marked_read": true }),
        false,
    )))
}

/// Marks all unread notifications for the user as read.
pub async fn mark_all_notifications_as_read(
    admin: AdminUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    state
        .notification_service
        .mark_all_as_read(admin.id, &admin.role)
        .await?;

    Ok(Json(ApiResponse::success(
        serde_json::json!({ "all_marked_read": true }),
        false,
    )))
}
