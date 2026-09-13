use crate::error::{AppError, Result};
use crate::models::ProductCard;
use serde_json::{Map, json};
use governor::{Quota, RateLimiter};
use reqwest::Client;
use serde_json::Value;
use std::num::NonZeroU32;
use std::time::Duration;

type OffRateLimiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    governor::clock::DefaultClock,
>;

/// Open Food Facts caps product-lookup requests at 15/min **per IP** — and
/// since every user's "new barcode" lookup goes through this one backend
/// process, that's a ceiling on our *entire* combined traffic, not per user.
/// Exceeding it risks an outright IP ban, not just a slow response. Staying
/// under 12/min leaves headroom for clock/measurement drift between our
/// limiter and theirs. Doesn't help with repeat lookups of the same
/// barcode — those never reach this client at all, since ProductService
/// only calls it on a genuine cache-and-DB miss.
const DEFAULT_MAX_REQUESTS_PER_MINUTE: u32 = 12;

/// How long a request will wait for a rate-limit slot before giving up.
/// Long enough to smooth out a short burst, short enough that a scan
/// doesn't hang indefinitely under sustained heavy load.
const RATE_LIMIT_WAIT_TIMEOUT: Duration = Duration::from_secs(8);

/// OFF's search endpoint has a separate, tighter budget (10/min per IP).
/// Searches never queue behind it — a throttled search just returns nothing
/// from OFF and the caller shows local results.
const SEARCH_REQUESTS_PER_MINUTE: u32 = 8;

pub struct OffClient {
    client: Client,
    base_url: String,
    search_url: String,
    rate_limiter: OffRateLimiter,
    search_limiter: OffRateLimiter,
}

impl OffClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("TrueLabel/0.1.0 (+https://truelabel.app)")
            .build()
            .unwrap();

        let requests_per_minute = std::env::var("OFF_RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .and_then(NonZeroU32::new)
            .unwrap_or_else(|| NonZeroU32::new(DEFAULT_MAX_REQUESTS_PER_MINUTE).unwrap());

        Self {
            client,
            base_url: "https://world.openfoodfacts.org/api/v3".to_string(),
            search_url: "https://world.openfoodfacts.org/cgi/search.pl".to_string(),
            rate_limiter: RateLimiter::direct(Quota::per_minute(requests_per_minute)),
            search_limiter: RateLimiter::direct(Quota::per_minute(
                NonZeroU32::new(SEARCH_REQUESTS_PER_MINUTE).unwrap(),
            )),
        }
    }

    /// Free-text product search. Returns an empty list (not an error) when
    /// the search budget is spent, so callers never block on it.
    #[tracing::instrument(skip(self))]
    pub async fn search(&self, term: &str) -> Result<Vec<ProductCard>> {
        if self.search_limiter.check().is_err() {
            tracing::warn!("search rate limit reached, skipping Open Food Facts");
            return Ok(Vec::new());
        }

        let url = reqwest::Url::parse_with_params(
            &self.search_url,
            &[
                ("search_terms", term),
                ("search_simple", "1"),
                ("action", "process"),
                ("json", "1"),
                ("page_size", "10"),
                ("fields", "code,product_name,brands,image_url,nutriscore_grade,nova_group,nutriments"),
            ],
        )
        .map_err(|e| AppError::ExternalApi(e.to_string()))?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalApi(format!("HTTP {}", response.status())));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(e.to_string()))?;

        Ok(parse_search_results(&json))
    }

    /// For background refreshes: gives up immediately rather than queueing,
    /// because the per-minute budget is shared with live scans and a refresh
    /// is never the urgent request. `None` means "nothing to do" for either
    /// reason, which is all a refresh needs to know.
    #[tracing::instrument(skip(self), fields(barcode = %barcode, country = %country))]
    pub async fn get_product_if_free(&self, barcode: &str, country: &str) -> Option<Value> {
        if self.rate_limiter.check().is_err() {
            tracing::debug!("no spare budget, skipping refresh");
            return None;
        }
        self.get_product(barcode, country).await.ok().flatten()
    }

    #[tracing::instrument(skip(self), fields(barcode = %barcode, country = %country))]
    pub async fn get_product(&self, barcode: &str, country: &str) -> Result<Option<Value>> {
        // A ready slot resolves immediately; only log when we actually had
        // to queue behind the rate limit, so routine traffic doesn't spam
        // the log with a line that's true (and uninteresting) every time.
        if self.rate_limiter.check().is_err() {
            tracing::warn!("rate limit reached, request queued");
        }

        tokio::time::timeout(RATE_LIMIT_WAIT_TIMEOUT, self.rate_limiter.until_ready())
            .await
            .map_err(|_| {
                tracing::error!("gave up waiting for a rate-limit slot after {:?}", RATE_LIMIT_WAIT_TIMEOUT);
                AppError::ExternalApi(
                    "Open Food Facts is receiving high demand right now — try again shortly"
                        .to_string(),
                )
            })?;

        let url = format!("{}/product/{}?country={}", self.base_url, barcode, country);
        let started = std::time::Instant::now();

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "request to Open Food Facts failed");
                AppError::ExternalApi(e.to_string())
            })?;

        let status = response.status();
        tracing::info!(status = %status, elapsed_ms = started.elapsed().as_millis(), "Open Food Facts responded");

        if status == 404 {
            return Ok(None);
        }

        if !status.is_success() {
            tracing::error!(status = %status, "Open Food Facts returned a non-success status");
            return Err(AppError::ExternalApi(format!("HTTP {status}")));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to decode Open Food Facts response body");
                AppError::ExternalApi(e.to_string())
            })?;

        Ok(Some(json))
    }
}

