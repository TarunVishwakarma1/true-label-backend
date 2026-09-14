use crate::{
    auth::{AdminUser, ClientIp},
    error::Result,
    models::{AdminProfile, AdminSession, ApiResponse, LoginRequest, RegisterRequest, UpdateRoleRequest},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
};
use uuid::Uuid;

/// A handful of staff accounts, not a public sign-up flow — small limits on
/// purpose.
const REGISTRATIONS_PER_HOUR: u32 = 10;
const LOGIN_ATTEMPTS_PER_HOUR: u32 = 20;
const HOUR: u64 = 3600;

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<AdminSession>>> {
    state.limit("admin_register", &ip, REGISTRATIONS_PER_HOUR, HOUR).await?;
    let session = state.admin_service.register(&body).await?;
    Ok(Json(ApiResponse::success(session, false)))
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<LoginRequest>,
) -> Result<Json<ApiResponse<AdminSession>>> {
    state.limit("admin_login", &ip, LOGIN_ATTEMPTS_PER_HOUR, HOUR).await?;
    let session = state.admin_service.login(&body).await?;
    Ok(Json(ApiResponse::success(session, false)))
}

pub async fn me(
    State(state): State<AppState>,
    user: AdminUser,
) -> Result<Json<ApiResponse<AdminProfile>>> {
    let profile = state.admin_service.profile(user.id).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn logout(
    State(state): State<AppState>,
    user: AdminUser,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    state.admin_service.logout(user.id).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "signed_out": true }), false)))
}

/// Any signed-in staff account can see who else is on the team — it's the
/// same posture as seeing who filed a crash report. Changing a role is the
/// privileged part, gated in the handler below.
pub async fn list_team(
    State(state): State<AppState>,
    _user: AdminUser,
) -> Result<Json<ApiResponse<Vec<AdminProfile>>>> {
    let team = state.admin_service.list_team().await?;
    Ok(Json(ApiResponse::success(team, false)))
}

pub async fn update_role(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<ApiResponse<AdminProfile>>> {
    user.require_admin()?;
    let profile = state.admin_service.update_role(id, &body.role).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}
