//! Who is calling, and may they call this often.
//!
//! There are no accounts in this app, so a device is the subject. It proves
//! itself with a bearer token issued once at first launch and kept in the
//! Keychain. The device id never appears in a URL again: the token *is* the
//! identity, so there is nothing for a caller to name but itself.

use crate::{error::AppError, state::AppState};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;

/// 244 bits from the OS random source, which is what `Uuid::new_v4` is. A
/// dedicated random crate would add a dependency to do the same thing.
pub fn new_token() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// Only the hash is stored, so a leaked database does not leak credentials.
/// A plain digest rather than a password hash on purpose: the input is 244
/// bits of uniform randomness, so there is no dictionary to attack and
/// nothing for a work factor to buy.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// An authenticated device. Handlers take this instead of reading an id out
/// of the path, which is what made every per-user endpoint impersonatable.
#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
}

impl FromRequestParts<AppState> for Device {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;

        // Looked up by hash, so no secret is ever compared in our own code
        // and there is no timing signal to read.
        let id: Option<String> =
            sqlx::query_scalar("SELECT device_id FROM users WHERE token_hash = $1")
                .bind(hash_token(&token))
                .fetch_optional(&state.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

        match id {
            Some(id) => Ok(Device { id }),
            None => {
                tracing::warn!("rejected a request carrying an unknown device token");
                Err(AppError::Unauthorized)
            }
        }
    }
}

/// For endpoints that work signed out but do better signed in — the
/// verification queue excludes what you have already voted on, and can only
/// do that if it knows who you are.
#[derive(Debug, Clone)]
pub struct MaybeDevice(pub Option<Device>);

impl FromRequestParts<AppState> for MaybeDevice {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(MaybeDevice(Device::from_request_parts(parts, state).await.ok()))
    }
}

fn bearer(parts: &Parts) -> Option<String> {
    let header = parts.headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = header.strip_prefix("Bearer ").or_else(|| header.strip_prefix("bearer "))?;
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// The subject to rate-limit when nobody is authenticated.
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(client_ip(parts, state.config.trust_proxy_headers)))
    }
}

/// Who to rate-limit when nobody is authenticated.
///
/// The socket's peer address is the only thing a caller cannot forge, so it
/// is the default. `X-Forwarded-For` is trusted only when `TRUST_PROXY_HEADERS`
/// says we sit behind a proxy that overwrites it — otherwise any client could
/// hand us a different address per request and lift its own limit.
pub fn client_ip(parts: &Parts, trust_proxy: bool) -> String {
    if trust_proxy
        && let Some(forwarded) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        && forwarded.parse::<IpAddr>().is_ok()
    {
        return forwarded.to_string();
    }

    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Request, header::AUTHORIZATION};

    fn parts_with(header: Option<&str>) -> Parts {
        let mut request = Request::new(());
        if let Some(value) = header {
            request
                .headers_mut()
                .insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        }
        request.into_parts().0
    }

    #[test]
    fn tokens_are_unique_and_long_enough_to_be_unguessable() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn the_token_itself_is_never_what_we_store() {
        let token = new_token();
        let hash = hash_token(&token);
        assert_ne!(hash, token);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_token(&token));
        assert_ne!(hash, hash_token(&new_token()));
    }

    #[test]
    fn only_a_bearer_header_counts() {
        assert_eq!(bearer(&parts_with(Some("Bearer abc123"))).as_deref(), Some("abc123"));
        assert_eq!(bearer(&parts_with(Some("bearer abc123"))).as_deref(), Some("abc123"));
        assert!(bearer(&parts_with(Some("Basic abc123"))).is_none());
        assert!(bearer(&parts_with(Some("Bearer   "))).is_none());
        assert!(bearer(&parts_with(Some("abc123"))).is_none());
        assert!(bearer(&parts_with(None)).is_none());
    }

    #[test]
    fn a_forwarded_address_is_ignored_unless_we_sit_behind_a_proxy() {
        let mut request = Request::new(());
        request
            .headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_static("203.0.113.9, 10.0.0.1"));
        let parts = request.into_parts().0;

        // Without a trusted proxy this is caller-supplied text, and honouring
        // it would let anyone raise their own limit by changing one header.
        assert_eq!(client_ip(&parts, false), "unknown");
        assert_eq!(client_ip(&parts, true), "203.0.113.9");
    }

    #[test]
    fn a_forwarded_header_that_is_not_an_address_is_refused() {
        let mut request = Request::new(());
        request
            .headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        let parts = request.into_parts().0;
        assert_eq!(client_ip(&parts, true), "unknown");
    }
}