/// Our nutrition key on the left, Open Food Facts' on the right. One table,
/// so the scan path and the search path can never read a product differently
/// from each other again — which they did: scans read the bare keys and
/// searches read `_100g`.
const NUTRIENTS: &[(&str, &str)] = &[
    ("energy_kcal", "energy-kcal"),
    ("protein", "proteins"),
    ("carbs", "carbohydrates"),
    ("fat", "fat"),
    ("saturated_fat", "saturated-fat"),
    ("trans_fat", "trans-fat"),
    ("fiber", "fiber"),
    ("sugar", "sugars"),
    ("sodium", "sodium"),
    ("cholesterol", "cholesterol"),
    ("potassium", "potassium"),
    ("calcium", "calcium"),
    ("iron", "iron"),
];

/// Every figure this app shows is labelled "per 100 g", so it has to actually
/// be per 100 g. Open Food Facts publishes each nutrient several ways:
/// `sugars_100g` is the normalised per-100g figure, the bare `sugars` is
/// historical and not guaranteed to be normalised, and `sugars_value` is the
/// number as printed in whatever unit `sugars_unit` names. Only the first can
/// be trusted, and the others are fallbacks for payloads that predate it.
///
/// Reading the bare key first — which the scan path did — both mislabels
/// per-serving figures as per-100g and drops nutrients that Open Food Facts
/// only publishes under `_100g`.
pub fn per_100g(nutriments: &Value, key: &str) -> Option<f64> {
    for candidate in [format!("{key}_100g"), key.to_string(), format!("{key}_value")] {
        if let Some(value) = number(nutriments.get(&candidate)) {
            return plausible(key, value);
        }
    }
    None
}

/// The `nutrition_facts` document as `products` stores it.
pub fn nutriments(product: &Value) -> Value {
    let source = product.get("nutriments").cloned().unwrap_or(Value::Null);
    let mut out = Map::with_capacity(NUTRIENTS.len());
    for (ours, theirs) in NUTRIENTS {
        let value = per_100g(&source, theirs).map(|v| json!(v)).unwrap_or(Value::Null);
        out.insert((*ours).to_string(), value);
    }
    Value::Object(out)
}

