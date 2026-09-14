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

/// A signed-in dashboard staff account — a different identity system from
/// `Device` above (named accounts with a password, not anonymous devices),
/// but the same bearer-token mechanics: only the hash is stored, and the
/// token never appears anywhere but the one response that issues it.
#[derive(Debug, Clone)]
pub struct AdminUser {
    pub id: uuid::Uuid,
    pub name: String,
    pub role: String,
}

impl AdminUser {
    /// Some actions (publishing a GitHub issue) are for admins specifically,
    /// not every signed-in staff account.
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.role == "admin" {
            Ok(())
        } else {
            Err(AppError::Forbidden("admin role required".to_string()))
        }
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;

        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "SELECT id, name, role FROM dashboard_users WHERE token_hash = $1",
        )
        .bind(hash_token(&token))
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        match row {
            Some((id, name, role)) => Ok(AdminUser { id, name, role }),
            None => {
                tracing::warn!("rejected a request carrying an unknown dashboard session token");
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
        Ok(ClientIp(client_ip(parts, state.config.trusted_proxy_hops)))
    }
}

/// Who to rate-limit when nobody is authenticated.
///
/// `X-Forwarded-For` is a list each proxy appends to, so **everything to the
/// left of what our own proxies added is written by the caller**. Reading the
/// leftmost entry — the obvious thing, and what this did first — means a
/// client sends `X-Forwarded-For: <anything>` and picks its own rate-limit
/// bucket on every request. Trusting the header at all is only safe if you
/// know how many hops of it are yours.
///
/// So `hops` is the number of proxies in front of this process
/// (`TRUSTED_PROXY_HOPS`), and the client is the entry that many places from
/// the right — the address our outermost trusted proxy actually observed.
/// Zero, the default, ignores the header entirely and uses the socket's peer
/// address, which no caller can forge.
pub fn client_ip(parts: &Parts, hops: usize) -> String {
    let peer = || {
        parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    if hops == 0 {
        return peer();
    }

    let Some(forwarded) = parts.headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) else {
        return peer();
    };
    let entries: Vec<&str> = forwarded.split(',').map(str::trim).filter(|e| !e.is_empty()).collect();

    // Fewer entries than hops means the header did not come through the
    // proxies we expect. Trusting it then would be trusting the caller.
    let Some(index) = entries.len().checked_sub(hops) else {
        tracing::warn!(entries = entries.len(), hops, "forwarded header shorter than the trusted hop count");
        return peer();
    };

    match entries.get(index).and_then(|e| e.parse::<IpAddr>().ok()) {
        Some(ip) => ip.to_string(),
        None => peer(),
    }
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

    fn forwarded(value: &str) -> Parts {
        let mut request = Request::new(());
        request
            .headers_mut()
            .insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        request.into_parts().0
    }

    #[test]
    fn a_caller_cannot_choose_its_own_rate_limit_bucket() {
        // One proxy in front. It appended 203.0.113.9, the address it really
        // saw. Everything left of that is text the caller sent, and picking
        // the leftmost entry would hand them a fresh bucket per request.
        let spoofed = forwarded("1.1.1.1, 2.2.2.2, 203.0.113.9");
        assert_eq!(client_ip(&spoofed, 1), "203.0.113.9");

        // Two proxies: ours appended 10.0.0.1, the one in front of it
        // appended the real client.
        let two = forwarded("9.9.9.9, 203.0.113.9, 10.0.0.1");
        assert_eq!(client_ip(&two, 2), "203.0.113.9");
    }

    #[test]
    fn zero_hops_ignores_the_header_entirely() {
        assert_eq!(client_ip(&forwarded("203.0.113.9"), 0), "unknown");
        assert_eq!(client_ip(&parts_with(None), 0), "unknown");
    }

    #[test]
    fn a_header_shorter_than_our_hops_is_not_from_our_proxies() {
        // Somebody reaching the process directly, past the ingress.
        assert_eq!(client_ip(&forwarded("203.0.113.9"), 2), "unknown");
    }

    #[test]
    fn a_forwarded_entry_that_is_not_an_address_is_refused() {
        assert_eq!(client_ip(&forwarded("not-an-ip"), 1), "unknown");
    }
}
