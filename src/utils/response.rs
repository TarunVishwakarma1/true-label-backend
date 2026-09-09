use serde_json::json;

pub fn not_found_response() -> serde_json::Value {
    json!({
        "status": "error",
        "error": "Product not found",
        "timestamp": chrono::Utc::now()
    })
}
