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

/// Enough to draw a product card anywhere a list of products appears
/// (search results, trending, same-shelf alternatives) — not a full
/// payload. Tapping one goes through the normal `search` look-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductCard {
    pub barcode: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub nutriscore_grade: Option<String>,
    pub nova_group: Option<i16>,
    pub verified: bool,
    pub energy_kcal: Option<f64>,
    pub sugar: Option<f64>,
    pub sodium: Option<f64>,
    /// For alternatives: the value of whichever nutrient `sort_by` asked
    /// for, per 100g. `None` elsewhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_value: Option<f64>,
}

impl ProductCard {
    pub fn from_row(
        barcode: String,
        product_name: String,
        brand: Option<String>,
        image_url: Option<String>,
        nutriscore_grade: Option<String>,
        nova_group: Option<i16>,
        verified: bool,
        nutrition_facts: &Value,
        sort_value: Option<f64>,
    ) -> Self {
        Self {
            barcode,
            product_name,
            brand,
            image_url,
            nutriscore_grade,
            nova_group,
            verified,
            energy_kcal: nutrition_facts.get("energy_kcal").and_then(Value::as_f64),
            sugar: nutrition_facts.get("sugar").and_then(Value::as_f64),
            sodium: nutrition_facts.get("sodium").and_then(Value::as_f64),
            sort_value,
        }
    }
}

/// Row shape shared by every card-producing query below.
pub type CardRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i16>,
    bool,
    Value,
    Option<f64>,
);

impl From<CardRow> for ProductCard {
    fn from(r: CardRow) -> Self {
        ProductCard::from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, &r.7, r.8)
    }
}

#[derive(Debug, Deserialize)]
pub struct AlternativesQuery {
    pub barcode: String,
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_alternatives_limit")]
    pub limit: i64,
}

fn default_sort_by() -> String {
    "sugar".to_string()
}

fn default_alternatives_limit() -> i64 {
    3
}

#[derive(Debug, Deserialize)]
pub struct QueryProductsQuery {
    pub q: String,
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default = "default_query_limit")]
    pub limit: i64,
}

fn default_query_limit() -> i64 {
    20
}

#[derive(Debug, Deserialize)]
pub struct TrendingQuery {
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default = "default_verification_limit")]
    pub limit: i64,
}

/// One card in the Verify tab's queue — enough to ask "does this look
/// right?" without claiming a full product payload the caller hasn't
/// fetched.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationCandidate {
    pub barcode: String,
    pub product_name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub nutriscore_grade: Option<String>,
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
