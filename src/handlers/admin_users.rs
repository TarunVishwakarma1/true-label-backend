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

/// Any signed-in staff account can browse — read-only, same posture as
/// crash reports and products.
pub async fn list(
    State(state): State<AppState>,
    _user: AdminUser,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<ApiResponse<AdminUserPage>>> {
    let page = state.user_service.list_admin(&query).await?;
    Ok(Json(ApiResponse::success(page, false)))
}

pub async fn get(
    State(state): State<AppState>,
    _user: AdminUser,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<AdminUserDetail>>> {
    let detail = state.user_service.get_admin(&device_id).await?;
    Ok(Json(ApiResponse::success(detail, false)))
}
