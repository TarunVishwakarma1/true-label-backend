use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A dashboard staff account. Deliberately a separate identity system from
/// `User` (anonymous devices) — a bug in one authority can never leak into
/// the other.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DashboardUser {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub date_of_birth: Option<NaiveDate>,
    pub occupation: Option<String>,
    pub password_hash: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminProfile {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub occupation: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

impl From<DashboardUser> for AdminProfile {
    fn from(u: DashboardUser) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
            occupation: u.occupation,
            role: u.role,
            created_at: u.created_at,
        }
    }
}

/// Handed out once, at registration or at login, and never again — same
/// shape as `DeviceRegistration`: the token is the only copy, and it is
/// stored hashed from the moment it exists.
#[derive(Debug, Serialize)]
pub struct AdminSession {
    pub token: String,
    pub profile: AdminProfile,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: String,
    /// `"YYYY-MM-DD"`. Optional — worth asking for, not worth blocking
    /// registration over if a client omits it.
    pub date_of_birth: Option<String>,
    pub occupation: Option<String>,
    pub password: String,
    // No `confirm_password` here: that comparison belongs to the form, not
    // the wire — the server only ever needs one password.
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

/// Self-service: a signed-in account changing its own password. Requires
/// the current one on purpose — the bearer token alone proves "a signed-in
/// session," not "this person still knows the password," and a stolen
/// unattended session shouldn't be enough to lock the real owner out.
#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Admin-only: resetting a *different* account's password (a locked-out
/// teammate). No current-password check — the admin's own session is the
/// authority here, the same posture as UpdateRoleRequest.
#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}
