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
    /// Structured tag arrays as Open Food Facts publishes them, locale
    /// prefix stripped. `None` is "not published", `[]` is "declared none".
    pub allergens_tags: Option<Value>,
    /// The "may contain" line. Its own field because a trace is a different
    /// claim from an ingredient, and for an allergy it is the one that matters.
    pub traces_tags: Option<Value>,
    pub labels_tags: Option<Value>,
    pub categories_tags: Option<Value>,
    /// `{"sugar": "high", ...}` — theirs when published, ours from the FSA
    /// thresholds when not.
    pub nutrient_levels: Option<Value>,
    pub serving_size: Option<String>,
    pub serving_quantity: Option<f64>,
    pub quantity: Option<String>,
    pub nutriscore_score: Option<i32>,
    pub ecoscore_grade: Option<String>,
    pub completeness: Option<f32>,
    /// When our copy was last brought in step with Open Food Facts, and
    /// their own last-edited stamp at that moment.
    pub off_synced_at: Option<DateTime<Utc>>,
    pub off_last_modified: Option<i64>,
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
    /// Net weight as printed, e.g. `"200 g"`.
    pub quantity: Option<String>,
    pub image_url: Option<String>,
    pub category: Option<String>,
    pub categories: Option<Vec<String>>,

    pub source: String,
    pub verified: bool,
    pub verification_count: i32,
    /// How far behind Open Food Facts this copy is, and how complete their
    /// record was when we took it (0–1, theirs).
    pub off_synced_at: Option<DateTime<Utc>>,
    pub completeness: Option<f32>,

    /// Everything in `nutrition_facts` is per 100 g. `nutrition_per_serving`
    /// is the same document scaled by `serving_quantity` — arithmetic only,
    /// absent rather than guessed when the serving size is unknown.
    pub serving_size: Option<String>,
    pub serving_quantity: Option<f64>,
    pub nutrition_facts: Value,
    pub nutrition_per_serving: Option<Value>,
    pub nutrient_levels: Option<Value>,

    pub ingredients: Option<String>,
    /// Tri-state throughout: `null` means the source doesn't publish this,
    /// `[]` means it publishes "none". Never collapse the two.
    pub allergens: Option<Vec<String>>,
    /// "May contain" — a trace is not an ingredient, and for an allergy it is
    /// the more important line.
    pub traces: Option<Vec<String>>,
    pub additives: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,

    pub nova_group: Option<i16>,
    pub nutriscore_grade: Option<String>,
    pub nutriscore_score: Option<i32>,
    pub ecoscore_grade: Option<String>,

    pub is_vegan: Option<bool>,
    pub is_vegetarian: Option<bool>,
    pub is_palm_oil_free: Option<bool>,
}

/// A JSONB array of strings as we store it.
fn string_list(value: &Option<Value>) -> Option<Vec<String>> {
    let array = value.as_ref()?.as_array()?;
    Some(array.iter().filter_map(Value::as_str).map(str::to_string).collect())
}

impl From<Product> for ProductResponse {
    fn from(product: Product) -> Self {
        // A contributed product stores the allergens its author confirmed as
        // one line of text; an Open Food Facts one stores tags. The client
        // should not have to know which, so both leave here as slugs.
        let allergens = string_list(&product.allergens_tags).or_else(|| {
            product.allergens.as_ref().map(|text| {
                text.split(',')
                    .map(|part| part.trim().to_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect()
            })
        });

        let nutrition_per_serving = crate::services::openfoodfacts::per_serving(
            &product.nutrition_facts,
            product.serving_quantity,
        );

        Self {
            id: product.id,
            barcode: product.barcode,
            country: product.country,
            product_name: product.product_name,
            brand: product.brand,
            quantity: product.quantity,
            image_url: product.image_url,
            category: product.category,
            categories: string_list(&product.categories_tags),
            source: product.source,
            verified: product.verified,
            verification_count: product.verification_count,
            off_synced_at: product.off_synced_at,
            completeness: product.completeness,
            serving_size: product.serving_size,
            serving_quantity: product.serving_quantity,
            nutrition_facts: product.nutrition_facts,
            nutrition_per_serving,
            nutrient_levels: product.nutrient_levels,
            ingredients: product.ingredients,
            allergens,
            traces: string_list(&product.traces_tags),
            additives: string_list(&product.additives),
            labels: string_list(&product.labels_tags),
            nova_group: product.nova_group,
            nutriscore_grade: product.nutriscore_grade,
            nutriscore_score: product.nutriscore_score,
            ecoscore_grade: product.ecoscore_grade,
            is_vegan: product.is_vegan,
            is_vegetarian: product.is_vegetarian,
            is_palm_oil_free: product.is_palm_oil_free,
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
