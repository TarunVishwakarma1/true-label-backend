//! One call: turn a crash report into a GitHub issue. Kept separate from
//! `CrashReportService` the same way `AppleAuth` is kept separate from
//! `UserService` — one external API, one small file.

use crate::error::{AppError, Result};
use crate::models::GitHubIssueRef;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const API_BASE: &str = "https://api.github.com";
/// Required by GitHub's API on every request — an unidentified client gets
/// a flat 403 regardless of the token.
const USER_AGENT: &str = "truelabel-backend";

pub struct GitHubService {
    client: reqwest::Client,
    /// A personal access token with `Issues: write` on `repo`. `None` means
    /// the dashboard's "publish" button was never configured — checked at
    /// call time, not at boot, so the rest of the app works without it.
    token: Option<String>,
    /// `owner/repo`.
    repo: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateIssueBody<'a> {
    title: &'a str,
    body: &'a str,
    labels: &'a [&'a str],
}

#[derive(Debug, Deserialize)]
struct CreateIssueResponse {
    number: i32,
    html_url: String,
}

#[derive(Debug, Serialize)]
struct UpdateIssueBody<'a> {
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_reason: Option<&'a str>,
}

impl GitHubService {
    pub fn new(token: Option<String>, repo: Option<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent(USER_AGENT)
                .build()
                .unwrap(),
            token,
            repo,
        }
    }

    /// Both `create_issue` and `update_issue_state` need a token and a repo
    /// before they can call anything — one place for that check so the two
    /// don't drift on the error message.
    fn credentials(&self) -> Result<(&str, &str)> {
        let token = self.token.as_deref().ok_or_else(|| {
            AppError::ExternalApi(
                "GitHub publishing isn't configured (set GITHUB_TOKEN and GITHUB_REPO)".to_string(),
            )
        })?;
        let repo = self.repo.as_deref().ok_or_else(|| {
            AppError::ExternalApi(
                "GitHub publishing isn't configured (set GITHUB_TOKEN and GITHUB_REPO)".to_string(),
            )
        })?;
        Ok((token, repo))
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_issue(&self, title: &str, body: &str, labels: &[&str]) -> Result<GitHubIssueRef> {
        let (token, repo) = self.credentials()?;

        let response = self
            .client
            .post(format!("{API_BASE}/repos/{repo}/issues"))
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&CreateIssueBody { title, body, labels })
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("GitHub unreachable: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            tracing::error!(%status, %detail, "GitHub rejected the issue creation request");
            return Err(AppError::ExternalApi(format!("GitHub returned HTTP {status}")));
        }

        let created: CreateIssueResponse = response
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(format!("unreadable GitHub response: {e}")))?;

        Ok(GitHubIssueRef { number: created.number, url: created.html_url })
    }

    /// Mirrors a dashboard status change onto the linked issue — `state` is
    /// `"open"` or `"closed"`, `state_reason` is `Some("completed")`,
    /// `Some("not_planned")`, or `None` (only meaningful alongside
    /// `"closed"`; GitHub ignores it otherwise).
    #[tracing::instrument(skip_all)]
    pub async fn update_issue_state(&self, number: i32, state: &str, state_reason: Option<&str>) -> Result<()> {
        let (token, repo) = self.credentials()?;

        let response = self
            .client
            .patch(format!("{API_BASE}/repos/{repo}/issues/{number}"))
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&UpdateIssueBody { state, state_reason })
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("GitHub unreachable: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            tracing::error!(%status, %detail, issue = number, "GitHub rejected the issue update request");
            return Err(AppError::ExternalApi(format!("GitHub returned HTTP {status}")));
        }

        Ok(())
    }
}
