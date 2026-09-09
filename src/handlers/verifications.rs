use crate::error::Result;
use axum::Json;

pub async fn submit_verification(
    Json(_body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({"status": "placeholder"})))
}
