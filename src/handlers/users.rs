use crate::{
    auth::{ClientIp, Device},
    error::Result,
    models::{
        ApiResponse, ContributionStats, DeviceRegistration, ProfileResponse, SubscriptionResponse,
        UpdateProfileRequest,
    },
    state::AppState,
};
use axum::{Json, extract::State};

/// Per hour. A device registers once per install, so anything beyond a
/// handful from one address is somebody minting rows.
const REGISTRATIONS_PER_HOUR: u32 = 10;
/// Per hour, per device. Generous for a person editing their own profile,
/// small enough that a loop achieves nothing.
const WRITES_PER_HOUR: u32 = 60;
const HOUR: u64 = 3600;

pub async fn register_device(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<ApiResponse<DeviceRegistration>>> {
    state.limit("register", &ip, REGISTRATIONS_PER_HOUR, HOUR).await?;
    let registration = state.user_service.register_device().await?;
    Ok(Json(ApiResponse::success(registration, false)))
}

pub async fn get_profile(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.get_or_create(&device.id, "IN").await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn update_profile(
    State(state): State<AppState>,
    device: Device,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    state.limit("profile", &device.id, WRITES_PER_HOUR, HOUR).await?;
    let profile = state.user_service.update(&device.id, &body).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn get_stats(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<ContributionStats>>> {
    let stats = state.user_service.stats(&device.id).await?;
    Ok(Json(ApiResponse::success(stats, false)))
}

pub async fn get_subscription(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    let subscription = state.user_service.subscription(&device.id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn activate_subscription(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    state.limit("subscription", &device.id, WRITES_PER_HOUR, HOUR).await?;
    let subscription = state.user_service.activate_plus(&device.id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn cancel_subscription(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<SubscriptionResponse>>> {
    state.limit("subscription", &device.id, WRITES_PER_HOUR, HOUR).await?;
    let subscription = state.user_service.cancel_plus(&device.id).await?;
    Ok(Json(ApiResponse::success(subscription, false)))
}

pub async fn link_account(
    State(state): State<AppState>,
    device: Device,
    Json(body): Json<crate::models::LinkAccountRequest>,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    // Each attempt costs an Apple key fetch and a signature check.
    state.limit("link", &device.id, WRITES_PER_HOUR, HOUR).await?;
    let profile = state.user_service.link_apple(&device.id, &body).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn unlink_account(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<ProfileResponse>>> {
    let profile = state.user_service.unlink(&device.id).await?;
    Ok(Json(ApiResponse::success(profile, false)))
}

pub async fn delete_account(
    State(state): State<AppState>,
    device: Device,
) -> Result<Json<ApiResponse<serde_json::Value>>> {
    state.user_service.delete_account(&device.id).await?;
    Ok(Json(ApiResponse::success(
        serde_json::json!({ "deleted": true }),
        false,
    )))
}
