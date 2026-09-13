use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: Uuid,
    pub barcode: String,
    pub country: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub nutrition_facts: Value,
    pub ingredients: Option<String>,
    pub allergens: Option<String>,
    pub source: String,
    pub verified: bool,
    pub verification_count: i32,
    pub confidence_score: Option<f32>,
    /// E-numbers, e.g. `["E150D", "E338"]` — the structured, filterable
    /// form of "what preservatives/artificial ingredients does this have".
    pub additives: Option<Value>,
    /// 1 (unprocessed) – 4 (ultra-processed), per the NOVA classification.
    pub nova_group: Option<i16>,
    /// Open Food Facts' overall nutrition quality grade, `a`–`e`.
    pub nutriscore_grade: Option<String>,
    /// Tri-state: `None` means Open Food Facts doesn't have a definitive
    /// answer, not that the product is "no" — never collapse to `false`.
    pub is_vegan: Option<bool>,
    pub is_vegetarian: Option<bool>,
    pub is_palm_oil_free: Option<bool>,
    /// Open Food Facts' most specific `categories_tags` entry, e.g.
    /// `"fruit-nectars"` — `None` for older rows and non-OFF sources.
    pub category: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductResponse {
    pub id: Uuid,
    pub barcode: String,
    pub country: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub nutrition_facts: Value,
    pub ingredients: Option<String>,
    pub allergens: Option<String>,
    pub source: String,
    pub verified: bool,
    pub verification_count: i32,
    pub additives: Option<Value>,
    pub nova_group: Option<i16>,
    pub nutriscore_grade: Option<String>,
    pub is_vegan: Option<bool>,
    pub is_vegetarian: Option<bool>,
    pub is_palm_oil_free: Option<bool>,
    pub category: Option<String>,
}

impl From<Product> for ProductResponse {
    fn from(product: Product) -> Self {
        Self {
            id: product.id,
            barcode: product.barcode,
            country: product.country,
            product_name: product.product_name,
            brand: product.brand,
            image_url: product.image_url,
            nutrition_facts: product.nutrition_facts,
            ingredients: product.ingredients,
            allergens: product.allergens,
            source: product.source,
            verified: product.verified,
            verification_count: product.verification_count,
            additives: product.additives,
            nova_group: product.nova_group,
            nutriscore_grade: product.nutriscore_grade,
            is_vegan: product.is_vegan,
            is_vegetarian: product.is_vegetarian,
            is_palm_oil_free: product.is_palm_oil_free,
            category: product.category,
        }
    }
}

/// Just enough to render the "same shelf" line — not a full product payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProductSummary {
    pub barcode: String,
    pub product_name: String,
    /// The value of whichever nutrient `sort_by` asked for, grams per 100g
    /// (matches how `nutrition_facts` stores every nutrient already).
    pub sort_value: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct AlternativesQuery {
    pub barcode: String,
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
}

fn default_sort_by() -> String {
    "sugar".to_string()
}

/// One card in the Verify tab's queue — enough to ask "does this look
/// right?" without claiming a full product payload the caller hasn't
/// fetched.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationCandidate {
    pub barcode: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub energy_kcal: Option<f64>,
    pub sugar: Option<f64>,
    pub sodium: Option<f64>,
    pub verification_count: i32,
}

#[derive(Debug, Deserialize)]
pub struct NeedsVerificationQuery {
    #[serde(default = "default_country")]
    pub country: String,
    pub device_id: Option<String>,
    #[serde(default = "default_verification_limit")]
    pub limit: i64,
}

fn default_verification_limit() -> i64 {
    10
}

#[derive(Debug, Deserialize)]
pub struct SearchProductQuery {
    pub barcode: String,
    #[serde(default = "default_country")]
    pub country: String,
}

fn default_country() -> String {
    "IN".to_string()
}
