use crate::{
    error::Result,
    models::{
        AlternativesQuery, ApiResponse, NeedsVerificationQuery, ProductCard, ProductResponse,
        QueryProductsQuery, SearchProductQuery, TrendingQuery, VerificationCandidate,
        VerifyProductRequest,
    },
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

pub async fn alternatives(
    State(state): State<AppState>,
    Query(query): Query<AlternativesQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    let alternatives = state
        .product_service
        .find_alternatives(&query.barcode, &query.country, &query.sort_by, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(alternatives, false)))
}

pub async fn query_products(
    State(state): State<AppState>,
    Query(query): Query<QueryProductsQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    let (cards, cached) = state
        .product_service
        .query_products(&query.q, &query.country, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(cards, cached)))
}

pub async fn trending(
    State(state): State<AppState>,
    Query(query): Query<TrendingQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    let cards = state
        .product_service
        .trending(&query.country, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(cards, false)))
}

pub async fn needs_verification(
    State(state): State<AppState>,
    Query(query): Query<NeedsVerificationQuery>,
) -> Result<Json<ApiResponse<Vec<VerificationCandidate>>>> {
    let candidates = state
        .product_service
        .find_needs_verification(&query.country, query.device_id.as_deref(), query.limit)
        .await?;
    Ok(Json(ApiResponse::success(candidates, false)))
}
