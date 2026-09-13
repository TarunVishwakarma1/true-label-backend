use crate::error::{AppError, Result};
use crate::models::ProductCard;
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
            let nutriments = p.get("nutriments").cloned().unwrap_or(Value::Null);
            let num = |key: &str| nutriments.get(key).and_then(Value::as_f64);
            Some(ProductCard {
                barcode,
                product_name: name,
                brand: p.get("brands").and_then(Value::as_str).map(str::to_string),
                image_url: p.get("image_url").and_then(Value::as_str).map(str::to_string),
                nutriscore_grade: p.get("nutriscore_grade").and_then(Value::as_str).map(str::to_string),
                nova_group: p.get("nova_group").and_then(Value::as_i64).map(|n| n as i16),
                verified: false,
                energy_kcal: num("energy-kcal_100g"),
                sugar: num("sugars_100g"),
                sodium: num("sodium_100g"),
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
