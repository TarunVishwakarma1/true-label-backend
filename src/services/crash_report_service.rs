use crate::{
    error::{AppError, Result},
    models::{CrashReport, CrashReportPage, ListCrashReportsQuery, SubmitCrashReportRequest, UpdateCrashReportRequest},
    services::GitHubService,
};
use sqlx::PgPool;
use uuid::Uuid;

const PLATFORMS: &[&str] = &["ios", "backend", "web"];
const SEVERITIES: &[&str] = &["low", "medium", "high", "critical"];
/// A JIRA/Bugzilla-shaped funnel — see the migration that introduced this
/// set (20240101000017) for why it replaced a coarser four-value one.
const STATUSES: &[&str] = &["submitted", "pending", "in_review", "in_progress", "done", "wont_fix"];

const MAX_TITLE_LEN: usize = 255;
const MAX_DESCRIPTION_LEN: usize = 5_000;
/// Matches the OCR submission bound elsewhere in this API — a stack trace
/// is the same shape of problem: bounded free text from an untrusted client.
const MAX_STACK_TRACE_LEN: usize = 20_000;
const MAX_SHORT_FIELD_LEN: usize = 100;

pub struct CrashReportService {
    db: PgPool,
    github: GitHubService,
}

impl CrashReportService {
    pub fn new(db: PgPool, github: GitHubService) -> Self {
        Self { db, github }
    }

    /// Unauthenticated — a crash can happen before a device finishes
    /// registering, so filing one can never depend on a token.
    #[tracing::instrument(skip_all)]
    pub async fn submit(&self, req: &SubmitCrashReportRequest) -> Result<CrashReport> {
        let platform = one_of("platform", &req.platform, PLATFORMS)?;
        let title = bounded("title", &req.title, 1, MAX_TITLE_LEN)?;
        let description = optional_bounded("description", req.description.as_deref(), MAX_DESCRIPTION_LEN)?;
        let stack_trace = optional_bounded("stack_trace", req.stack_trace.as_deref(), MAX_STACK_TRACE_LEN)?;
        let app_version = optional_bounded("app_version", req.app_version.as_deref(), MAX_SHORT_FIELD_LEN)?;
        let os_version = optional_bounded("os_version", req.os_version.as_deref(), MAX_SHORT_FIELD_LEN)?;
        let device_model = optional_bounded("device_model", req.device_model.as_deref(), MAX_SHORT_FIELD_LEN)?;
        let severity = match &req.severity {
            Some(s) => one_of("severity", s, SEVERITIES)?,
            None => "medium".to_string(),
        };
        let device_id = optional_bounded("device_id", req.device_id.as_deref(), 128)?;
        let metadata = req.metadata.clone().unwrap_or_else(|| serde_json::json!({}));

        let report = sqlx::query_as::<_, CrashReport>(
            "INSERT INTO crash_reports
               (platform, title, description, stack_trace, app_version, os_version,
                device_model, severity, source, device_id, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'app', $9, $10)
             RETURNING *",
        )
        .bind(&platform)
        .bind(&title)
        .bind(description.as_deref())
        .bind(stack_trace.as_deref())
        .bind(app_version.as_deref())
        .bind(os_version.as_deref())
        .bind(device_model.as_deref())
        .bind(&severity)
        .bind(device_id.as_deref())
        .bind(&metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(platform = %report.platform, severity = %report.severity, "crash report filed");
        Ok(report)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self, query: &ListCrashReportsQuery) -> Result<CrashReportPage> {
        let limit = query.limit.clamp(1, 200);
        let offset = query.offset.max(0);
        let status = query.status.as_deref().map(|s| one_of("status", s, STATUSES)).transpose()?;
        let platform = query.platform.as_deref().map(|p| one_of("platform", p, PLATFORMS)).transpose()?;
        let severity = query.severity.as_deref().map(|s| one_of("severity", s, SEVERITIES)).transpose()?;

        let items = sqlx::query_as::<_, CrashReport>(
            "SELECT * FROM crash_reports
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::text IS NULL OR platform = $2)
               AND ($3::text IS NULL OR severity = $3)
             ORDER BY created_at DESC
             LIMIT $4 OFFSET $5",
        )
        .bind(&status)
        .bind(&platform)
        .bind(&severity)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM crash_reports
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::text IS NULL OR platform = $2)
               AND ($3::text IS NULL OR severity = $3)",
        )
        .bind(&status)
        .bind(&platform)
        .bind(&severity)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(CrashReportPage { items, total })
    }

    #[tracing::instrument(skip_all)]
    pub async fn get(&self, id: Uuid) -> Result<CrashReport> {
        sqlx::query_as::<_, CrashReport>("SELECT * FROM crash_reports WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?
            .ok_or(AppError::ProductNotFound)
    }

    /// Any signed-in staff account can triage — status and severity are
    /// judgment calls, not a privileged action. Publishing to GitHub is the
    /// one gated separately, in the handler.
    #[tracing::instrument(skip_all)]
    pub async fn update(&self, id: Uuid, req: &UpdateCrashReportRequest) -> Result<CrashReport> {
        let status = req.status.as_deref().map(|s| one_of("status", s, STATUSES)).transpose()?;
        let severity = req.severity.as_deref().map(|s| one_of("severity", s, SEVERITIES)).transpose()?;

        // Every expression here reads the row as it stood before this
        // statement, `status` included — so the CASE below is comparing
        // against the *old* status even though `status` itself is also
        // being written a few lines up. Newly done stamps `resolved_at`;
        // moved to anything else clears it; a severity-only update (`$2`
        // NULL) or marking an already-done report done again leaves it
        // alone. `wont_fix` deliberately does not set it — that is a closed
        // state, not a fixed one.
        let report = sqlx::query_as::<_, CrashReport>(
            "UPDATE crash_reports SET
               status = COALESCE($2, status),
               severity = COALESCE($3, severity),
               resolved_at = CASE
                 WHEN $2 = 'done' AND status <> 'done' THEN NOW()
                 WHEN $2 IS NOT NULL AND $2 <> 'done' THEN NULL
                 ELSE resolved_at
               END,
               updated_at = NOW()
             WHERE id = $1
             RETURNING *",
        )
        .bind(id)
        .bind(&status)
        .bind(&severity)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or(AppError::ProductNotFound)?;

        tracing::info!(status = %report.status, "crash report updated");
        Ok(report)
    }

    /// One click, one issue: a report already carrying a `github_issue_url`
    /// refuses rather than opening a duplicate.
    #[tracing::instrument(skip_all)]
    pub async fn publish_to_github(&self, id: Uuid) -> Result<CrashReport> {
        let report = self.get(id).await?;
        if report.github_issue_url.is_some() {
            return Err(AppError::Conflict("already published to GitHub".to_string()));
        }

        let title = format!("[{}] {}", report.platform, report.title);
        let body = issue_body(&report);
        let labels: Vec<&str> = vec!["crash-report", report.platform.as_str(), report.severity.as_str()];
        let issue = self.github.create_issue(&title, &body, &labels).await?;

        let report = sqlx::query_as::<_, CrashReport>(
            "UPDATE crash_reports SET github_issue_number = $2, github_issue_url = $3, updated_at = NOW()
             WHERE id = $1 RETURNING *",
        )
        .bind(id)
        .bind(issue.number)
        .bind(&issue.url)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(issue = issue.number, "crash report published to GitHub");
        Ok(report)
    }
}

