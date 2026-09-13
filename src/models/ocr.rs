use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OcrSubmission {
    pub id: Uuid,
    pub barcode: String,
    pub country: String,
    pub image_url: Option<String>,
    pub extracted_text: String,
    pub parsed_nutrition: Option<Value>,
    pub confidence_score: Option<f32>,
    pub status: String,
    pub final_product_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Shape matches the iOS client's `ExtractedProductData` exactly (see
/// `IngredientAPIClient.swift` / `ProductInfo.swift`) so it decodes with no
/// custom `CodingKeys`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResponse {
    pub guessed_name: String,
    pub ingredients: String,
    pub allergens: Vec<String>,
}

/// The client runs OCR on-device (Vision framework) and sends us the raw
/// recognized text plus what the user confirmed after reviewing/editing our
/// parse of it. `extracted_text` is kept as the untouched audit trail —
/// `reviewed_ingredients`/`reviewed_allergens` are what actually gets
/// stored, but always checked against our own parse of `extracted_text`
/// before trusting them (see `OcrService::submit_label`).
#[derive(Debug, Deserialize)]
pub struct SubmitLabelRequest {
    pub barcode: String,
    pub country: String,
    pub extracted_text: String,
    pub reviewed_ingredients: String,
    pub reviewed_allergens: Vec<String>,
    /// What the user confirmed as the product's name/brand, when the client
    /// read the front of the pack too. Both optional — older clients only
    /// send the ingredient side.
    #[serde(default)]
    pub product_name: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    /// Per-100g values parsed from the nutrition table and confirmed by the
    /// user, keyed exactly like `products.nutrition_facts` (`energy_kcal`,
    /// `sugar`, ...). Validated server-side before it's stored.
    #[serde(default)]
    pub nutrition: Option<Value>,
}
