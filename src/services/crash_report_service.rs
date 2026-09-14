use crate::{
    audit,
    auth::AdminUser,
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
    dashboard_url: Option<String>,
}

impl CrashReportService {
    pub fn new(db: PgPool, github: GitHubService, dashboard_url: Option<String>) -> Self {
        Self { db, github, dashboard_url }
    }

    /// Unauthenticated — a crash can happen before a device finishes
    /// registering, so filing one can never depend on a token.
    #[tracing::instrument(skip_all)]
    pub async fn submit(&self, req: &SubmitCrashReportRequest) -> Result<CrashReport> {
        let v = validate_submission(req)?;

        let report = sqlx::query_as::<_, CrashReport>(
            "INSERT INTO crash_reports
               (platform, title, description, stack_trace, app_version, os_version,
                device_model, severity, source, device_id, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'app', $9, $10)
             RETURNING *",
        )
        .bind(&v.platform)
        .bind(&v.title)
        .bind(v.description.as_deref())
        .bind(v.stack_trace.as_deref())
        .bind(v.app_version.as_deref())
        .bind(v.os_version.as_deref())
        .bind(v.device_model.as_deref())
        .bind(&v.severity)
        .bind(v.device_id.as_deref())
        .bind(&v.metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(platform = %report.platform, severity = %report.severity, "crash report filed");
        Ok(report)
    }

    /// Staff filing a report by hand — from a TestFlight crash log, a user
    /// email, whatever has no automated path yet. `reported_by` is who to
    /// ask; `device_id` from the client is dropped, since a human typing
    /// this in isn't the device that crashed.
    #[tracing::instrument(skip_all)]
    pub async fn create_manual(&self, admin_id: Uuid, req: &SubmitCrashReportRequest) -> Result<CrashReport> {
        let v = validate_submission(req)?;

        let report = sqlx::query_as::<_, CrashReport>(
            "INSERT INTO crash_reports
               (platform, title, description, stack_trace, app_version, os_version,
                device_model, severity, source, reported_by, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'manual', $9, $10)
             RETURNING *",
        )
        .bind(&v.platform)
        .bind(&v.title)
        .bind(v.description.as_deref())
        .bind(v.stack_trace.as_deref())
        .bind(v.app_version.as_deref())
        .bind(v.os_version.as_deref())
        .bind(v.device_model.as_deref())
        .bind(&v.severity)
        .bind(admin_id)
        .bind(&v.metadata)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!(platform = %report.platform, "crash report filed manually");
        Ok(report)
    }

    #[tracing::instrument(skip_all)]
    pub async fn list(&self, query: &ListCrashReportsQuery) -> Result<CrashReportPage> {
        let limit = query.limit.clamp(1, 200);
        let offset = query.offset.max(0);
        let status = query.status.as_deref().map(|s| one_of("status", s, STATUSES)).transpose()?;
        let platform = query.platform.as_deref().map(|p| one_of("platform", p, PLATFORMS)).transpose()?;
        let severity = query.severity.as_deref().map(|s| one_of("severity", s, SEVERITIES)).transpose()?;
        let q = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());

        let items = sqlx::query_as::<_, CrashReport>(
            "SELECT * FROM crash_reports
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::text IS NULL OR platform = $2)
               AND ($3::text IS NULL OR severity = $3)
               AND ($4::text IS NULL OR title ILIKE '%' || $4 || '%' OR description ILIKE '%' || $4 || '%')
             ORDER BY created_at DESC
             LIMIT $5 OFFSET $6",
        )
        .bind(&status)
        .bind(&platform)
        .bind(&severity)
        .bind(q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM crash_reports
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::text IS NULL OR platform = $2)
               AND ($3::text IS NULL OR severity = $3)
               AND ($4::text IS NULL OR title ILIKE '%' || $4 || '%' OR description ILIKE '%' || $4 || '%')",
        )
        .bind(&status)
        .bind(&platform)
        .bind(&severity)
        .bind(q)
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
    pub async fn update(&self, actor: &AdminUser, id: Uuid, req: &UpdateCrashReportRequest) -> Result<CrashReport> {
        let status = req.status.as_deref().map(|s| one_of("status", s, STATUSES)).transpose()?;
        let severity = req.severity.as_deref().map(|s| one_of("severity", s, SEVERITIES)).transpose()?;

        let report = write_status_update(&self.db, id, status.as_deref(), severity.as_deref()).await?;
        tracing::info!(status = %report.status, "crash report updated");

        if status.is_some() || severity.is_some() {
            audit::record(
                &self.db,
                Some(actor.id),
                &actor.name,
                "crash_report.status_changed",
                "crash_report",
                report.id,
                serde_json::json!({ "status": report.status, "severity": report.severity }),
            )
            .await;
        }

        // Dashboard → GitHub. Never the other way from this path — the
        // webhook handler calls `write_status_update` directly, never
        // `update`, so there is no `self.github` in scope on that path and
        // no way for this call to be what a webhook delivery triggers.
        if let Some(new_status) = &status {
            self.sync_status_to_github(&report, new_status).await;
        }

        Ok(report)
    }

    /// GitHub → dashboard. A miss (the issue isn't one of ours — this repo's
    /// webhook fires for every issue, not just crash reports) is a no-op,
    /// not an error. Deliberately does not call back out to GitHub: this is
    /// the one-way half of the sync, the other half lives in `update`.
    #[tracing::instrument(skip_all)]
    pub async fn sync_status_from_github(&self, issue_number: i32, status: &str) -> Result<()> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM crash_reports WHERE github_issue_number = $1")
                .bind(issue_number)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

        let Some(id) = id else { return Ok(()) };

        write_status_update(&self.db, id, Some(status), None).await?;
        tracing::info!(%id, issue = issue_number, status, "crash report status synced from a GitHub webhook");
        audit::record(
            &self.db,
            None,
            "GitHub",
            "crash_report.status_changed",
            "crash_report",
            id,
            serde_json::json!({ "status": status, "source": "github_webhook" }),
        )
        .await;
        Ok(())
    }

    /// Best-effort: a report with no linked issue, or a GitHub API hiccup,
    /// never blocks the dashboard-side status change that triggered this —
    /// the write already committed by the time this runs.
    async fn sync_status_to_github(&self, report: &CrashReport, new_status: &str) {
        let Some(number) = report.github_issue_number else { return };
        let (state, reason) = match new_status {
            "done" => ("closed", Some("completed")),
            "wont_fix" => ("closed", Some("not_planned")),
            _ => ("open", None),
        };
        if let Err(e) = self.github.update_issue_state(number, state, reason).await {
            tracing::warn!(error = %e, issue = number, "failed to sync crash report status to GitHub");
        }
    }

    /// One click, one issue: a report already carrying a `github_issue_url`
    /// refuses rather than opening a duplicate.
    #[tracing::instrument(skip_all)]
    pub async fn publish_to_github(&self, actor: &AdminUser, id: Uuid) -> Result<CrashReport> {
        let report = self.get(id).await?;
        if report.github_issue_url.is_some() {
            return Err(AppError::Conflict("already published to GitHub".to_string()));
        }

        // reported_by is a dashboard_users id, not a name — resolve it so a
        // manually-filed report reads as "filed by a real person", the same
        // way an actual user's issue would carry a name, not a bare UUID.
        let reporter_name = match report.reported_by {
            Some(user_id) => sqlx::query_scalar::<_, String>("SELECT name FROM dashboard_users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?,
            None => None,
        };

        let title = format!("[{}] {}", report.platform, report.title);
        let body = issue_body(&report, reporter_name.as_deref(), self.dashboard_url.as_deref());
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
        audit::record(
            &self.db,
            Some(actor.id),
            &actor.name,
            "crash_report.github_issue_published",
            "crash_report",
            report.id,
            serde_json::json!({ "issue_number": issue.number, "issue_url": issue.url }),
        )
        .await;
        Ok(report)
    }
}

