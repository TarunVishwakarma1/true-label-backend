use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub device_id: String,
    pub country: String,
    pub dietary_preferences: Value,
    pub plus_since: Option<DateTime<Utc>>,
    pub plus_expires_at: Option<DateTime<Utc>>,
    pub plus_source: Option<String>,
    pub apple_user_id: Option<String>,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub linked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub device_id: String,
    pub country: String,
    pub dietary_preferences: Vec<String>,
    pub subscription: SubscriptionResponse,
    pub identity: IdentityResponse,
    pub created_at: DateTime<Utc>,
}

/// Who this is, when they've said. Signed out is the normal state: the app
/// works fully without an account, and signing in only makes the profile
/// portable to another phone.
#[derive(Debug, Serialize)]
pub struct IdentityResponse {
    pub signed_in: bool,
    pub provider: Option<String>,
    /// Apple only sends this on the first authorization, and the user may
    /// hide it, so it is often absent even when signed in.
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub linked_at: Option<DateTime<Utc>>,
}

impl From<&User> for IdentityResponse {
    fn from(u: &User) -> Self {
        Self {
            signed_in: u.apple_user_id.is_some(),
            provider: u.apple_user_id.as_ref().map(|_| "apple".to_string()),
            email: u.email.clone(),
            display_name: u.display_name.clone(),
            linked_at: u.linked_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    /// `"free"` or `"plus"` — what the client should unlock.
    pub tier: String,
    pub active: bool,
    pub since: Option<DateTime<Utc>>,
    /// `None` while Plus is complimentary: on, with no expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// How it was granted: `"complimentary"` today, `"paid"` once payments
    /// exist. Kept separate from `tier` so the client can say why.
    pub source: Option<String>,
}

impl From<User> for ProfileResponse {
    fn from(u: User) -> Self {
        let subscription = SubscriptionResponse::from(&u);
        let identity = IdentityResponse::from(&u);
        Self {
            device_id: u.device_id,
            country: u.country,
            dietary_preferences: u
                .dietary_preferences
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            subscription,
            identity,
            created_at: u.created_at,
        }
    }
}

impl From<&User> for SubscriptionResponse {
    fn from(u: &User) -> Self {
        // Never expired if there is no expiry — that is what the free period
        // looks like, and it must not read as "lapsed".
        let active = u.plus_since.is_some()
            && u.plus_expires_at.map(|exp| exp > Utc::now()).unwrap_or(true);
        Self {
            tier: if active { "plus" } else { "free" }.to_string(),
            active,
            since: u.plus_since,
            expires_at: u.plus_expires_at,
            source: u.plus_source.clone(),
        }
    }
}

/// Handed out once, at first launch, and never again. The token is the only
/// copy — it is stored hashed, so it cannot be re-read or re-sent.
#[derive(Debug, Serialize)]
pub struct DeviceRegistration {
    pub device_id: String,
    pub token: String,
}

/// What this device has actually put into the catalogue. Deliberately about
/// contribution rather than consumption: how much someone scans is their own
/// business and never leaves their phone.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ContributionStats {
    /// Labels confirmed against the pack.
    pub confirmations: i64,
    /// Products added from a label photo.
    pub contributions: i64,
    /// Of those confirmations, how many are on products that have since
    /// crossed the threshold and are now verified for everyone.
    pub helped_verify: i64,
    pub member_since: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct LinkAccountRequest {
    /// Apple's signed identity token. Verified against Apple's published
    /// keys before anything is written — this endpoint is otherwise
    /// unauthenticated, so an unverified token would let a caller claim
    /// somebody else's account.
    pub identity_token: String,
    /// Apple hands the name to the client on first authorization only, and
    /// never puts it in the token, so the client forwards it here or it is
    /// lost.
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub country: Option<String>,
    pub dietary_preferences: Option<Vec<String>>,
    /// What to call this person. Settable without an account — a name is not
    /// an identity, and it is the only thing a client can offer when Sign in
    /// with Apple isn't available to it.
    pub display_name: Option<String>,
}

fn default_admin_limit() -> i64 {
    50
}

/// Read-only admin browsing — closes the gap where the only way to answer
/// "why can't this person restore Plus" was a direct `psql` session.
#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    /// Matched against device_id, email, and display_name, case-insensitively.
    pub q: Option<String>,
    /// `true` = currently-active Plus only, `false` = not currently active,
    /// omitted = both. Mirrors `SubscriptionResponse::from`'s exact
    /// "no expiry means still on" rule, computed in SQL so it can't drift.
    pub plus_active: Option<bool>,
    #[serde(default = "default_admin_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminUserPage {
    pub items: Vec<User>,
    pub total: i64,
}

/// The full row plus its contribution numbers in one response — the same
/// "one round trip, not three" reasoning `UserService::stats` already
/// documents applies here too.
#[derive(Debug, Serialize)]
pub struct AdminUserDetail {
    #[serde(flatten)]
    pub user: User,
    pub stats: ContributionStats,
}
