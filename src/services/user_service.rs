use crate::{
    auth,
    error::{AppError, Result},
    models::{
        ContributionStats, DeviceRegistration, LinkAccountRequest, ProfileResponse,
        SubscriptionResponse, UpdateProfileRequest, User,
    },
    services::AppleAuth,
};
use serde_json::json;
use sqlx::PgPool;

/// Trust boundary: these arrive from an unauthenticated client, so they are
/// bounded before anything reaches the database.
const MAX_PREFERENCES: usize = 32;
const MAX_PREFERENCE_LEN: usize = 64;

const MAX_DISPLAY_NAME_LEN: usize = 120;

pub struct UserService {
    db: PgPool,
    apple: AppleAuth,
}

impl UserService {
    pub fn new(db: PgPool, apple: AppleAuth) -> Self {
        Self { db, apple }
    }

    /// The device id is ours to generate, not the client's to choose. When
    /// the client supplied it, anyone who learned one could act as that
    /// device; now there is nothing to guess, because the only way to hold a
    /// row is to have been handed its token.
    #[tracing::instrument(skip_all)]
    pub async fn register_device(&self) -> Result<DeviceRegistration> {
        let token = auth::new_token();
        let device_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO users (device_id, token_hash, token_issued_at) VALUES ($1, $2, NOW())",
        )
        .bind(&device_id)
        .bind(auth::hash_token(&token))
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("registered a device");
        Ok(DeviceRegistration { device_id, token })
    }

    /// Reading a profile creates it if it is missing, so the client never
    /// needs a separate registration step — the first scan is the sign-up.
    #[tracing::instrument(skip_all)]
    pub async fn get_or_create(&self, device_id: &str, country: &str) -> Result<ProfileResponse> {
        validate_device_id(device_id)?;
        let country = normalize_country(country)?;

        let user = sqlx::query_as::<_, User>(
            "INSERT INTO users (device_id, country) VALUES ($1, $2)
             ON CONFLICT (device_id) DO UPDATE SET updated_at = NOW()
             RETURNING *",
        )
        .bind(device_id)
        .bind(&country)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(ProfileResponse::from(user))
    }

    /// Both fields are optional; absent means "leave it alone", so a client
    /// can change country without resending the whole preference list.
    #[tracing::instrument(skip_all)]
    pub async fn update(&self, device_id: &str, req: &UpdateProfileRequest) -> Result<ProfileResponse> {
        validate_device_id(device_id)?;

        let country = req.country.as_deref().map(normalize_country).transpose()?;
        let preferences = req
            .dietary_preferences
            .as_deref()
            .map(sanitize_preferences)
            .transpose()?;
        // An empty string clears the name; absent leaves it alone.
        let display_name = req.display_name.as_deref().map(|n| {
            n.trim().chars().take(MAX_DISPLAY_NAME_LEN).collect::<String>()
        });

        let user = sqlx::query_as::<_, User>(
            "INSERT INTO users (device_id, country, dietary_preferences, display_name)
             VALUES ($1, COALESCE($2, 'IN'), COALESCE($3, '[]'::jsonb), NULLIF($4, ''))
             ON CONFLICT (device_id) DO UPDATE SET
               country = COALESCE($2, users.country),
               dietary_preferences = COALESCE($3, users.dietary_preferences),
               display_name = CASE WHEN $4 IS NULL THEN users.display_name ELSE NULLIF($4, '') END,
               updated_at = NOW()
             RETURNING *",
        )
        .bind(device_id)
        .bind(country.as_deref())
        .bind(preferences.map(|p| json!(p)))
        .bind(display_name.as_deref())
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("profile updated");
        Ok(ProfileResponse::from(user))
    }

    /// Turns Plus on. No payment is taken or checked: while Plus is
    /// complimentary, asking for it is the whole transaction, and
    /// `plus_expires_at` stays NULL so it never reads as lapsed. When
    /// payments land, this takes a receipt, verifies it, and sets a real
    /// expiry — the column and the `source` discriminator are already here
    /// so that change doesn't move the shape of this response.
    #[tracing::instrument(skip_all)]
    pub async fn activate_plus(&self, device_id: &str) -> Result<SubscriptionResponse> {
        validate_device_id(device_id)?;

        let user = sqlx::query_as::<_, User>(
            "INSERT INTO users (device_id, plus_since, plus_source)
             VALUES ($1, NOW(), 'complimentary')
             ON CONFLICT (device_id) DO UPDATE SET
               plus_since = COALESCE(users.plus_since, NOW()),
               plus_expires_at = NULL,
               plus_source = 'complimentary',
               updated_at = NOW()
             RETURNING *",
        )
        .bind(device_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("plus activated");
        Ok(SubscriptionResponse::from(&user))
    }

    #[tracing::instrument(skip_all)]
    pub async fn cancel_plus(&self, device_id: &str) -> Result<SubscriptionResponse> {
        validate_device_id(device_id)?;

        let user = sqlx::query_as::<_, User>(
            "UPDATE users SET plus_since = NULL, plus_expires_at = NULL, plus_source = NULL, updated_at = NOW()
             WHERE device_id = $1 RETURNING *",
        )
        .bind(device_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        tracing::info!("plus cancelled");
        Ok(SubscriptionResponse::from(&user))
    }

    /// Attaches an Apple identity to this device's row. If that identity
    /// already owns a row on another device, the account *moves*: its Plus
    /// state and preferences come across and the old row is removed, so
    /// signing in on a new phone restores everything and the old phone
    /// reverts to anonymous. One active device per account — enough for an
    /// app with no cross-device sync, and it never silently duplicates
    /// entitlements.
    #[tracing::instrument(skip_all)]
    pub async fn link_apple(&self, device_id: &str, req: &LinkAccountRequest) -> Result<ProfileResponse> {
        validate_device_id(device_id)?;
        let claims = self.apple.verify(&req.identity_token).await?;
        let display_name = req
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(|n| n.chars().take(MAX_DISPLAY_NAME_LEN).collect::<String>());

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        // Make sure a row exists for this device before anything moves onto it.
        sqlx::query("INSERT INTO users (device_id) VALUES ($1) ON CONFLICT (device_id) DO NOTHING")
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        let previous = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE apple_user_id = $1 AND device_id <> $2",
        )
        .bind(&claims.sub)
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if let Some(previous) = &previous {
            sqlx::query("DELETE FROM users WHERE device_id = $1")
                .bind(&previous.device_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
            tracing::info!(from = %previous.device_id, "account moved to this device");
        }

        // Apple sends the email only on first authorization, so a later
        // sign-in must not blank out what we already hold. Same for the name.
        let user = sqlx::query_as::<_, User>(
            "UPDATE users SET
               apple_user_id = $2,
               email = COALESCE($3, $6, users.email),
               display_name = COALESCE($4, $7, users.display_name),
               dietary_preferences = CASE
                   WHEN users.dietary_preferences = '[]'::jsonb THEN COALESCE($8, users.dietary_preferences)
                   ELSE users.dietary_preferences END,
               plus_since = COALESCE(users.plus_since, $9),
               plus_expires_at = COALESCE(users.plus_expires_at, $10),
               plus_source = COALESCE(users.plus_source, $11),
               linked_at = COALESCE(users.linked_at, NOW()),
               country = COALESCE($5, users.country),
               updated_at = NOW()
             WHERE device_id = $1
             RETURNING *",
        )
        .bind(device_id)
        .bind(&claims.sub)
        .bind(claims.email.as_deref())
        .bind(display_name.as_deref())
        .bind(Option::<String>::None)
        .bind(previous.as_ref().and_then(|p| p.email.clone()))
        .bind(previous.as_ref().and_then(|p| p.display_name.clone()))
        .bind(previous.as_ref().map(|p| p.dietary_preferences.clone()))
        .bind(previous.as_ref().and_then(|p| p.plus_since))
        .bind(previous.as_ref().and_then(|p| p.plus_expires_at))
        .bind(previous.as_ref().and_then(|p| p.plus_source.clone()))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tx.commit().await.map_err(|e| AppError::Database(e.to_string()))?;
        tracing::info!("apple account linked");
        Ok(ProfileResponse::from(user))
    }

    /// Signing out detaches the identity and keeps the device row, so the
    /// app keeps working and nothing is destroyed. Deleting the account is a
    /// separate, explicit action.
    #[tracing::instrument(skip_all)]
    pub async fn unlink(&self, device_id: &str) -> Result<ProfileResponse> {
        validate_device_id(device_id)?;
        let user = sqlx::query_as::<_, User>(
            "UPDATE users SET apple_user_id = NULL, email = NULL, display_name = NULL,
                              linked_at = NULL, updated_at = NOW()
             WHERE device_id = $1 RETURNING *",
        )
        .bind(device_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        tracing::info!("account unlinked");
        Ok(ProfileResponse::from(user))
    }

    /// Full deletion, not deactivation — the App Store requires an app that
    /// creates accounts to let people delete them from inside the app.
    /// Verifications this device submitted are left alone: they carry no
    /// identity beyond the device id and removing them would silently
    /// unverify products other people rely on.
    #[tracing::instrument(skip_all)]
    pub async fn delete_account(&self, device_id: &str) -> Result<()> {
        validate_device_id(device_id)?;
        sqlx::query("DELETE FROM users WHERE device_id = $1")
            .bind(device_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        tracing::info!("account deleted");
        Ok(())
    }

    /// One round trip for the three numbers, because three separate counts
    /// on a profile screen is three chances to be half-loaded.
    #[tracing::instrument(skip_all)]
    pub async fn stats(&self, device_id: &str) -> Result<ContributionStats> {
        validate_device_id(device_id)?;
        sqlx::query_as::<_, ContributionStats>(
            "SELECT
               (SELECT COUNT(*) FROM verifications WHERE device_id = $1) AS confirmations,
               (SELECT COUNT(*) FROM ocr_submissions WHERE device_id = $1) AS contributions,
               (SELECT COUNT(*) FROM verifications v
                  JOIN products p ON p.id = v.product_id
                  WHERE v.device_id = $1 AND p.verified = TRUE) AS helped_verify,
               (SELECT created_at FROM users WHERE device_id = $1) AS member_since",
        )
        .bind(device_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
    }

    #[tracing::instrument(skip_all)]
    pub async fn subscription(&self, device_id: &str) -> Result<SubscriptionResponse> {
        validate_device_id(device_id)?;
        let profile = self.get_or_create(device_id, "IN").await?;
        Ok(profile.subscription)
    }
}

/// identifierForVendor is a UUID, but nothing stops another client sending
/// something else — bound the length and the alphabet rather than trust it.
fn validate_device_id(device_id: &str) -> Result<()> {
    let ok = (8..=128).contains(&device_id.len())
        && device_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidRequest("invalid device id".to_string()))
    }
}

fn normalize_country(country: &str) -> Result<String> {
    if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(AppError::InvalidCountry);
    }
    Ok(country.to_uppercase())
}

/// The backend does not own the preference vocabulary — the app does, and it
/// will grow. So these are stored as given rather than matched against a
/// list, but trimmed, de-duplicated, and bounded so one client cannot write
/// an unbounded blob into a shared table.
fn sanitize_preferences(input: &[String]) -> Result<Vec<String>> {
    if input.len() > MAX_PREFERENCES {
        return Err(AppError::InvalidRequest(format!(
            "at most {MAX_PREFERENCES} preferences"
        )));
    }
    let mut out: Vec<String> = Vec::with_capacity(input.len());
    for raw in input {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > MAX_PREFERENCE_LEN {
            return Err(AppError::InvalidRequest(format!(
                "preference longer than {MAX_PREFERENCE_LEN} characters"
            )));
        }
        if !out.iter().any(|e| e == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SubscriptionResponse, User};
    use chrono::{Duration, Utc};

    fn user(plus_since: Option<chrono::DateTime<Utc>>, expires: Option<chrono::DateTime<Utc>>) -> User {
        User {
            device_id: "ABCDEF12-3456".to_string(),
            country: "IN".to_string(),
            dietary_preferences: json!([]),
            plus_since,
            plus_expires_at: expires,
            plus_source: plus_since.map(|_| "complimentary".to_string()),
            apple_user_id: None,
            email: None,
            display_name: None,
            linked_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn free_period_has_no_expiry_and_is_active() {
        let s = SubscriptionResponse::from(&user(Some(Utc::now()), None));
        assert!(s.active);
        assert_eq!(s.tier, "plus");
        assert!(s.expires_at.is_none());
    }

    #[test]
    fn never_activated_is_free() {
        let s = SubscriptionResponse::from(&user(None, None));
        assert!(!s.active);
        assert_eq!(s.tier, "free");
    }

    #[test]
    fn a_past_expiry_lapses_but_a_future_one_does_not() {
        let lapsed = SubscriptionResponse::from(&user(
            Some(Utc::now() - Duration::days(40)),
            Some(Utc::now() - Duration::days(1)),
        ));
        assert!(!lapsed.active);

        let current = SubscriptionResponse::from(&user(
            Some(Utc::now() - Duration::days(1)),
            Some(Utc::now() + Duration::days(29)),
        ));
        assert!(current.active);
    }

    #[test]
    fn identity_reports_signed_out_until_an_apple_id_is_attached() {
        use crate::models::IdentityResponse;

        let mut u = user(None, None);
        let anonymous = IdentityResponse::from(&u);
        assert!(!anonymous.signed_in);
        assert!(anonymous.provider.is_none());

        u.apple_user_id = Some("000123.abc.0001".to_string());
        u.display_name = Some("Tarun".to_string());
        let signed_in = IdentityResponse::from(&u);
        assert!(signed_in.signed_in);
        assert_eq!(signed_in.provider.as_deref(), Some("apple"));
        // Apple withholds the email after first authorization; signed in
        // without one is normal, not an error.
        assert!(signed_in.email.is_none());
        assert_eq!(signed_in.display_name.as_deref(), Some("Tarun"));
    }

    #[test]
    fn a_registered_id_is_ours_to_issue_and_valid_to_use() {
        // The client no longer chooses this, so whatever we mint has to pass
        // the same validation every authenticated path applies.
        let id = uuid::Uuid::new_v4().to_string();
        assert!(validate_device_id(&id).is_ok());
    }

    #[test]
    fn device_ids_are_bounded_and_alphanumeric() {
        assert!(validate_device_id("9B1D6F20-80E2-47DB-9D9B-FDF6ECFA8B85").is_ok());
        assert!(validate_device_id("short").is_err());
        assert!(validate_device_id(&"a".repeat(129)).is_err());
        assert!(validate_device_id("has spaces in it").is_err());
        assert!(validate_device_id("drop--table;users").is_err());
    }

    #[test]
    fn preferences_are_trimmed_deduped_and_bounded() {
        let input = vec![
            "  Vegetarian ".to_string(),
            "Vegetarian".to_string(),
            "".to_string(),
            "Low sugar".to_string(),
        ];
        assert_eq!(
            sanitize_preferences(&input).unwrap(),
            vec!["Vegetarian".to_string(), "Low sugar".to_string()]
        );
        assert!(sanitize_preferences(&vec!["x".to_string(); 33]).is_err());
        assert!(sanitize_preferences(&[ "x".repeat(65) ]).is_err());
    }

    #[test]
    fn country_is_normalized_and_validated() {
        assert_eq!(normalize_country("in").unwrap(), "IN");
        assert!(normalize_country("IND").is_err());
        assert!(normalize_country("1N").is_err());
    }
}