/// What a nutrient can physically be, per 100 g. Open Food Facts is
/// crowd-edited and does contain typos — a sugar figure of 1000 g per 100 g
/// is not a number to render, it is a data-entry error, and showing it would
/// be worse than showing nothing.
fn plausible(key: &str, value: f64) -> Option<f64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let ceiling = match key {
        // No food is more than 900 kcal per 100 g; pure fat is about 900.
        "energy-kcal" => 900.0,
        // Nothing can be more than 100 g of anything per 100 g.
        _ => 100.0,
    };
    (value <= ceiling).then_some(value)
}

/// Open Food Facts occasionally ships a number as a string.
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Scale a per-100g document to one serving. Arithmetic only — nothing is
/// invented, and a product without a usable serving quantity simply has no
/// per-serving figures rather than a guessed one.
pub fn per_serving(facts: &Value, serving_quantity: Option<f64>) -> Option<Value> {
    let grams = serving_quantity.filter(|g| *g > 0.0 && *g <= 2000.0)?;
    let source = facts.as_object()?;
    let mut out = Map::with_capacity(source.len());
    for (key, value) in source {
        let scaled = value.as_f64().map(|v| (v * grams / 100.0 * 100.0).round() / 100.0);
        out.insert(key.clone(), scaled.map(|v| json!(v)).unwrap_or(Value::Null));
    }
    Some(Value::Object(out))
}

/// UK FSA front-of-pack thresholds, per 100 g, as published. Drinks get their
/// own lower set because the same sugar figure means something different in a
/// bottle than in a biscuit. Only used when Open Food Facts has not published
/// its own `nutrient_levels` — theirs is preferred, since matching the source
/// matters more than matching our own arithmetic.
struct Thresholds {
    low: f64,
    high: f64,
}

fn thresholds(nutrient: &str, is_beverage: bool) -> Option<Thresholds> {
    let (low, high) = match (nutrient, is_beverage) {
        ("fat", false) => (3.0, 17.5),
        ("fat", true) => (1.5, 8.75),
        ("saturated_fat", false) => (1.5, 5.0),
        ("saturated_fat", true) => (0.75, 2.5),
        ("sugar", false) => (5.0, 22.5),
        ("sugar", true) => (2.5, 11.25),
        // Salt 0.3 / 1.5 g, expressed as sodium, which is how we store it.
        ("sodium", _) => (0.12, 0.6),
        _ => return None,
    };
    Some(Thresholds { low, high })
}

/// `{"fat": "low", "sugar": "high", ...}` — the same shape Open Food Facts
/// publishes, keyed the way we store nutrients.
pub fn nutrient_levels(product: &Value, facts: &Value, is_beverage: bool) -> Value {
    const THEIRS: &[(&str, &str)] = &[
        ("fat", "fat"),
        ("saturated_fat", "saturated-fat"),
        ("sugar", "sugars"),
        ("sodium", "salt"),
    ];

    let published = product.get("nutrient_levels");
    let mut out = Map::new();

    for (ours, theirs) in THEIRS {
        // Prefer what they publish; they account for category rules we do not.
        if let Some(level) = published
            .and_then(|l| l.get(*theirs))
            .and_then(Value::as_str)
            .filter(|l| matches!(*l, "low" | "moderate" | "high"))
        {
            out.insert((*ours).to_string(), json!(level));
            continue;
        }

        let Some(value) = facts.get(*ours).and_then(Value::as_f64) else { continue };
        let Some(t) = thresholds(ours, is_beverage) else { continue };
        let level = if value <= t.low {
            "low"
        } else if value > t.high {
            "high"
        } else {
            "moderate"
        };
        out.insert((*ours).to_string(), json!(level));
    }

    Value::Object(out)
}

