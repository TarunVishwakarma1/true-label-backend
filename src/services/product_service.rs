use crate::{
    error::{AppError, Result},
    models::{CardRow, Product, ProductCard, ProductResponse, SearchProductQuery, VerificationCandidate},
    services::cache_service::CacheService,
    services::openfoodfacts::OffClient,
};
use serde_json::json;
use sqlx::PgPool;

pub struct ProductService {
    db: PgPool,
    cache: CacheService,
    off_client: OffClient,
}

impl ProductService {
    pub fn new(db: PgPool, cache: CacheService) -> Self {
        Self {
            db,
            cache,
            off_client: OffClient::new(),
        }
    }

    #[tracing::instrument(skip(self, query), fields(barcode = %query.barcode, country = %query.country))]
    pub async fn search_product(
        &self,
        query: &SearchProductQuery,
    ) -> Result<(ProductResponse, bool)> {
        if query.barcode.len() < 8 || query.barcode.len() > 14 {
            tracing::warn!(barcode = %query.barcode, "rejected: invalid barcode length");
            return Err(AppError::InvalidBarcode);
        }

        if query.country.len() != 2 {
            tracing::warn!(country = %query.country, "rejected: invalid country code");
            return Err(AppError::InvalidCountry);
        }

        let cache_key = CacheService::cache_key(&query.barcode, &query.country);

        if let Ok(Some(cached)) = self.cache.get(&cache_key).await
            && let Ok(product) = serde_json::from_str::<ProductResponse>(&cached)
        {
            tracing::info!(cache_key = %cache_key, "cache hit");
            self.bump_lookup(&query.barcode);
            return Ok((product, true));
        }
        tracing::debug!(cache_key = %cache_key, "cache miss");

        let product = sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE barcode = $1 AND country = $2",
        )
        .bind(&query.barcode)
        .bind(&query.country)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if let Some(p) = product {
            tracing::info!(product_id = %p.id, source = %p.source, "database hit");
            let response = ProductResponse::from(p);
            let _ = self
                .cache
                .set(&cache_key, &serde_json::to_string(&response).unwrap(), 300)
                .await;
            self.bump_lookup(&query.barcode);
            return Ok((response, false));
        }
        tracing::info!("not in database, fetching from Open Food Facts");

