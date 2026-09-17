//! Outgoing webhook notifications for real-time dashboard events.
//!
//! Sends instant notification updates to configured endpoints (e.g. Slack, Discord,
//! or custom webhooks) when:
//! - A new user self-registers (`new-user`)
//! - A user requests resource or role elevation access
//! - An administrator changes a teammate's role or product permissions
//! - A new crash report is submitted

use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WebhookService {
    client: Client,
    webhook_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Field read by Slack incoming webhooks & custom webhook consumers.
    pub text: String,
    /// Field read by Discord incoming webhooks (`content`).
    pub content: String,
    pub data: serde_json::Value,
}

impl WebhookService {
    pub fn new(webhook_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        Self { client, webhook_url }
    }

    /// Dispatches an event payload asynchronously without blocking the caller.
    pub fn dispatch(&self, event: &str, text: &str, data: serde_json::Value) {
        let Some(url) = self.webhook_url.clone() else {
            return;
        };

        let payload = WebhookPayload {
            event: event.to_string(),
            timestamp: chrono::Utc::now(),
            text: text.to_string(),
            content: text.to_string(),
            data,
        };

        let client = self.client.clone();
        tokio::spawn(async move {
            if let Err(err) = client.post(&url).json(&payload).send().await {
                tracing::warn!(error = %err, url = %url, "failed to dispatch outgoing webhook notification");
            } else {
                tracing::info!(event = %payload.event, "outgoing webhook notification dispatched");
            }
        });
    }

    pub fn notify_user_registered(&self, name: &str, email: &str, role: &str) {
        let text = format!("👤 **New User Registered**: `{name}` ({email}) joined with role `{role}`.");
        self.dispatch(
            "auth.user_registered",
            &text,
            serde_json::json!({
                "name": name,
                "email": email,
                "role": role,
            }),
        );
    }

    pub fn notify_access_requested(
        &self,
        name: &str,
        email: &str,
        resource: Option<&str>,
        message: Option<&str>,
    ) {
        let resource_str = resource.unwrap_or("general access / member elevation");
        let msg_str = message.unwrap_or("No message provided");
        let text = format!(
            "🔑 **Access Request**: `{name}` ({email}) requested `{resource_str}`.\n> \"{msg_str}\""
        );
        self.dispatch(
            "auth.access_requested",
            &text,
            serde_json::json!({
                "name": name,
                "email": email,
                "resource": resource,
                "message": message,
            }),
        );
    }

    pub fn notify_role_changed(&self, admin_name: &str, target_name: &str, target_email: &str, old_role: &str, new_role: &str) {
        let text = format!(
            "🛡️ **Role Updated**: Admin `{admin_name}` changed `{target_name}`'s ({target_email}) role from `{old_role}` to `{new_role}`."
        );
        self.dispatch(
            "team.role_changed",
            &text,
            serde_json::json!({
                "admin_name": admin_name,
                "target_name": target_name,
                "target_email": target_email,
                "from_role": old_role,
                "to_role": new_role,
            }),
        );
    }

    pub fn notify_permissions_changed(&self, admin_name: &str, target_name: &str, can_edit_products: bool) {
        let perm_str = if can_edit_products { "granted" } else { "revoked" };
        let text = format!(
            "✏️ **Product Permissions Updated**: Admin `{admin_name}` {perm_str} product editing rights for `{target_name}`."
        );
        self.dispatch(
            "team.permissions_changed",
            &text,
            serde_json::json!({
                "admin_name": admin_name,
                "target_name": target_name,
                "can_edit_products": can_edit_products,
            }),
        );
    }
}
