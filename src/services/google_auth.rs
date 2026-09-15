use crate::error::{AppError, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const GOOGLE_KEYS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
/// Google's tokens carry either form depending on how the token was minted;
/// both are accepted rather than picking one.
const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];
/// Mirrors `AppleAuth`'s key cache TTL — Google's signing keys rotate on a
/// similarly slow, published schedule.
const KEY_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Deserialize)]
struct GoogleKey {
    kid: String,
    n: String,
    e: String,
}

#[derive(Debug, Deserialize)]
struct GoogleKeys {
    keys: Vec<GoogleKey>,
}

/// What we trust from a verified token. `sub` is Google's stable, opaque
/// per-account identifier — the only field ever used as an identity key.
#[derive(Debug, Deserialize)]
pub struct GoogleClaims {
    pub sub: String,
    pub email: Option<String>,
}

/// Mirrors `AppleAuth` as a sibling implementation, not a shared
/// abstraction — two similar, boring code paths, not worth generalizing
/// for two providers.
pub struct GoogleAuth {
    client: reqwest::Client,
    /// The **Web application** OAuth client ID from Google Cloud Console —
    /// not the separate Android-type client, which has no secret and never
    /// reaches the backend. `None` means Google sign-in isn't set up yet;
    /// `verify` refuses cleanly rather than the process failing to boot.
    web_client_id: Option<String>,
    cache: RwLock<Option<(Vec<GoogleKey>, Instant)>>,
}

impl GoogleAuth {
    pub fn new(web_client_id: Option<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .unwrap(),
            web_client_id,
            cache: RwLock::new(None),
        }
    }

    /// Verifies an ID token end to end: Google's signature over Google's
    /// published key, issued by Google, for this app's web client, and not
    /// expired. Anything less would let a caller claim any account by
    /// inventing a token, since this endpoint is otherwise unauthenticated.
    #[tracing::instrument(skip_all)]
    pub async fn verify(&self, id_token: &str) -> Result<GoogleClaims> {
        let web_client_id = self.web_client_id.as_deref().ok_or_else(|| {
            AppError::InvalidRequest("Google sign-in isn't configured on this server".to_string())
        })?;

        let header = decode_header(id_token)
            .map_err(|_| AppError::InvalidRequest("malformed identity token".to_string()))?;
        let kid = header
            .kid
            .ok_or_else(|| AppError::InvalidRequest("identity token has no key id".to_string()))?;

        let key = match self.key(&kid, false).await? {
            Some(key) => key,
            // An unknown kid means Google rotated keys since we cached them.
            None => self
                .key(&kid, true)
                .await?
                .ok_or_else(|| AppError::InvalidRequest("unknown signing key".to_string()))?,
        };

        let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&GOOGLE_ISSUERS);
        validation.set_audience(&[web_client_id]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

        let token = decode::<GoogleClaims>(id_token, &decoding_key, &validation).map_err(|e| {
            tracing::warn!(error = %e, "rejected Google identity token");
            AppError::InvalidRequest("identity token failed verification".to_string())
        })?;

        Ok(token.claims)
    }

    async fn key(&self, kid: &str, force_refresh: bool) -> Result<Option<GoogleKey>> {
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

    async fn fetch_keys(&self) -> Result<Vec<GoogleKey>> {
        let response = self
            .client
            .get(GOOGLE_KEYS_URL)
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Google keys unreachable: {e}")))?;

        if !response.status().is_success() {
            return Err(AppError::ExternalApi(format!(
                "Google keys returned HTTP {}",
                response.status()
            )));
        }

        // Google's JWK Set includes extra fields (alg, use, kty) Apple's
        // doesn't send; serde ignores what GoogleKey doesn't declare.
        let keys: GoogleKeys = response
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
        let auth = GoogleAuth::new(Some("test-client-id".to_string()));
        assert!(auth.verify("not-a-jwt").await.is_err());
        assert!(auth.verify("").await.is_err());
    }

    #[tokio::test]
    async fn rejects_a_well_formed_token_with_no_key_id() {
        // Header {"alg":"RS256"} with no kid, so verification stops before
        // any network call — an unsigned token can never be accepted.
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMDAxIn0.c2ln";
        let auth = GoogleAuth::new(Some("test-client-id".to_string()));
        assert!(auth.verify(token).await.is_err());
    }

    #[tokio::test]
    async fn refuses_cleanly_when_unconfigured() {
        let auth = GoogleAuth::new(None);
        assert!(auth.verify("anything").await.is_err());
    }
}