        match self
            .off_client
            .get_product(&query.barcode, &query.country)
            .await?
        {
            Some(json) => {
                let product_obj = json.get("product").ok_or(AppError::ProductNotFound)?;

                let nutrition_facts = json!({
                    "energy_kcal": product_obj["nutriments"].get("energy-kcal"),
                    "protein": product_obj["nutriments"].get("proteins"),
                    "carbs": product_obj["nutriments"].get("carbohydrates"),
                    "fat": product_obj["nutriments"].get("fat"),
                    "saturated_fat": product_obj["nutriments"].get("saturated-fat"),
                    "trans_fat": product_obj["nutriments"].get("trans-fat"),
                    "fiber": product_obj["nutriments"].get("fiber"),
                    "sugar": product_obj["nutriments"].get("sugars"),
                    "sodium": product_obj["nutriments"].get("sodium"),
                    "cholesterol": product_obj["nutriments"].get("cholesterol"),
                    "potassium": product_obj["nutriments"].get("potassium"),
                    "calcium": product_obj["nutriments"].get("calcium"),
                    "iron": product_obj["nutriments"].get("iron"),
                });

                let additives = extract_additives(product_obj);
                let nova_group = product_obj.get("nova_group").and_then(|v| v.as_i64()).map(|n| n as i16);
                let nutriscore_grade = product_obj
                    .get("nutriscore_grade")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim())
                    .filter(|s| s.len() == 1 && s.chars().all(|c| c.is_ascii_alphabetic()))
                    .map(String::from);
                let analysis_tags = extract_analysis_tags(product_obj);
                let is_vegan = dietary_flag(&analysis_tags, "en:vegan", "en:non-vegan");
                let is_vegetarian = dietary_flag(&analysis_tags, "en:vegetarian", "en:non-vegetarian");
                let is_palm_oil_free = dietary_flag(&analysis_tags, "en:palm-oil-free", "en:palm-oil");
                let category = extract_category(product_obj);

                let id = sqlx::query_scalar::<_, uuid::Uuid>(
                    "INSERT INTO products (barcode, country, product_name, brand, image_url, nutrition_facts, ingredients, allergens, source, additives, nova_group, nutriscore_grade, is_vegan, is_vegetarian, is_palm_oil_free, category)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) RETURNING id",
                )
                .bind(&query.barcode)
                .bind(&query.country)
                .bind(product_obj.get("product_name").and_then(|v| v.as_str()).unwrap_or("Unknown"))
                .bind(product_obj.get("brands").and_then(|v| v.as_str()))
                .bind(product_obj.get("image_url").and_then(|v| v.as_str()))
                .bind(&nutrition_facts)
                .bind(product_obj.get("ingredients_text").and_then(|v| v.as_str()))
                .bind(product_obj.get("allergens").and_then(|v| v.as_str()))
                .bind("open_food_facts")
                .bind(&additives)
                .bind(nova_group)
                .bind(&nutriscore_grade)
                .bind(is_vegan)
                .bind(is_vegetarian)
                .bind(is_palm_oil_free)
                .bind(&category)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

                let response = ProductResponse {
                    id,
                    barcode: query.barcode.clone(),
                    country: query.country.clone(),
                    product_name: product_obj
                        .get("product_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                    brand: product_obj
                        .get("brands")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    image_url: product_obj
                        .get("image_url")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    nutrition_facts,
                    ingredients: product_obj
                        .get("ingredients_text")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    allergens: product_obj
                        .get("allergens")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    source: "open_food_facts".to_string(),
                    verified: false,
                    verification_count: 0,
                    additives,
                    nova_group,
                    nutriscore_grade,
                    is_vegan,
                    is_vegetarian,
                    is_palm_oil_free,
                    category,
                };

                tracing::info!(
                    product_id = %id,
                    product_name = %response.product_name,
                    additive_count = response.additives.as_ref().and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0),
                    "created product from Open Food Facts"
                );

                let _ = self
                    .cache
                    .set(&cache_key, &serde_json::to_string(&response).unwrap(), 300)
                    .await;
                self.bump_lookup(&query.barcode);
                Ok((response, false))
            }
            None => {
                tracing::info!("not found in Open Food Facts either");
                Err(AppError::ProductNotFound)
            }
        }
    }

    #[tracing::instrument(skip(self, device_id), fields(barcode = %barcode, country = %country))]
    pub async fn verify_product(
        &self,
        barcode: &str,
        country: &str,
        device_id: &str,
    ) -> Result<ProductResponse> {
        let product = sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE barcode = $1 AND country = $2",
        )
        .bind(barcode)
        .bind(country)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        sqlx::query(
            "INSERT INTO verifications (product_id, barcode, country, device_id, verified) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(product.id)
        .bind(barcode)
        .bind(country)
        .bind(device_id)
        .bind(true)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let new_count: i32 = sqlx::query_scalar(
            "UPDATE products SET verification_count = verification_count + 1 WHERE id = $1 RETURNING verification_count",
        )
        .bind(product.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let newly_verified = new_count >= 3 && !product.verified;
        if new_count >= 3 {
            sqlx::query("UPDATE products SET verified = TRUE WHERE id = $1")
                .bind(product.id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
        }

        if newly_verified {
            tracing::info!(product_id = %product.id, verification_count = new_count, "product reached verified status");
        } else {
            tracing::info!(product_id = %product.id, verification_count = new_count, "verification recorded");
        }

        let cache_key = CacheService::cache_key(barcode, country);
        let _ = self.cache.delete(&cache_key).await;

        let updated_product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(product.id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(ProductResponse::from(updated_product))
    }

    /// Every look-up that resolves to a product counts toward "popular near
    /// you". Fire-and-forget: a lost increment is nothing, a slow one on
    /// the scan path would be felt.
    fn bump_lookup(&self, barcode: &str) {
        let db = self.db.clone();
        let barcode = barcode.to_string();
        tokio::spawn(async move {
            let _ = sqlx::query("UPDATE products SET lookup_count = lookup_count + 1 WHERE barcode = $1")
                .bind(barcode)
                .execute(&db)
                .await;
        });
    }

    /// Same-category products ranked on one nutrient (or on overall
    /// grade). Products without a `category` never appear — an unmatched
    /// shelf beats a wrong one. Empty is a legitimate answer.
    #[tracing::instrument(skip(self), fields(barcode = %barcode, country = %country, sort_by = %sort_by))]
    pub async fn find_alternatives(
        &self,
        barcode: &str,
        country: &str,
        sort_by: &str,
        limit: i64,
    ) -> Result<Vec<ProductCard>> {
        let limit = limit.clamp(1, 10);

        let product = sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE barcode = $1 AND country = $2",
        )
        .bind(barcode)
        .bind(country)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        let Some(category) = product.category else {
            return Ok(Vec::new());
        };

        // Only these literals are ever interpolated — never the caller's
        // raw string — so the format! below has no injection surface.
        let (sort_expr, order) = match sort_by {
            "sugar" | "sodium" | "fat" | "saturated_fat" | "energy_kcal" | "carbs" => {
                (format!("(nutrition_facts->>'{sort_by}')::float8"), "sort_value ASC NULLS LAST")
            }
            "protein" | "fiber" => {
                (format!("(nutrition_facts->>'{sort_by}')::float8"), "sort_value DESC NULLS LAST")
            }
            "score" => ("NULL::float8".to_string(), "nutriscore_grade ASC NULLS LAST, nova_group ASC NULLS LAST"),
            _ => {
                tracing::warn!("unrecognized sort_by, defaulting to sugar");
                ("(nutrition_facts->>'sugar')::float8".to_string(), "sort_value ASC NULLS LAST")
            }
        };

        let rows: Vec<CardRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT barcode, product_name, brand, image_url, nutriscore_grade, nova_group,
                    COALESCE(verified, false), nutrition_facts, {sort_expr} AS sort_value
             FROM products
             WHERE category = $1 AND country = $2 AND barcode != $3
             ORDER BY {order}, lookup_count DESC
             LIMIT $4"
        )))
        .bind(&category)
        .bind(country)
        .bind(barcode)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(ProductCard::from).collect())
    }

    /// Free-text search over what we already know (trigram-ranked), topped
    /// up from Open Food Facts when the local shelf is thin. The OFF call
    /// never waits on a rate-limit slot — a thin local result beats a slow
    /// one. Cached briefly since people retype the same few things.
    #[tracing::instrument(skip(self), fields(country = %country))]
    pub async fn query_products(&self, q: &str, country: &str, limit: i64) -> Result<(Vec<ProductCard>, bool)> {
        let term = q.trim();
        if term.chars().count() < 2 || term.chars().count() > 60 {
            return Err(AppError::InvalidRequest("query must be 2–60 characters".to_string()));
        }
        if country.len() != 2 {
            return Err(AppError::InvalidCountry);
        }
        let limit = limit.clamp(1, 40);
        let cache_key = format!("search:{}:{}:{limit}", country.to_uppercase(), term.to_lowercase());

        if let Ok(Some(cached)) = self.cache.get(&cache_key).await
            && let Ok(cards) = serde_json::from_str::<Vec<ProductCard>>(&cached)
        {
            return Ok((cards, true));
        }

        let rows: Vec<CardRow> = sqlx::query_as(
            "SELECT barcode, product_name, brand, image_url, nutriscore_grade, nova_group,
                    COALESCE(verified, false), nutrition_facts, NULL::float8
             FROM products
             WHERE country = $1
               AND (product_name ILIKE $2 OR brand ILIKE $2 OR barcode = $3
                    OR similarity(product_name, $3) > 0.3)
             ORDER BY GREATEST(similarity(product_name, $3), similarity(COALESCE(brand, ''), $3)) DESC,
                      lookup_count DESC
             LIMIT $4",
        )
        .bind(country)
        .bind(format!("%{term}%"))
        .bind(term)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let mut cards: Vec<ProductCard> = rows.into_iter().map(ProductCard::from).collect();

        if cards.len() < 5 {
            match self.off_client.search(term).await {
                Ok(remote) => {
                    for card in remote {
                        if cards.len() as i64 >= limit {
                            break;
                        }
                        if !cards.iter().any(|c| c.barcode == card.barcode) {
                            cards.push(card);
                        }
                    }
                }
                Err(e) => tracing::warn!(error = %e, "Open Food Facts search failed, returning local results only"),
            }
        }

        let _ = self
            .cache
            .set(&cache_key, &serde_json::to_string(&cards).unwrap(), 600)
            .await;
        Ok((cards, false))
    }

    /// Most looked-up products in a country. Falls through to newest when
    /// nothing has been looked up yet, so a fresh deployment isn't blank.
    #[tracing::instrument(skip(self), fields(country = %country))]
    pub async fn trending(&self, country: &str, limit: i64) -> Result<Vec<ProductCard>> {
        let rows: Vec<CardRow> = sqlx::query_as(
            "SELECT barcode, product_name, brand, image_url, nutriscore_grade, nova_group,
                    COALESCE(verified, false), nutrition_facts, NULL::float8
             FROM products
             WHERE country = $1 AND product_name <> 'Unknown'
             ORDER BY lookup_count DESC, verification_count DESC, created_at DESC
             LIMIT $2",
        )
        .bind(country)
        .bind(limit.clamp(1, 30))
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows.into_iter().map(ProductCard::from).collect())
    }

    /// Feeds the Verify tab's queue: products this device hasn't already
    /// voted on, that haven't yet crossed the 3-verification threshold.
    /// `device_id` absent (a fresh install, no identifierForVendor yet)
    /// just skips the exclusion rather than erroring — an occasional
    /// repeat card is a much smaller problem than a broken feed.
    #[tracing::instrument(skip(self), fields(country = %country))]
    pub async fn find_needs_verification(
        &self,
        country: &str,
        device_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<VerificationCandidate>> {
        let rows: Vec<(String, String, Option<String>, Option<String>, Option<String>, serde_json::Value, i32)> = sqlx::query_as(
            "SELECT barcode, product_name, brand, image_url, nutriscore_grade, nutrition_facts, verification_count
             FROM products
             WHERE country = $1 AND verification_count < 3
               AND ($2::text IS NULL OR NOT EXISTS (
                 SELECT 1 FROM verifications v
                 JOIN products p2 ON p2.id = v.product_id
                 WHERE p2.barcode = products.barcode AND v.device_id = $2
               ))
             ORDER BY created_at DESC
             LIMIT $3",
        )
        .bind(country)
        .bind(device_id)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(barcode, product_name, brand, image_url, nutriscore_grade, nutrition_facts, verification_count)| {
                VerificationCandidate {
                    barcode,
                    product_name,
                    brand,
                    image_url,
                    nutriscore_grade,
                    energy_kcal: nutrition_facts.get("energy_kcal").and_then(|v| v.as_f64()),
                    sugar: nutrition_facts.get("sugar").and_then(|v| v.as_f64()),
                    sodium: nutrition_facts.get("sodium").and_then(|v| v.as_f64()),
                    verification_count,
                }
            })
            .collect())
    }
}

