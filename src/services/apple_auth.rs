use crate::error::{AppError, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const APPLE_KEYS_URL: &str = "https://appleid.apple.com/auth/keys";
const APPLE_ISSUER: &str = "https://appleid.apple.com";
/// Apple's signing keys rotate slowly; re-fetching them on every sign-in
/// would be a network round trip on the critical path for no benefit.
const KEY_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Deserialize)]
struct AppleKey {
    kid: String,
    n: String,
    e: String,
}

#[derive(Debug, Deserialize)]
struct AppleKeys {
    keys: Vec<AppleKey>,
}

/// What we trust from a verified token. Apple only sends `email` on the very
/// first authorization for an app, and the user can choose to hide it, so it
/// is optional and must never be treated as an identifier — `sub` is.
#[derive(Debug, Deserialize)]
pub struct AppleClaims {
    pub sub: String,
    pub email: Option<String>,
}

pub struct AppleAuth {
    client: reqwest::Client,
    bundle_id: String,
    cache: RwLock<Option<(Vec<AppleKey>, Instant)>>,
}

impl AppleAuth {
    pub fn new(bundle_id: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .unwrap(),
            bundle_id,
            cache: RwLock::new(None),
        }
    }

    /// Verifies an identity token end to end: Apple's signature over Apple's
    /// published key, issued by Apple, for *this* app, and not expired.
    /// Anything less would let a caller claim any account by inventing a
    /// token, since this endpoint is otherwise unauthenticated.
    #[tracing::instrument(skip_all)]
    pub async fn verify(&self, identity_token: &str) -> Result<AppleClaims> {
        let header = decode_header(identity_token)
            .map_err(|_| AppError::InvalidRequest("malformed identity token".to_string()))?;
        let kid = header
            .kid
            .ok_or_else(|| AppError::InvalidRequest("identity token has no key id".to_string()))?;

        let key = match self.key(&kid, false).await? {
            Some(key) => key,
            // An unknown kid means Apple rotated keys since we cached them.
            None => self
                .key(&kid, true)
                .await?
                .ok_or_else(|| AppError::InvalidRequest("unknown signing key".to_string()))?,
        };

        let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[APPLE_ISSUER]);
        validation.set_audience(&[&self.bundle_id]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

        let token = decode::<AppleClaims>(identity_token, &decoding_key, &validation)
            .map_err(|e| {
                tracing::warn!(error = %e, "rejected Apple identity token");
                AppError::InvalidRequest("identity token failed verification".to_string())
            })?;

        Ok(token.claims)
    }

    async fn key(&self, kid: &str, force_refresh: bool) -> Result<Option<AppleKey>> {
        if !force_refresh
            && let Ok(guard) = self.cache.read()
            && let Some((keys, fetched)) = guard.as_ref()
            && fetched.elapsed() < KEY_TTL
            && let Some(key) = keys.iter().find(|k| k.kid == kid)
        {
            return Ok(Some(key.clone()));
        }

        let keys = self.fetch_keys().await?;
        let found = keys.iter().find(|k| k.kid == kid).cloned();
        if let Ok(mut guard) = self.cache.write() {
            *guard = Some((keys, Instant::now()));
        }
        Ok(found)
    }

    async fn fetch_keys(&self) -> Result<Vec<AppleKey>> {
        let response = self
            .client
            .get(APPLE_KEYS_URL)
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Apple keys unreachable: {e}")))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalApi(format!(
                "Apple keys returned HTTP {}",
                response.status()
            )));
        }

        let keys: AppleKeys = response
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(e.to_string()))?;
        Ok(keys.keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_a_token_that_is_not_a_token() {
        let auth = AppleAuth::new("com.example.app".to_string());
        assert!(auth.verify("not-a-jwt").await.is_err());
        assert!(auth.verify("").await.is_err());
    }

    #[tokio::test]
    async fn rejects_a_well_formed_token_with_no_key_id() {
        // Header {"alg":"RS256"} with no kid, so verification stops before
        // any network call — an unsigned token can never be accepted.
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMDAxIn0.c2ln";
        let auth = AppleAuth::new("com.example.app".to_string());
        assert!(auth.verify(token).await.is_err());
    }
}
