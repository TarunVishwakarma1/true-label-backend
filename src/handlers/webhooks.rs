//! GitHub → dashboard, the inbound half of crash-report status sync. This
//! handler never calls `GitHubService` — it only ever writes through
//! `CrashReportService::sync_status_from_github`, which is itself built so
//! that path has no way to call back out to GitHub. See that method's doc
//! comment for the other half of the loop-avoidance story.

use crate::{
    error::{AppError, Result},
    models::GitHubIssueWebhookPayload,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub async fn github(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Result<StatusCode> {
    let secret = state.config.github_webhook_secret.as_deref().ok_or_else(|| {
        tracing::warn!("rejected a GitHub webhook delivery — GITHUB_WEBHOOK_SECRET isn't configured");
        AppError::Unauthorized
    })?;

    verify_signature(secret, &headers, &body)?;

    // GitHub subscribes at the repo level, so this fires for the very
    // first `ping` delivery (sent the moment the webhook is saved, and on
    // every manual "redeliver" test — neither carries an `issue` field at
    // all) and for every ordinary issue in the repo, not just crash
    // reports. Anything that isn't an `issues` event is a silent 200, not
    // a 404/error — otherwise GitHub's delivery log fills with red X's for
    // traffic this endpoint was never meant to act on.
    let event = headers.get("x-github-event").and_then(|v| v.to_str().ok()).unwrap_or("");
    if event != "issues" {
        return Ok(StatusCode::OK);
    }

    let payload: GitHubIssueWebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| AppError::InvalidRequest(format!("unreadable issues webhook payload: {e}")))?;

    // Only these two actions carry a status change worth mirroring —
    // edited/labeled/assigned/etc. are all ignored.
    let status = match payload.action.as_str() {
        "closed" => match payload.issue.state_reason.as_deref() {
            Some("not_planned") => "wont_fix",
            _ => "done",
        },
        "reopened" => "in_progress",
        _ => return Ok(StatusCode::OK),
    };

    state
        .crash_report_service
        .sync_status_from_github(payload.issue.number, status)
        .await?;

    Ok(StatusCode::OK)
}

/// Constant-time via `Mac::verify_slice` — the whole point of HMAC here is
/// that a caller can't derive a valid signature without the secret, but a
/// naive byte-by-byte `==` would leak timing information about how many
/// leading bytes matched.
fn verify_signature(secret: &str, headers: &HeaderMap, body: &[u8]) -> Result<()> {
    let header = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    let hex_sig = header.strip_prefix("sha256=").ok_or(AppError::Unauthorized)?;
    let signature = decode_hex(hex_sig).ok_or(AppError::Unauthorized)?;

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC key setup failed: {e}")))?;
    mac.update(body);
    mac.verify_slice(&signature).map_err(|_| {
        tracing::warn!("rejected a GitHub webhook delivery with an invalid signature");
        AppError::Unauthorized
    })
}

/// Byte-level on purpose, not `&hex[i..i+2]` string slicing — a header
/// value is untrusted input, and slicing a `str` by byte index panics on a
/// non-ASCII UTF-8 boundary. Working on bytes throughout means a malformed
/// header just fails to parse instead of being a crash.
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let bytes = hex.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16)?;
            let lo = (pair[1] as char).to_digit(16)?;
            Some(((hi as u8) << 4) | (lo as u8))
        })
        .collect()
}
