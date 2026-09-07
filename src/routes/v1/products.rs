use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_products))
        .route("/{barcode}", get(get_product_by_barcode))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Product {
    pub barcode: String,
    pub name: String,
    pub brand: Option<String>,
    pub ingredients: Vec<String>,
    pub nutrition_grade: Option<String>,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
}

async fn list_products() -> AppResult<Json<ApiResponse<Vec<Product>>>> {
    let products = vec![Product {
        barcode: "0123456789012".to_string(),
        name: "Organic Whole Milk".to_string(),
        brand: Some("Nature Farm".to_string()),
        ingredients: vec!["Organic Pasteurized Milk".to_string()],
        nutrition_grade: Some("A".to_string()),
    }];

    Ok(Json(ApiResponse {
        success: true,
        data: products,
    }))
}

async fn get_product_by_barcode(
    State(state): State<AppState>,
    Path(barcode): Path<String>,
) -> AppResult<Json<ApiResponse<Product>>> {
    if barcode.trim().is_empty() {
        return Err(AppError::BadRequest("Barcode cannot be empty".to_string()));
    }

    let cache_key = format!("product:{barcode}");
    let mut redis_conn = state.redis_conn().await?;

    let cached: Option<String> = redis_conn.get(&cache_key).await.ok();
    if let Some(cached_json) = cached
        && let Ok(product) = serde_json::from_str::<Product>(&cached_json)
    {
        tracing::debug!(barcode = %barcode, "Product retrieved from Redis cache");
        return Ok(Json(ApiResponse {
            success: true,
            data: product,
        }));
    }

    let product = Product {
        barcode: barcode.clone(),
        name: format!("Product {barcode}"),
        brand: Some("TrueLabel Brand".to_string()),
        ingredients: vec!["Ingredient 1".to_string(), "Ingredient 2".to_string()],
        nutrition_grade: Some("B".to_string()),
    };

    if let Ok(json) = serde_json::to_string(&product) {
        let _: Result<(), _> = redis_conn.set_ex(&cache_key, json, 60).await;
    }

    Ok(Json(ApiResponse {
        success: true,
        data: product,
    }))
}
