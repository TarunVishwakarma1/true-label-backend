use serde::Deserialize;

/// Trimmed to exactly what the handler reads. GitHub's real payload is much
/// larger; `state` itself is never read — `state_reason` alone is enough to
/// tell a completed close from a "not planned" one, and `action` alone
/// tells a reopen from a close.
#[derive(Debug, Deserialize)]
pub struct GitHubIssueWebhookPayload {
    pub action: String,
    pub issue: GitHubIssuePayload,
}

#[derive(Debug, Deserialize)]
pub struct GitHubIssuePayload {
    pub number: i32,
    pub state_reason: Option<String>,
}
