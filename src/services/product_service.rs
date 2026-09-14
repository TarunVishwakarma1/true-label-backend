use crate::{
    error::{AppError, Result},
    models::{CardRow, Product, ProductCard, ProductResponse, SearchProductQuery, VerificationCandidate},
    services::cache_service::CacheService,
    services::openfoodfacts::OffClient,
};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;

/// A product's facts don't change between two scans, and the one thing that
/// does — a verification — deletes the key itself.
const PRODUCT_TTL_SECS: usize = 3600;

/// How long "Open Food Facts doesn't have this either" is remembered. Without
/// it, re-scanning one unknown barcode hits the external API every time and
/// burns a budget shared by every user of this service.
const MISS_TTL_SECS: usize = 600;

/// How far our copy may lag Open Food Facts before the next read pays to
/// catch it up. Long on purpose: labels change rarely, and the shared
/// per-minute budget is better spent on barcodes nobody has scanned yet.
const OFF_STALE_AFTER_DAYS: i64 = 30;

/// "Trending" is a ranking, not a fact — nobody needs it accurate to the
/// second, and `lookup_count` moves on every scan. Caching it turns one of
/// the most-hit reads in the app (shown on Home for every user) from a
/// per-request table scan into a handful of Redis keys, one per country.
const TRENDING_TTL_SECS: usize = 600;

/// Same-shelf alternatives change only when nutrition facts do, which is
/// rare — longer than search, which people retype differently every time.
const ALTERNATIVES_TTL_SECS: usize = 1800;

pub struct ProductService {
    db: PgPool,
    cache: CacheService,
    off_client: Arc<OffClient>,
}

