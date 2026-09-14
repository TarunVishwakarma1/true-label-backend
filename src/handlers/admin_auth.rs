use crate::{
    audit,
    auth::{AdminUser, ClientIp, MaybeAdminUser},
    error::Result,
    models::{
        AdminProfile, AdminSession, ApiResponse, AuditLogPage, ChangePasswordRequest,
        ListAuditLogQuery, LoginRequest, RegisterRequest, ResetPasswordRequest, UpdateRoleRequest,
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

/// A handful of staff accounts, not a public sign-up flow — small limits on
/// purpose. The rate limit alone used to be the only gate on registration
/// after the first account; AdminService::register now also requires an
/// existing admin's token for every account after the first.
const REGISTRATIONS_PER_HOUR: u32 = 10;
const LOGIN_ATTEMPTS_PER_HOUR: u32 = 20;
/// Same ceiling as login attempts — change_password's current-password
/// check is exactly as guessable as login's, same limit makes sense.
const PASSWORD_ATTEMPTS_PER_HOUR: u32 = 20;
const HOUR: u64 = 3600;

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    caller: MaybeAdminUser,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<AdminSession>>> {
    state.limit("admin_register", &ip, REGISTRATIONS_PER_HOUR, HOUR).await?;
    let session = state.admin_service.register(&body, caller.0.as_ref()).await?;
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
    let profile = state.admin_service.update_role(&user, id, &body.role).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn change_password(
    State(state): State<AppState>,
    user: AdminUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    state.limit("admin_password", &user.id.to_string(), PASSWORD_ATTEMPTS_PER_HOUR, HOUR).await?;
    state.admin_service.change_password(user.id, &body.current_password, &body.new_password).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "changed": true }), false)))
}

/// For a locked-out teammate — an admin resets it, the teammate logs in
/// with the new password and should change it via `change_password`
/// afterward (this endpoint doesn't force that, it just isn't a
/// self-service flow to begin with).
pub async fn reset_password(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    user.require_admin()?;
    state.admin_service.reset_password(&user, id, &body.new_password).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "reset": true }), false)))
}

/// Admin-only, same posture as `reset_password`/`update_role` — irreversible,
/// so the guard rails (no self-removal, can't remove the last admin) live in
/// the service, not here.
pub async fn remove_member(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    user.require_admin()?;
    state.admin_service.remove_member(&user, id).await?;
    Ok(Json(ApiResponse::success(serde_json::json!({ "removed": true }), false)))
}

/// Admin-only to view — tighter than crash-reports' "any signed-in staff,"
/// since this surfaces password resets and role changes.
pub async fn list_activity(
    State(state): State<AppState>,
    user: AdminUser,
    Query(query): Query<ListAuditLogQuery>,
) -> Result<Json<ApiResponse<AuditLogPage>>> {
    user.require_admin()?;
    let page = audit::list(&state.db, &query).await?;
    Ok(Json(ApiResponse::success(page, false)))
}
