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
    pub role: String,
    pub created_at: DateTime<Utc>,
}

impl From<DashboardUser> for AdminProfile {
    fn from(u: DashboardUser) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
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