impl ProductService {
    pub fn new(db: PgPool, cache: CacheService) -> Self {
        Self {
            db,
            cache,
            off_client: Arc::new(OffClient::new()),
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

        let miss_key = format!("{cache_key}:miss");
        if matches!(self.cache.get(&miss_key).await, Ok(Some(_))) {
            tracing::info!("known miss, not calling Open Food Facts");
            return Err(AppError::ProductNotFound);
        }

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
            self.refresh_if_stale(&p);
            let response = ProductResponse::from(p);
            let _ = self
                .cache
                .set(&cache_key, &serde_json::to_string(&response).unwrap(), PRODUCT_TTL_SECS)
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
                let off = OffFields::from(product_obj);

                let stored = sqlx::query_as::<_, Product>(
                    "INSERT INTO products (barcode, country, product_name, brand, quantity, image_url,
                        nutrition_facts, nutrient_levels, serving_size, serving_quantity,
                        ingredients, allergens_tags, traces_tags, labels_tags, categories_tags,
                        source, additives, nova_group, nutriscore_grade, nutriscore_score,
                        ecoscore_grade, is_vegan, is_vegetarian, is_palm_oil_free, category,
                        completeness, off_last_modified, off_synced_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
                             'open_food_facts', $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, NOW())
                     ON CONFLICT (barcode) DO UPDATE SET updated_at = NOW()
                     RETURNING *",
                )
                .bind(&query.barcode)
                .bind(&query.country)
                .bind(&off.product_name)
                .bind(&off.brand)
                .bind(&off.quantity)
                .bind(&off.image_url)
                .bind(&off.nutrition_facts)
                .bind(&off.nutrient_levels)
                .bind(&off.serving_size)
                .bind(off.serving_quantity)
                .bind(&off.ingredients)
                .bind(&off.allergens_tags)
                .bind(&off.traces_tags)
                .bind(&off.labels_tags)
                .bind(&off.categories_tags)
                .bind(&off.additives)
                .bind(off.nova_group)
                .bind(&off.nutriscore_grade)
                .bind(off.nutriscore_score)
                .bind(&off.ecoscore_grade)
                .bind(off.is_vegan)
                .bind(off.is_vegetarian)
                .bind(off.is_palm_oil_free)
                .bind(&off.category)
                .bind(off.completeness)
                .bind(off.last_modified)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

                let id = stored.id;
                let response = ProductResponse::from(stored);

                tracing::info!(
                    product_id = %id,
                    product_name = %response.product_name,
                    additive_count = response.additives.as_ref().map_or(0, Vec::len),
                    "created product from Open Food Facts"
                );

                let _ = self
                    .cache
                    .set(&cache_key, &serde_json::to_string(&response).unwrap(), PRODUCT_TTL_SECS)
                    .await;
                self.bump_lookup(&query.barcode);
                Ok((response, false))
            }
            None => {
                tracing::info!("not found in Open Food Facts either");
                let _ = self.cache.set(&miss_key, "1", MISS_TTL_SECS).await;
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
        let _ = self.cache.delete(&format!("{cache_key}:miss")).await;

        let updated_product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(product.id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(ProductResponse::from(updated_product))
    }

    /// A read is the only signal we get that a product still matters, so it
    /// is also when we check whether our copy has fallen behind Open Food
    /// Facts. Fire-and-forget and opportunistic: the refresh gives up rather
    /// than queue behind the rate limiter, and the reader is served our
    /// existing row either way. Verification counts, `verified` and
    /// `lookup_count` are ours, not theirs, and are never overwritten.
    fn refresh_if_stale(&self, product: &Product) {
        if product.source != "open_food_facts" {
            return;
        }
        let synced = product.off_synced_at.unwrap_or(product.created_at);
        if (Utc::now() - synced).num_days() < OFF_STALE_AFTER_DAYS {
            return;
        }

        let db = self.db.clone();
        let cache = self.cache.clone();
        let off_client = Arc::clone(&self.off_client);
        let barcode = product.barcode.clone();
        let country = product.country.clone();

        tokio::spawn(async move {
            let Some(json) = off_client.get_product_if_free(&barcode, &country).await else {
                return;
            };
            let Some(product_obj) = json.get("product") else { return };
            let off = OffFields::from(product_obj);

            let updated = sqlx::query(
                "UPDATE products SET
                   product_name = $2, brand = $3, quantity = $4, image_url = $5,
                   nutrition_facts = $6, nutrient_levels = $7, serving_size = $8,
                   serving_quantity = $9, ingredients = $10, allergens_tags = $11,
                   traces_tags = $12, labels_tags = $13, categories_tags = $14,
                   additives = $15, nova_group = $16, nutriscore_grade = $17,
                   nutriscore_score = $18, ecoscore_grade = $19, is_vegan = $20,
                   is_vegetarian = $21, is_palm_oil_free = $22, category = $23,
                   completeness = $24, off_last_modified = $25,
                   off_synced_at = NOW(), updated_at = NOW()
                 WHERE barcode = $1 AND source = 'open_food_facts'",
            )
            .bind(&barcode)
            .bind(&off.product_name)
            .bind(&off.brand)
            .bind(&off.quantity)
            .bind(&off.image_url)
            .bind(&off.nutrition_facts)
            .bind(&off.nutrient_levels)
            .bind(&off.serving_size)
            .bind(off.serving_quantity)
            .bind(&off.ingredients)
            .bind(&off.allergens_tags)
            .bind(&off.traces_tags)
            .bind(&off.labels_tags)
            .bind(&off.categories_tags)
            .bind(&off.additives)
            .bind(off.nova_group)
            .bind(&off.nutriscore_grade)
            .bind(off.nutriscore_score)
            .bind(&off.ecoscore_grade)
            .bind(off.is_vegan)
            .bind(off.is_vegetarian)
            .bind(off.is_palm_oil_free)
            .bind(&off.category)
            .bind(off.completeness)
            .bind(off.last_modified)
            .execute(&db)
            .await;

            match updated {
                Ok(_) => {
                    let _ = cache.delete(&CacheService::cache_key(&barcode, &country)).await;
                    tracing::info!(barcode = %barcode, "refreshed from Open Food Facts");
                }
                Err(e) => tracing::warn!(error = %e, barcode = %barcode, "refresh failed"),
            }
        });
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
    /// shelf beats a wrong one. Empty is a legitimate answer. Cached — this
    /// is a Product-detail read, hit on nearly every scan.
    #[tracing::instrument(skip(self), fields(barcode = %barcode, country = %country, sort_by = %sort_by))]
    pub async fn find_alternatives(
        &self,
        barcode: &str,
        country: &str,
        sort_by: &str,
        limit: i64,
    ) -> Result<(Vec<ProductCard>, bool)> {
        let limit = limit.clamp(1, 10);
        let cache_key = format!("alts:{}:{}:{sort_by}:{limit}", barcode, country.to_uppercase());

        if let Ok(Some(cached)) = self.cache.get(&cache_key).await
            && let Ok(cards) = serde_json::from_str::<Vec<ProductCard>>(&cached)
        {
            return Ok((cards, true));
        }

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
            return Ok((Vec::new(), false));
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

        let cards: Vec<ProductCard> = rows.into_iter().map(ProductCard::from).collect();
        let _ = self
            .cache
            .set(&cache_key, &serde_json::to_string(&cards).unwrap(), ALTERNATIVES_TTL_SECS)
            .await;
        Ok((cards, false))
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
    /// Cached — this is a Home-screen read, hit by every user on every
    /// open, for a ranking that has no reason to be second-fresh.
    #[tracing::instrument(skip(self), fields(country = %country))]
    pub async fn trending(&self, country: &str, limit: i64) -> Result<(Vec<ProductCard>, bool)> {
        let limit = limit.clamp(1, 30);
        let cache_key = format!("trending:{}:{limit}", country.to_uppercase());

        if let Ok(Some(cached)) = self.cache.get(&cache_key).await
            && let Ok(cards) = serde_json::from_str::<Vec<ProductCard>>(&cached)
        {
            return Ok((cards, true));
        }

        let rows: Vec<CardRow> = sqlx::query_as(
            "SELECT barcode, product_name, brand, image_url, nutriscore_grade, nova_group,
                    COALESCE(verified, false), nutrition_facts, NULL::float8
             FROM products
             WHERE country = $1 AND product_name <> 'Unknown'
             ORDER BY lookup_count DESC, verification_count DESC, created_at DESC
             LIMIT $2",
        )
        .bind(country)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let cards: Vec<ProductCard> = rows.into_iter().map(ProductCard::from).collect();
        let _ = self
            .cache
            .set(&cache_key, &serde_json::to_string(&cards).unwrap(), TRENDING_TTL_SECS)
            .await;
        Ok((cards, false))
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

/// Everything we take from an Open Food Facts product, mapped once. Both the
/// first insert and every later refresh go through here — when the mapping
/// lived inline at the insert, the refresh had nowhere to share it from and
/// the two would drift apart, which is the whole failure this guards against.
pub struct OffFields {
    pub product_name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub nutrition_facts: serde_json::Value,
    pub ingredients: Option<String>,
    pub allergens: Option<String>,
    pub additives: Option<serde_json::Value>,
    pub nova_group: Option<i16>,
    pub nutriscore_grade: Option<String>,
    pub is_vegan: Option<bool>,
    pub is_vegetarian: Option<bool>,
    pub is_palm_oil_free: Option<bool>,
    pub category: Option<String>,
    pub categories_tags: Option<serde_json::Value>,
    pub allergens_tags: Option<serde_json::Value>,
    pub traces_tags: Option<serde_json::Value>,
    pub labels_tags: Option<serde_json::Value>,
    pub nutrient_levels: serde_json::Value,
    pub serving_size: Option<String>,
    pub serving_quantity: Option<f64>,
    pub quantity: Option<String>,
    pub nutriscore_score: Option<i32>,
    pub ecoscore_grade: Option<String>,
    pub completeness: Option<f32>,
    /// Open Food Facts' own last-edited stamp, so "are we in step with them"
    /// is answerable without re-reading the whole product.
    pub last_modified: Option<i64>,
}

impl From<&serde_json::Value> for OffFields {
    fn from(product_obj: &serde_json::Value) -> Self {
        let text = |key: &str| {
            product_obj
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        let analysis_tags = extract_analysis_tags(product_obj);
        let nutrition_facts = crate::services::openfoodfacts::nutriments(product_obj);
        let categories = crate::services::openfoodfacts::tags(product_obj, "categories_tags");
        // Sugar in a bottle is judged against a different bar from sugar in a
        // biscuit, so the drink thresholds need to know which this is.
        let is_beverage = categories
            .as_ref()
            .is_some_and(|tags| tags.iter().any(|t| t.contains("beverage") || t.contains("drink")));
        let list = |key: &str| {
            crate::services::openfoodfacts::tags(product_obj, key).map(|tags| serde_json::json!(tags))
        };

        Self {
            product_name: text("product_name").unwrap_or_else(|| "Unknown".to_string()),
            brand: text("brands"),
            image_url: text("image_url"),
            nutrition_facts: nutrition_facts.clone(),
            ingredients: text("ingredients_text"),
            allergens: text("allergens"),
            additives: extract_additives(product_obj),
            nova_group: product_obj.get("nova_group").and_then(|v| v.as_i64()).map(|n| n as i16),
            nutriscore_grade: text("nutriscore_grade"),
            is_vegan: dietary_flag(&analysis_tags, "en:vegan", "en:non-vegan"),
            is_vegetarian: dietary_flag(&analysis_tags, "en:vegetarian", "en:non-vegetarian"),
            is_palm_oil_free: dietary_flag(&analysis_tags, "en:palm-oil-free", "en:palm-oil"),
            category: extract_category(product_obj),
            categories_tags: categories.as_ref().map(|tags| serde_json::json!(tags)),
            allergens_tags: list("allergens_tags"),
            traces_tags: list("traces_tags"),
            labels_tags: list("labels_tags"),
            nutrient_levels: crate::services::openfoodfacts::nutrient_levels(
                product_obj,
                &nutrition_facts,
                is_beverage,
            ),
            serving_size: text("serving_size"),
            serving_quantity: product_obj.get("serving_quantity").and_then(|v| match v {
                serde_json::Value::Number(n) => n.as_f64(),
                serde_json::Value::String(s) => s.trim().parse().ok(),
                _ => None,
            }),
            quantity: text("quantity"),
            nutriscore_score: product_obj
                .get("nutriscore_score")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32),
            ecoscore_grade: text("ecoscore_grade"),
            completeness: product_obj.get("completeness").and_then(|v| v.as_f64()).map(|v| v as f32),
            last_modified: product_obj.get("last_modified_t").and_then(|v| v.as_i64()),
        }
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
    fn maps_every_column_we_store_from_one_product() {
        let product = json!({
            "product_name": "  Aloo Bhujia  ",
            "brands": "Haldiram's",
            "image_url": "https://images.example/front.jpg",
            "nutriments": { "energy-kcal_100g": 546, "sugars": 2.4, "sodium_100g": 1.18 },
            "ingredients_text": "Gram flour, palm oil, salt",
            "allergens": "en:peanuts",
            "additives_tags": ["en:e330"],
            "nova_group": 4,
            "nutriscore_grade": "d",
            "ingredients_analysis_tags": ["en:vegan", "en:palm-oil"],
            "categories_tags": ["en:snacks", "en:namkeen"],
            "last_modified_t": 1_700_000_000
        });

        let off = OffFields::from(&product);
        assert_eq!(off.product_name, "Aloo Bhujia");
        assert_eq!(off.nutrition_facts["energy_kcal"], json!(546.0));
        assert_eq!(off.nutrition_facts["sugar"], json!(2.4));
        assert_eq!(off.nutrition_facts["sodium"], json!(1.18));
        assert_eq!(off.additives, Some(json!(["E330"])));
        assert_eq!(off.category.as_deref(), Some("namkeen"));
        assert_eq!(off.is_vegan, Some(true));
        assert_eq!(off.is_palm_oil_free, Some(false));
        assert_eq!(off.last_modified, Some(1_700_000_000));
    }

    #[test]
    fn a_product_with_nothing_on_it_still_stores_every_nutrition_key() {
        let off = OffFields::from(&json!({}));
        // "Unknown" rather than an empty name, and thirteen nulls rather than
        // an empty document, so the client can tell "not published" from
        // "we failed to map it".
        assert_eq!(off.product_name, "Unknown");
        assert_eq!(off.nutrition_facts.as_object().unwrap().len(), 13);
        assert!(off.nutrition_facts["fat"].is_null());
        assert!(off.brand.is_none());
        assert!(off.last_modified.is_none());
    }

    #[test]
    fn a_blank_name_is_not_a_name() {
        let off = OffFields::from(&json!({ "product_name": "   ", "brands": "" }));
        assert_eq!(off.product_name, "Unknown");
        assert!(off.brand.is_none());
    }

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
