use crate::{
    error::Result,
    models::{ApiResponse, ProfileResponse, SubscriptionResponse, UpdateProfileRequest},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProfileQuery {
    /// Seeds the row on first read; ignored once the profile exists, since
    /// the client owns its country through the update endpoint after that.
    #[serde(default = "default_country")]
    pub country: String,
}

fn default_country() -> String {
    "IN".to_string()
}

pub async fn get_profile(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Query(query): Query<ProfileQuery>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.get_or_create(&device_id, &query.country).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn update_profile(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.update(&device_id, &body).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn get_subscription(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    let subscription = state.user_service.subscription(&device_id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn activate_subscription(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    let subscription = state.user_service.activate_plus(&device_id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn cancel_subscription(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    let subscription = state.user_service.cancel_plus(&device_id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn link_account(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
    Json(body): Json<crate::models::LinkAccountRequest>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.link_apple(&device_id, &body).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn unlink_account(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.unlink(&device_id).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn delete_account(
    State(state): State<AppState>,
    Path(device_id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    state.user_service.delete_account(&device_id).await?;
    Ok(Json(ApiResponse::success(
        serde_json::json!({ "deleted": true }),
        false,
    )))
}
