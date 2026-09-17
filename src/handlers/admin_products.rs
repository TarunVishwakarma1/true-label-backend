use crate::{
    auth::AdminUser,
    error::Result,
    models::{AdminProductPage, AdminUpdateProductRequest, ApiResponse, Product, ListAdminProductsQuery, VerifyProductAdminRequest},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

/// Browsing is open to members and admins.
pub async fn list(
    State(state): State<AppState>,
    user: AdminUser,
    Query(query): Query<ListAdminProductsQuery>,
) -> Result<Json<ApiResponse<AdminProductPage>>> {
    user.require_member_or_admin()?;
    let page = state.product_service.list_admin(&query).await?;
    Ok(Json(ApiResponse::success(page, false)))
}

pub async fn get(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Product>>> {
    user.require_member_or_admin()?;
    let product = state.product_service.get_by_id(id).await?;
    Ok(Json(ApiResponse::success(product, false)))
}

/// Gated by `require_product_edit()`, not `require_admin()` — this is the
/// one endpoint here open to a `member` who's been granted
/// `can_edit_products`, same as crash-reports' `PATCH` is open to any
/// signed-in staff account rather than admin-only.
pub async fn update(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AdminUpdateProductRequest>,
) -> Result<Json<ApiResponse<Product>>> {
    user.require_product_edit()?;
    let product = state.product_service.update_admin(&user, id, &body).await?;
    Ok(Json(ApiResponse::success(product, false)))
}

/// Admin role only — unlike `update` above, `can_edit_products` does not
/// extend to this. Verifying is a separate authoritative call from editing
/// the data itself, same shape as crash-reports' `github-issue` being its
/// own separately-gated action next to the open-to-any-staff `PATCH`.
pub async fn verify(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VerifyProductAdminRequest>,
) -> Result<Json<ApiResponse<Product>>> {
    user.require_admin()?;
    let product = state.product_service.set_verified(&user, id, body.verified).await?;
    Ok(Json(ApiResponse::success(product, false)))
}
