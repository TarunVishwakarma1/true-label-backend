use crate::{
    auth::{ClientIp, Device, MaybeDevice},
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

/// Per hour. Scanning is the core loop, so the ceiling is high; it exists to
/// stop a script walking the barcode space, not to meter a shopper.
const LOOKUPS_PER_HOUR: u32 = 600;
/// Text search is dearer — trigram matching, and a top-up call to Open Food
/// Facts when the local shelf is thin.
const SEARCHES_PER_HOUR: u32 = 200;
/// Per hour, per device. A confirmation means holding the pack and reading
/// it, which nobody does sixty times an hour for long.
const CONFIRMATIONS_PER_HOUR: u32 = 60;
const HOUR: u64 = 3600;

/// Who to count this against. A token when there is one, the address
/// otherwise — mobile networks put thousands of people behind one address,
/// so limiting everybody by IP would punish a whole carrier for one script.
fn subject(device: &MaybeDevice, ip: &str) -> String {
    match &device.0 {
        Some(device) => format!("d:{}", device.id),
        None => format!("i:{ip}"),
    }
}

pub async fn search_product(
    State(state): State<AppState>,
    device: MaybeDevice,
    ClientIp(ip): ClientIp,
    Query(query): Query<SearchProductQuery>,
) -> Result<Json<ApiResponse<ProductResponse>>> {
    state.limit("lookup", &subject(&device, &ip), LOOKUPS_PER_HOUR, HOUR).await?;
    let (product, cached) = state.product_service.search_product(&query).await?;
    Ok(Json(ApiResponse::success(product, cached)))
}

pub async fn verify_product(
    State(state): State<AppState>,
    device: Device,
    Json(body): Json<VerifyProductRequest>,
) -> Result<Json<ApiResponse<ProductResponse>>> {
    state.limit("verify", &device.id, CONFIRMATIONS_PER_HOUR, HOUR).await?;
    let product = state
        .product_service
        .verify_product(&body.barcode, &body.country, &device.id)
        .await?;
    Ok(Json(ApiResponse::success(product, false)))
}

pub async fn alternatives(
    State(state): State<AppState>,
    device: MaybeDevice,
    ClientIp(ip): ClientIp,
    Query(query): Query<AlternativesQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    state.limit("alternatives", &subject(&device, &ip), LOOKUPS_PER_HOUR, HOUR).await?;
    let (alternatives, cached) = state
        .product_service
        .find_alternatives(&query.barcode, &query.country, &query.sort_by, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(alternatives, cached)))
}

pub async fn query_products(
    State(state): State<AppState>,
    device: MaybeDevice,
    ClientIp(ip): ClientIp,
    Query(query): Query<QueryProductsQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    state.limit("query", &subject(&device, &ip), SEARCHES_PER_HOUR, HOUR).await?;
    let (cards, cached) = state
        .product_service
        .query_products(&query.q, &query.country, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(cards, cached)))
}

pub async fn trending(
    State(state): State<AppState>,
    device: MaybeDevice,
    ClientIp(ip): ClientIp,
    Query(query): Query<TrendingQuery>,
) -> Result<Json<ApiResponse<Vec<ProductCard>>>> {
    state.limit("trending", &subject(&device, &ip), LOOKUPS_PER_HOUR, HOUR).await?;
    let (cards, cached) = state
        .product_service
        .trending(&query.country, query.limit)
        .await?;
    Ok(Json(ApiResponse::success(cards, cached)))
}

pub async fn needs_verification(
    State(state): State<AppState>,
    device: MaybeDevice,
    Query(query): Query<NeedsVerificationQuery>,
) -> Result<Json<ApiResponse<Vec<VerificationCandidate>>>> {
    // Works signed out, but can only skip what you have already voted on if
    // it knows who you are.
    let candidates = state
        .product_service
        .find_needs_verification(&query.country, device.0.as_ref().map(|d| d.id.as_str()), query.limit)
        .await?;
    Ok(Json(ApiResponse::success(candidates, false)))
}