/// The actual status/severity write, shared by both sync directions.
/// Deliberately a free function taking `db: &PgPool` rather than a method
/// on `CrashReportService` — there is no `self` in scope here, so there is
/// no `self.github` to call, which is what makes the webhook path
/// structurally incapable of triggering an outbound GitHub call rather than
/// merely convention not to.
///
/// Every expression here reads the row as it stood before this statement,
/// `status` included — so the CASE below is comparing against the *old*
/// status even though `status` itself is also being written a few lines up.
/// Newly done stamps `resolved_at`; moved to anything else clears it; a
/// severity-only update (`$2` NULL) or marking an already-done report done
/// again leaves it alone. `wont_fix` deliberately does not set it — that is
/// a closed state, not a fixed one.
async fn write_status_update(
    db: &PgPool,
    id: Uuid,
    status: Option<&str>,
    severity: Option<&str>,
) -> Result<CrashReport> {
    sqlx::query_as::<_, CrashReport>(
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
    .bind(status)
    .bind(severity)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or(AppError::ProductNotFound)
}

/// Reads like a person filed it, because as much of it as the data allows
/// now actually is a person: who reported it (resolved from `reported_by`,
/// not left as a bare id), whether it was them or the app itself, and every
/// field the report actually carries — not just the four that happened to
/// be in the original version of this function. Previously silent gaps
/// (`source`, `reported_by`, `device_id`, `metadata`) all show up here now.
fn issue_body(report: &CrashReport, reporter_name: Option<&str>, dashboard_url: Option<&str>) -> String {
    let mut body = String::new();

    body.push_str("## Description\n\n");
    match &report.description {
        Some(description) if !description.trim().is_empty() => {
            body.push_str(description);
        }
        _ => body.push_str("_No description was provided with this report._"),
    }
    body.push_str("\n\n");

    body.push_str("## Environment\n\n| | |\n|---|---|\n");
    body.push_str(&format!("| Platform | {} |\n", report.platform));
    body.push_str(&format!("| Severity | {} |\n", severity_label(&report.severity)));
    body.push_str(&format!("| Source | {} |\n", source_label(&report.source, reporter_name)));
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
    if let Some(device_id) = &report.device_id {
        // A loose correlation id, not an identity — same framing as the API
        // docs use elsewhere. Useful for spotting "this device again" across
        // separate issues without being anything to look someone up by.
        body.push_str(&format!("| Device correlation id | `{device_id}` |\n"));
    }

    if let Some(trace) = &report.stack_trace {
        body.push_str("\n## Stack Trace\n\n<details><summary>Expand</summary>\n\n```\n");
        body.push_str(trace);
        body.push_str("\n```\n</details>\n");
    }

    if !report.metadata.is_null() && report.metadata != serde_json::json!({}) {
        let pretty = serde_json::to_string_pretty(&report.metadata).unwrap_or_default();
        body.push_str("\n## Additional Metadata\n\n<details><summary>Expand</summary>\n\n```json\n");
        body.push_str(&pretty);
        body.push_str("\n```\n</details>\n");
    }

    body.push_str("\n---\n");
    body.push_str(&format!("Filed from the TrueLabel dashboard — crash report `{}`.", report.id));
    if let Some(base) = dashboard_url {
        body.push_str(&format!(" [View in dashboard]({base}/dashboard/crash-reports/{}).", report.id));
    }
    body
}

fn severity_label(severity: &str) -> String {
    let emoji = match severity {
        "critical" => "🔴",
        "high" => "🟠",
        "medium" => "🟡",
        "low" => "🟢",
        _ => "⚪",
    };
    format!("{emoji} {}", capitalize(severity))
}

/// "as if an actual user posted this" is exactly backwards for an app-
/// sourced crash — nobody typed it, MetricKit did, and saying so plainly is
/// more honest than dressing it up as a person. A manual report is the one
/// case a name belongs here, so that one gets it.
fn source_label(source: &str, reporter_name: Option<&str>) -> String {
    match (source, reporter_name) {
        ("manual", Some(name)) => format!("✍️ Filed manually by **{name}**"),
        ("manual", None) => "✍️ Filed manually by a staff member".to_string(),
        ("app", _) => "🤖 Reported automatically by the app (MetricKit)".to_string(),
        (other, _) => capitalize(other),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Every field `submit` and `create_manual` share — only where the row ends
/// up (source, device_id vs. reported_by) differs between them.
struct ValidatedReport {
    platform: String,
    title: String,
    description: Option<String>,
    stack_trace: Option<String>,
    app_version: Option<String>,
    os_version: Option<String>,
    device_model: Option<String>,
    severity: String,
    device_id: Option<String>,
    metadata: serde_json::Value,
}

fn validate_submission(req: &SubmitCrashReportRequest) -> Result<ValidatedReport> {
    let severity = match &req.severity {
        Some(s) => one_of("severity", s, SEVERITIES)?,
        None => "medium".to_string(),
    };
    Ok(ValidatedReport {
        platform: one_of("platform", &req.platform, PLATFORMS)?,
        title: bounded("title", &req.title, 1, MAX_TITLE_LEN)?,
        description: optional_bounded("description", req.description.as_deref(), MAX_DESCRIPTION_LEN)?,
        stack_trace: optional_bounded("stack_trace", req.stack_trace.as_deref(), MAX_STACK_TRACE_LEN)?,
        app_version: optional_bounded("app_version", req.app_version.as_deref(), MAX_SHORT_FIELD_LEN)?,
        os_version: optional_bounded("os_version", req.os_version.as_deref(), MAX_SHORT_FIELD_LEN)?,
        device_model: optional_bounded("device_model", req.device_model.as_deref(), MAX_SHORT_FIELD_LEN)?,
        severity,
        device_id: optional_bounded("device_id", req.device_id.as_deref(), 128)?,
        metadata: req.metadata.clone().unwrap_or_else(|| serde_json::json!({})),
    })
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