/// Open Food Facts tags carry a locale prefix: `"en:peanuts"`. The prefix is
/// theirs, the slug is the stable identifier, and the slug is what travels —
/// a client can both match on it and prettify it, which a display string
/// cannot do. `None` rather than `[]` when the field is absent: "not
/// published" and "declared none" are different claims about food.
pub fn tags(product: &Value, key: &str) -> Option<Vec<String>> {
    let raw = product.get(key)?.as_array()?;
    Some(
        raw.iter()
            .filter_map(Value::as_str)
            .map(|tag| tag.split_once(':').map_or(tag, |(_, rest)| rest).trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect(),
    )
}

/// Pull the card fields out of OFF's search payload. Entries without a code
/// or a name are useless as cards and are dropped.
fn parse_search_results(json: &Value) -> Vec<ProductCard> {
    let Some(products) = json.get("products").and_then(Value::as_array) else {
        return Vec::new();
    };
    products
        .iter()
        .filter_map(|p| {
            let barcode = p.get("code")?.as_str()?.to_string();
            let name = p.get("product_name")?.as_str()?.trim().to_string();
            if name.is_empty() || barcode.len() < 8 || barcode.len() > 14 {
                return None;
            }
            let facts = p.get("nutriments").cloned().unwrap_or(Value::Null);
            let num = |key: &str| per_100g(&facts, key);
            Some(ProductCard {
                barcode,
                product_name: name,
                brand: p.get("brands").and_then(Value::as_str).map(str::to_string),
                image_url: p.get("image_url").and_then(Value::as_str).map(str::to_string),
                nutriscore_grade: p.get("nutriscore_grade").and_then(Value::as_str).map(str::to_string),
                nova_group: p.get("nova_group").and_then(Value::as_i64).map(|n| n as i16),
                verified: false,
                energy_kcal: num("energy-kcal"),
                sugar: num("sugars"),
                sodium: num("sodium"),
                sort_value: None,
            })
        })
        .collect()
}

impl Default for OffClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_the_normalised_per_100g_figure() {
        // The pack says 21 g per serving; per 100 g it is 42. Reading the
        // bare key would label the serving figure "per 100 g".
        let n = serde_json::json!({
            "sugars": 21, "sugars_100g": 42.0, "sugars_serving": 21, "sugars_value": 21
        });
        assert_eq!(per_100g(&n, "sugars"), Some(42.0));
    }

    #[test]
    fn falls_back_only_when_the_normalised_figure_is_absent() {
        let bare = serde_json::json!({ "fat": 9.5 });
        assert_eq!(per_100g(&bare, "fat"), Some(9.5));

        let printed = serde_json::json!({ "iron": "0.004" });
        assert_eq!(per_100g(&printed, "iron"), Some(0.004));

        assert_eq!(per_100g(&serde_json::json!({}), "fat"), None);
    }

    #[test]
    fn a_typo_is_dropped_rather_than_rendered() {
        // Open Food Facts is crowd-edited. Nothing can be 1000 g per 100 g,
        // and showing it would be worse than showing nothing.
        assert_eq!(per_100g(&serde_json::json!({ "sugars_100g": 1000 }), "sugars"), None);
        assert_eq!(per_100g(&serde_json::json!({ "fat_100g": -3 }), "fat"), None);
        assert_eq!(
            per_100g(&serde_json::json!({ "energy-kcal_100g": 4000 }), "energy-kcal"),
            None
        );
        // The edge of possible is still possible: pure fat is ~900 kcal.
        assert_eq!(
            per_100g(&serde_json::json!({ "energy-kcal_100g": 900 }), "energy-kcal"),
            Some(900.0)
        );
    }

    #[test]
    fn tags_lose_their_locale_prefix_and_keep_their_slug() {
        let product = serde_json::json!({ "labels_tags": ["en:Gluten-Free", "fr:bio", "organic"] });
        assert_eq!(
            tags(&product, "labels_tags"),
            Some(vec!["gluten-free".to_string(), "bio".to_string(), "organic".to_string()])
        );
        // Absent is not the same claim as empty.
        assert_eq!(tags(&product, "traces_tags"), None);
        assert_eq!(
            tags(&serde_json::json!({ "traces_tags": [] }), "traces_tags"),
            Some(vec![])
        );
    }

    #[test]
    fn published_levels_win_over_our_own_arithmetic() {
        let product = serde_json::json!({ "nutrient_levels": { "sugars": "high", "salt": "low" } });
        let facts = serde_json::json!({ "sugar": 1.0, "sodium": 5.0 });
        let levels = nutrient_levels(&product, &facts, false);
        // Ours would have said "low" for that sugar figure. Theirs wins,
        // because matching the source matters more than matching ourselves.
        assert_eq!(levels["sugar"], serde_json::json!("high"));
        assert_eq!(levels["sodium"], serde_json::json!("low"));
    }

    #[test]
    fn we_fall_back_to_the_fsa_thresholds_and_drinks_get_their_own() {
        let facts = serde_json::json!({ "sugar": 12.0, "fat": 2.0, "saturated_fat": 6.0, "sodium": 0.05 });
        let food = nutrient_levels(&serde_json::json!({}), &facts, false);
        assert_eq!(food["sugar"], serde_json::json!("moderate")); // 5 < 12 <= 22.5
        assert_eq!(food["fat"], serde_json::json!("low")); // <= 3
        assert_eq!(food["saturated_fat"], serde_json::json!("high")); // > 5
        assert_eq!(food["sodium"], serde_json::json!("low")); // <= 0.12

        // 12 g of sugar is moderate in a biscuit and high in a bottle, which
        // is the whole reason drinks get their own thresholds.
        let drink = nutrient_levels(&serde_json::json!({}), &facts, true);
        assert_eq!(drink["sugar"], serde_json::json!("high")); // > 11.25
    }

    #[test]
    fn per_serving_scales_and_refuses_to_guess() {
        let facts = serde_json::json!({ "sugar": 40.0, "fat": 10.0, "protein": Value::Null });
        let scaled = per_serving(&facts, Some(30.0)).unwrap();
        assert_eq!(scaled["sugar"], serde_json::json!(12.0));
        assert_eq!(scaled["fat"], serde_json::json!(3.0));
        assert!(scaled["protein"].is_null());

        // No serving size means no per-serving figures, not invented ones.
        assert!(per_serving(&facts, None).is_none());
        assert!(per_serving(&facts, Some(0.0)).is_none());
    }

    #[test]
    fn nutriments_always_carries_every_key() {
        let facts = nutriments(&serde_json::json!({ "nutriments": { "energy-kcal_100g": 546 } }));
        let object = facts.as_object().unwrap();
        assert_eq!(object.len(), NUTRIENTS.len());
        assert_eq!(object["energy_kcal"], serde_json::json!(546.0));
        // Absent stays null rather than disappearing, so the client can tell
        // "not published" from "we forgot to map it".
        assert!(object["sugar"].is_null());
    }

    #[test]
    fn parses_search_results_and_drops_junk() {
        let json = serde_json::json!({"products": [
            {"code": "8901030895564", "product_name": "Aloo Bhujia", "brands": "Haldiram's",
             "nutriscore_grade": "d", "nova_group": 4, "nutriments": {"energy-kcal_100g": 546, "sugars_100g": 2.4}},
            {"code": "123", "product_name": "Too short"},
            {"code": "8901030895565", "product_name": "   "}
        ]});
        let cards = parse_search_results(&json);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].product_name, "Aloo Bhujia");
        assert_eq!(cards[0].energy_kcal, Some(546.0));
        assert_eq!(cards[0].nova_group, Some(4));
    }

    #[test]
    fn rate_limiter_throttles_once_quota_is_exhausted() {
        let limiter: OffRateLimiter = RateLimiter::direct(Quota::per_minute(NonZeroU32::new(2).unwrap()));

        assert!(limiter.check().is_ok(), "first request should have a free slot");
        assert!(limiter.check().is_ok(), "second request should have a free slot");
        assert!(
            limiter.check().is_err(),
            "third immediate request must be throttled, not silently allowed through"
        );
    }
}
