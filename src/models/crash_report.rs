use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CrashReport {
    pub id: Uuid,
    pub platform: String,
    pub title: String,
    pub description: Option<String>,
    pub stack_trace: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub severity: String,
    pub status: String,
    pub source: String,
    pub device_id: Option<String>,
    pub reported_by: Option<Uuid>,
    pub github_issue_number: Option<i32>,
    pub github_issue_url: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// Unauthenticated on purpose — a crash can happen before a device has
/// finished registering, so reporting one can never require a token.
#[derive(Debug, Deserialize)]
pub struct SubmitCrashReportRequest {
    pub platform: String,
    pub title: String,
    pub description: Option<String>,
    pub stack_trace: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub severity: Option<String>,
    pub device_id: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCrashReportRequest {
    pub status: Option<String>,
    pub severity: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct ListCrashReportsQuery {
    pub status: Option<String>,
    pub platform: Option<String>,
    pub severity: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct CrashReportPage {
    pub items: Vec<CrashReport>,
    pub total: i64,
}

/// What publishing a report as a GitHub issue returns — the report itself
/// already carries `github_issue_url`, this is just the immediate result of
/// the click.
#[derive(Debug, Serialize)]
pub struct GitHubIssueRef {
    pub number: i32,
    pub url: String,
}
