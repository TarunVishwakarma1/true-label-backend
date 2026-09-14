//! Dashboard staff accounts: registration, login, and the session that
//! comes out of both. A separate identity system from `UserService` — see
//! the migration comment on `dashboard_users` for why.

use crate::{
    audit,
    auth::{self, AdminUser},
    error::{AppError, Result},
    models::{AdminProfile, AdminSession, DashboardUser, LoginRequest, RegisterRequest},
};
use uuid::Uuid;
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::NaiveDate;
use sqlx::PgPool;

const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 256;
const MAX_NAME_LEN: usize = 120;
const MAX_EMAIL_LEN: usize = 255;
const MAX_OCCUPATION_LEN: usize = 120;

pub struct AdminService {
    db: PgPool,
}

impl AdminService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// The first account this dashboard ever creates is the admin — that is
    /// what makes "I will be the admin" true without a manual database
    /// edit. Every account after it starts as `member`.
    ///
    /// Only that first account is a genuinely open sign-up: once one exists,
    /// `caller` must be an authenticated admin, or this is a stranger who
    /// found the URL, not a teammate being invited. Previously the only gate
    /// past the first account was the per-address rate limit — real, but not
    /// the same as actually requiring an invite.
    ///
    /// ponytail: this reads-then-writes the count without a lock, so two
    /// truly simultaneous first registrations could both become admin.
    /// Real risk only exists in the first few seconds this table has ever
    /// existed; an advisory lock is the upgrade if that ever matters.
    #[tracing::instrument(skip_all)]
    pub async fn register(&self, req: &RegisterRequest, caller: Option<&AdminUser>) -> Result<AdminSession> {
        let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dashboard_users")
            .fetch_one(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        if existing > 0 {
            match caller {
                Some(admin) => admin.require_admin()?,
                None => return Err(AppError::Unauthorized),
            }
        }

        let name = validate_name(&req.name)?;
        let email = normalize_email(&req.email)?;
        validate_password(&req.password)?;
        let date_of_birth = req.date_of_birth.as_deref().map(parse_dob).transpose()?;
        let occupation = req
            .occupation
            .as_deref()
            .map(str::trim)
            .filter(|o| !o.is_empty())
            .map(|o| truncate(o, MAX_OCCUPATION_LEN));

        let password_hash = hash_password(&req.password)?;
        let token = auth::new_token();
        let role = if existing == 0 { "admin" } else { "member" };

        let user = sqlx::query_as::<_, DashboardUser>(
            "INSERT INTO dashboard_users
               (name, email, date_of_birth, occupation, password_hash, role, token_hash, token_issued_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
             RETURNING *",
        )
        .bind(&name)
        .bind(&email)
        .bind(date_of_birth)
        .bind(occupation.as_deref())
        .bind(&password_hash)
        .bind(role)
        .bind(auth::hash_token(&token))
        .fetch_one(&self.db)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                AppError::Conflict("an account with this email already exists".to_string())
            } else {
                AppError::Database(e.to_string())
            }
        })?;

        tracing::info!(role = %user.role, "dashboard account registered");
        // Only a real invite, not the first-account bootstrap — nobody
        // "invited" the very first admin, there was no one to do it.
        if let Some(admin) = caller {
            audit::record(
                &self.db,
                Some(admin.id),
                &admin.name,
                "team.member_invited",
                "dashboard_user",
                user.id,
                serde_json::json!({ "email": user.email, "role": user.role }),
            )
            .await;
        }
        Ok(AdminSession { token, profile: AdminProfile::from(user) })
    }

    /// Mints a fresh token and overwrites the stored one — a second login
    /// (a different browser, say) signs the first one out. One active
    /// session at a time is the simple choice for a handful of staff
    /// accounts; a sessions table is the upgrade if that ever stops
    /// being enough.
    #[tracing::instrument(skip_all)]
    pub async fn login(&self, req: &LoginRequest) -> Result<AdminSession> {
        let email = normalize_email(&req.email)?;

        let user = sqlx::query_as::<_, DashboardUser>(
            "SELECT * FROM dashboard_users WHERE email = $1",
        )
        .bind(&email)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::Unauthorized)?;

        if !verify_password(&req.password, &user.password_hash)? {
            tracing::warn!("rejected a dashboard login with a bad password");
            return Err(AppError::Unauthorized);
        }

        let token = auth::new_token();
        let user = sqlx::query_as::<_, DashboardUser>(
            "UPDATE dashboard_users SET token_hash = $2, token_issued_at = NOW(), updated_at = NOW()
             WHERE id = $1 RETURNING *",
        )
        .bind(user.id)
        .bind(auth::hash_token(&token))
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("dashboard login");
        Ok(AdminSession { token, profile: AdminProfile::from(user) })
    }

    #[tracing::instrument(skip_all)]
    pub async fn profile(&self, user_id: uuid::Uuid) -> Result<AdminProfile> {
        let user = sqlx::query_as::<_, DashboardUser>("SELECT * FROM dashboard_users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or(AppError::Unauthorized)?;
        Ok(AdminProfile::from(user))
    }

    #[tracing::instrument(skip_all)]
    pub async fn logout(&self, user_id: uuid::Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE dashboard_users SET token_hash = NULL, token_issued_at = NULL, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn list_team(&self) -> Result<Vec<AdminProfile>> {
        let users = sqlx::query_as::<_, DashboardUser>("SELECT * FROM dashboard_users ORDER BY created_at ASC")
            .fetch_all(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(users.into_iter().map(AdminProfile::from).collect())
    }

    /// Refuses to demote the last admin — otherwise the team can lock
    /// itself out with no way back in short of a direct database edit.
    #[tracing::instrument(skip_all)]
    pub async fn update_role(&self, actor: &AdminUser, user_id: Uuid, role: &str) -> Result<AdminProfile> {
        let role = one_of("role", role, &["admin", "member"])?;

        let target = sqlx::query_as::<_, DashboardUser>("SELECT * FROM dashboard_users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or(AppError::ProductNotFound)?;

        if target.role == "admin" && role != "admin" {
            let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dashboard_users WHERE role = 'admin'")
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
            if admins <= 1 {
                return Err(AppError::Conflict("can't demote the last admin".to_string()));
            }
        }

        let user = sqlx::query_as::<_, DashboardUser>(
            "UPDATE dashboard_users SET role = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(user_id)
        .bind(&role)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(role = %user.role, "dashboard role updated");
        audit::record(
            &self.db,
            Some(actor.id),
            &actor.name,
            "team.role_changed",
            "dashboard_user",
            user.id,
            serde_json::json!({ "from": target.role, "to": user.role }),
        )
        .await;
        Ok(AdminProfile::from(user))
    }

    /// Self-service. Requires the current password — the bearer token alone
    /// proves "a live session," not "this is really the account owner," and
    /// an unattended signed-in browser shouldn't be enough to lock the real
    /// owner out of their own account.
    #[tracing::instrument(skip_all)]
    pub async fn change_password(&self, user_id: Uuid, current_password: &str, new_password: &str) -> Result<()> {
        let user = sqlx::query_as::<_, DashboardUser>("SELECT * FROM dashboard_users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or(AppError::Unauthorized)?;

        if !verify_password(current_password, &user.password_hash)? {
            tracing::warn!("rejected a password change with the wrong current password");
            return Err(AppError::Unauthorized);
        }
        validate_password(new_password)?;

        sqlx::query("UPDATE dashboard_users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
            .bind(user_id)
            .bind(hash_password(new_password)?)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("dashboard password changed");
        Ok(())
    }

    /// Admin-only, for a locked-out teammate — no current-password check,
    /// the caller's own admin session is the authority (same posture as
    /// `update_role`). Doesn't cover the sole-admin-locked-themselves-out
    /// case by construction: there's no other admin left to call this.
    /// That's a documented manual/DB recovery, not an API — see
    /// docs/deployment.mdx.
    #[tracing::instrument(skip_all)]
    pub async fn reset_password(&self, actor: &AdminUser, target_id: Uuid, new_password: &str) -> Result<()> {
        validate_password(new_password)?;

        let result = sqlx::query("UPDATE dashboard_users SET password_hash = $2, updated_at = NOW() WHERE id = $1")
            .bind(target_id)
            .bind(hash_password(new_password)?)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::ProductNotFound);
        }
        tracing::info!(target = %target_id, "dashboard password reset by an admin");
        audit::record(
            &self.db,
            Some(actor.id),
            &actor.name,
            "team.password_reset",
            "dashboard_user",
            target_id,
            serde_json::json!({}),
        )
        .await;
        Ok(())
    }

    /// Refuses self-removal (no way back into your own account once your
    /// session eventually expires) and refuses removing the last admin —
    /// the same guard `update_role` already uses for demotion, reused here
    /// for the more permanent version of the same mistake.
    #[tracing::instrument(skip_all)]
    pub async fn remove_member(&self, actor: &AdminUser, target_id: Uuid) -> Result<()> {
        if target_id == actor.id {
            return Err(AppError::Conflict("can't remove your own account".to_string()));
        }

        let target = sqlx::query_as::<_, DashboardUser>("SELECT * FROM dashboard_users WHERE id = $1")
            .bind(target_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or(AppError::ProductNotFound)?;

        if target.role == "admin" {
            let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dashboard_users WHERE role = 'admin'")
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
            if admins <= 1 {
                return Err(AppError::Conflict("can't remove the last admin".to_string()));
            }
        }

        sqlx::query("DELETE FROM dashboard_users WHERE id = $1")
            .bind(target_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(target = %target_id, "dashboard account removed");
        audit::record(
            &self.db,
            Some(actor.id),
            &actor.name,
            "team.member_removed",
            "dashboard_user",
            target_id,
            serde_json::json!({ "name": target.name, "email": target.email, "role": target.role }),
        )
        .await;
        Ok(())
    }

    /// Grants or revokes a `member`'s product-edit rights. Never touches
    /// `role` — an admin's own edit rights come from `role` directly and
    /// don't depend on this flag at all.
    #[tracing::instrument(skip_all)]
    pub async fn set_can_edit_products(
        &self,
        actor: &AdminUser,
        target_id: Uuid,
        can_edit_products: bool,
    ) -> Result<AdminProfile> {
        let user = sqlx::query_as::<_, DashboardUser>(
            "UPDATE dashboard_users SET can_edit_products = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(target_id)
        .bind(can_edit_products)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        tracing::info!(target = %target_id, can_edit_products, "product edit permission updated");
        audit::record(
            &self.db,
            Some(actor.id),
            &actor.name,
            "team.product_permission_changed",
            "dashboard_user",
            target_id,
            serde_json::json!({ "can_edit_products": can_edit_products }),
        )
        .await;
        Ok(AdminProfile::from(user))
    }
}

fn one_of(field: &str, value: &str, allowed: &[&str]) -> Result<String> {
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(AppError::InvalidRequest(format!("{field} must be one of: {}", allowed.join(", "))))
    }
}

/// `pub`, not just crate-private: `main.rs`'s `--hash-password` mode calls
/// this directly, so the manual sole-admin-lockout recovery in
/// docs/deployment.mdx hashes with the exact same code path a real
/// register/reset would — never a hand-rolled or differently-parameterized
/// hash that might not verify.
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(format!("stored password hash unreadable: {e}")))?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_LEN {
        return Err(AppError::InvalidRequest(format!(
            "name must be 1-{MAX_NAME_LEN} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Lowercased for storage so `Tarun@x.com` and `tarun@x.com` are the same
/// account regardless of what the client sends. Not a full RFC 5322
/// validator — this only needs to reject obvious garbage before it reaches
/// the database, not police every edge case of the email spec.
fn normalize_email(email: &str) -> Result<String> {
    let trimmed = email.trim();
    let looks_like_email = trimmed.len() <= MAX_EMAIL_LEN
        && trimmed.matches('@').count() == 1
        && !trimmed.starts_with('@')
        && !trimmed.ends_with('@')
        && !trimmed.contains(char::is_whitespace)
        && trimmed.split('@').nth(1).is_some_and(|domain| domain.contains('.'));

    if looks_like_email {
        Ok(trimmed.to_lowercase())
    } else {
        Err(AppError::InvalidRequest("invalid email address".to_string()))
    }
}

fn validate_password(password: &str) -> Result<()> {
    if (MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&password.len()) {
        Ok(())
    } else {
        Err(AppError::InvalidRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )))
    }
}

fn parse_dob(raw: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::InvalidRequest("date_of_birth must be YYYY-MM-DD".to_string()))?;
    if date > chrono::Utc::now().date_naive() {
        return Err(AppError::InvalidRequest("date_of_birth cannot be in the future".to_string()));
    }
    Ok(date)
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hashed_password_verifies_against_itself_and_nothing_else() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert_ne!(hash, "correct horse battery staple");
        assert!(verify_password("correct horse battery staple", &hash).unwrap());
        assert!(!verify_password("wrong password", &hash).unwrap());
    }

    #[test]
    fn emails_are_normalized_and_obviously_invalid_ones_rejected() {
        assert_eq!(normalize_email("Tarun@Example.com").unwrap(), "tarun@example.com");
        assert_eq!(normalize_email("  tarun@example.com  ").unwrap(), "tarun@example.com");
        assert!(normalize_email("not-an-email").is_err());
        assert!(normalize_email("@example.com").is_err());
        assert!(normalize_email("tarun@").is_err());
        assert!(normalize_email("tarun@localhost").is_err());
        assert!(normalize_email("two@at@signs.com").is_err());
        assert!(normalize_email("has space@example.com").is_err());
    }

    #[test]
    fn password_length_is_bounded() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("a very reasonable password").is_ok());
        assert!(validate_password(&"a".repeat(300)).is_err());
    }

    #[test]
    fn date_of_birth_must_be_a_real_past_date() {
        assert!(parse_dob("1995-06-12").is_ok());
        assert!(parse_dob("not-a-date").is_err());
        assert!(parse_dob("2099-01-01").is_err());
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(validate_name("  Tarun  ").unwrap(), "Tarun");
        assert!(validate_name("").is_err());
        assert!(validate_name("   ").is_err());
        assert!(validate_name(&"a".repeat(200)).is_err());
    }
}
