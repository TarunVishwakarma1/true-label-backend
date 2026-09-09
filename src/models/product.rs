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
        }
    }
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
