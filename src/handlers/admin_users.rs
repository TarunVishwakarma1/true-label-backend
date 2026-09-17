use crate::{
    auth::AdminUser,
    error::Result,
    models::{AdminUserDetail, AdminUserPage, ApiResponse, ListUsersQuery},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};

/// Browsing user accounts is open to members and admins.
pub async fn list(
    State(state): State<AppState>,
    user: AdminUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<ApiResponse<AdminUserPage>>> {
    user.require_member_or_admin()?;
    let page = state.user_service.list_admin(&query).await?;
    Ok(Json(ApiResponse::success(page, false)))
}

pub async fn get(
    State(state): State<AppState>,
    user: AdminUser,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<AdminUserDetail>>> {
    user.require_member_or_admin()?;
    let detail = state.user_service.get_admin(&device_id).await?;
    Ok(Json(ApiResponse::success(detail, false)))
}
