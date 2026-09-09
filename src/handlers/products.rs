use crate::{
    error::Result,
    models::{ApiResponse, ProductResponse, SearchProductQuery, VerifyProductRequest},
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
};

pub async fn search_product(
    State(state): State<AppState>,
    Query(query): Query<SearchProductQuery>,
) -> Result<Json<ApiResponse<ProductResponse>>> {
    let (product, cached) = state.product_service.search_product(&query).await?;
    Ok(Json(ApiResponse::success(product, cached)))
}

pub async fn verify_product(
    State(state): State<AppState>,
    Json(body): Json<VerifyProductRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>> {
    let device_id = body.device_id.unwrap_or_else(|| "unknown".to_string());
    let product = state
        .product_service
        .verify_product(&body.barcode, &body.country, &device_id)
        .await?;
    Ok(Json(ApiResponse::success(product, false)))
}
