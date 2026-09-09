use crate::error::{AppError, Result};
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

pub struct OffClient {
    client: Client,
    base_url: String,
    rate_limiter: OffRateLimiter,
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
            rate_limiter: RateLimiter::direct(Quota::per_minute(requests_per_minute)),
        }
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

impl Default for OffClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