fn issue_body(report: &CrashReport) -> String {
    let mut body = String::new();
    if let Some(description) = &report.description {
        body.push_str(description);
        body.push_str("\n\n");
    }
    body.push_str("| | |\n|---|---|\n");
    body.push_str(&format!("| Platform | {} |\n", report.platform));
    body.push_str(&format!("| Severity | {} |\n", report.severity));
    if let Some(v) = &report.app_version {
        body.push_str(&format!("| App version | {v} |\n"));
    }
    if let Some(v) = &report.os_version {
        body.push_str(&format!("| OS version | {v} |\n"));
    }
    if let Some(v) = &report.device_model {
        body.push_str(&format!("| Device | {v} |\n"));
    }
    body.push_str(&format!("| Reported | {} |\n", report.created_at.to_rfc3339()));
    if let Some(trace) = &report.stack_trace {
        body.push_str("\n<details><summary>Stack trace</summary>\n\n```\n");
        body.push_str(trace);
        body.push_str("\n```\n</details>\n");
    }
    body.push_str(&format!("\n_Filed from the TrueLabel dashboard — crash report `{}`._", report.id));
    body
}

fn one_of(field: &str, value: &str, allowed: &[&str]) -> Result<String> {
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        Err(AppError::InvalidRequest(format!(
            "{field} must be one of: {}",
            allowed.join(", ")
        )))
    }
}

fn bounded(field: &str, value: &str, min: usize, max: usize) -> Result<String> {
    let trimmed = value.trim();
    if (min..=max).contains(&trimmed.chars().count()) {
        Ok(trimmed.to_string())
    } else {
        Err(AppError::InvalidRequest(format!(
            "{field} must be {min}-{max} characters"
        )))
    }
}

fn optional_bounded(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if v.chars().count() <= max => Ok(Some(v.to_string())),
        Some(_) => Err(AppError::InvalidRequest(format!("{field} longer than {max} characters"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_fields_reject_anything_off_the_list() {
        assert!(one_of("platform", "ios", PLATFORMS).is_ok());
        assert!(one_of("platform", "android", PLATFORMS).is_err());
        assert!(one_of("severity", "critical", SEVERITIES).is_ok());
        assert!(one_of("status", "done", STATUSES).is_ok());
        assert!(one_of("status", "in_review", STATUSES).is_ok());
        assert!(one_of("status", "resolved", STATUSES).is_err());
        assert!(one_of("status", "deleted", STATUSES).is_err());
    }

    #[test]
    fn bounded_fields_are_trimmed_and_length_checked() {
        assert_eq!(bounded("title", "  Crash on launch  ", 1, 255).unwrap(), "Crash on launch");
        assert!(bounded("title", "   ", 1, 255).is_err());
        assert!(bounded("title", &"x".repeat(300), 1, 255).is_err());
    }

    #[test]
    fn optional_fields_treat_blank_as_absent() {
        assert_eq!(optional_bounded("device_model", Some("  "), 50).unwrap(), None);
        assert_eq!(
            optional_bounded("device_model", Some(" iPhone 17 "), 50).unwrap(),
            Some("iPhone 17".to_string())
        );
        assert!(optional_bounded("device_model", Some(&"x".repeat(60)), 50).is_err());
    }
}