/// OFF tags E-numbers like `"en:e150d"` — strip the locale prefix and
/// uppercase to the conventional `"E150D"` form. `None` (not an empty
/// array) when the field is absent, so callers can tell "no additives" from
/// "this product simply doesn't have additive data" — those aren't the same
/// claim to make about what's in the ingredients.
fn extract_additives(product_obj: &serde_json::Value) -> Option<serde_json::Value> {
    let tags = product_obj.get("additives_tags")?.as_array()?;
    let additives: Vec<String> = tags
        .iter()
        .filter_map(|t| t.as_str())
        .map(|t| t.trim_start_matches("en:").to_uppercase())
        .collect();
    Some(json!(additives))
}

/// OFF's `categories_tags` runs general→specific (e.g. `["en:beverages",
/// "en:fruit-drinks", "en:fruit-nectars"]`) — the last entry is the most
/// specific shelf a product belongs to, which is what "same shelf"
/// alternatives should actually match on.
fn extract_category(product_obj: &serde_json::Value) -> Option<String> {
    let tags = product_obj.get("categories_tags")?.as_array()?;
    let last = tags.last()?.as_str()?;
    Some(last.trim_start_matches("en:").to_string())
}

fn extract_analysis_tags(product_obj: &serde_json::Value) -> Vec<String> {
    product_obj
        .get("ingredients_analysis_tags")
        .and_then(|v| v.as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(|t| t.as_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Tri-state read of an OFF ingredients-analysis flag: `Some(true)` for a
/// definitive positive tag, `Some(false)` for a definitive negative,
/// `None` for "uncertain"/absent — deliberately never guessed, since a
/// wrong vegan/allergen-adjacent claim is worse than no claim. Positive and
/// negative tags don't follow one shared naming pattern across OFF's
/// taxonomy (`en:non-vegan` vs. plain `en:palm-oil`, not
/// `en:non-palm-oil-free`), so both are passed in explicitly.
fn dietary_flag(tags: &[String], positive: &str, negative: &str) -> Option<bool> {
    if tags.iter().any(|t| t == positive) {
        return Some(true);
    }
    if tags.iter().any(|t| t == negative) {
        return Some(false);
    }
    None
}

#[cfg(test)]
mod enrichment_tests {
    use super::*;

    #[test]
    fn extracts_and_formats_additive_tags() {
        let product = json!({ "additives_tags": ["en:e150d", "en:e338"] });
        let additives = extract_additives(&product).unwrap();
        assert_eq!(additives, json!(["E150D", "E338"]));
    }

    #[test]
    fn category_takes_the_most_specific_tag() {
        let product = json!({ "categories_tags": ["en:beverages", "en:fruit-drinks", "en:fruit-nectars"] });
        assert_eq!(extract_category(&product), Some("fruit-nectars".to_string()));
    }

    #[test]
    fn category_is_none_when_field_absent() {
        let product = json!({});
        assert!(extract_category(&product).is_none());
    }

    #[test]
    fn additives_is_none_when_field_absent() {
        let product = json!({});
        assert!(extract_additives(&product).is_none());
    }

    #[test]
    fn vegan_flag_true_on_definitive_positive_tag() {
        let tags = vec!["en:vegan".to_string()];
        assert_eq!(dietary_flag(&tags, "en:vegan", "en:non-vegan"), Some(true));
    }

    #[test]
    fn vegan_flag_false_on_definitive_negative_tag() {
        let tags = vec!["en:non-vegan".to_string()];
        assert_eq!(dietary_flag(&tags, "en:vegan", "en:non-vegan"), Some(false));
    }

    #[test]
    fn vegan_flag_none_when_only_uncertain_tag_present() {
        let tags = vec!["en:maybe-vegan".to_string()];
        assert_eq!(dietary_flag(&tags, "en:vegan", "en:non-vegan"), None);
    }

    #[test]
    fn palm_oil_free_detects_presence_via_its_own_negative_tag() {
        // Regression test: palm oil's negative tag is "en:palm-oil", not
        // "en:non-palm-oil-free" — a generic "non-{label}" pattern would
        // silently miss this and report `None` even when OFF explicitly
        // says the product contains palm oil.
        let contains_palm_oil = vec!["en:palm-oil".to_string()];
        assert_eq!(
            dietary_flag(&contains_palm_oil, "en:palm-oil-free", "en:palm-oil"),
            Some(false)
        );

        let is_free = vec!["en:palm-oil-free".to_string()];
        assert_eq!(
            dietary_flag(&is_free, "en:palm-oil-free", "en:palm-oil"),
            Some(true)
        );
    }
}
