use crate::{
    auth::{AdminUser, ClientIp},
    error::Result,
    models::{
        ApiResponse, CrashReport, CrashReportPage, GitHubIssueRef, ListCrashReportsQuery,
        SubmitCrashReportRequest, UpdateCrashReportRequest,
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use uuid::Uuid;

/// Per address. Generous — a crash loop on a real bug can genuinely report
/// itself dozens of times in an hour, and the cost of accepting one too many
/// is far lower than the cost of silently dropping a real crash.
const REPORTS_PER_HOUR: u32 = 200;
const HOUR: u64 = 3600;

/// Unauthenticated: a crash can happen before a device finishes
/// registering, so filing one can never require a token.
pub async fn submit(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<SubmitCrashReportRequest>,
) -> Result<Json<ApiResponse<CrashReport>>> {
    state.limit("crash_report", &ip, REPORTS_PER_HOUR, HOUR).await?;
    let report = state.crash_report_service.submit(&body).await?;
    Ok(Json(ApiResponse::success(report, false)))
}

pub async fn list(
    State(state): State<AppState>,
    _user: AdminUser,
    Query(query): Query<ListCrashReportsQuery>,
) -> Result<Json<ApiResponse<CrashReportPage>>> {
    let page = state.crash_report_service.list(&query).await?;
    Ok(Json(ApiResponse::success(page, false)))
}

/// Staff filing a report by hand — any signed-in account, the same as
/// triaging. Not a privileged action the way publishing to GitHub is.
pub async fn create_manual(
    State(state): State<AppState>,
    user: AdminUser,
    Json(body): Json<SubmitCrashReportRequest>,
) -> Result<Json<ApiResponse<CrashReport>>> {
    let report = state.crash_report_service.create_manual(user.id, &body).await?;
    Ok(Json(ApiResponse::success(report, false)))
}

pub async fn get(
    State(state): State<AppState>,
    _user: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<CrashReport>>> {
    let report = state.crash_report_service.get(id).await?;
    Ok(Json(ApiResponse::success(report, false)))
}

pub async fn update(
    State(state): State<AppState>,
    _user: AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCrashReportRequest>,
) -> Result<Json<ApiResponse<CrashReport>>> {
    let report = state.crash_report_service.update(id, &body).await?;
    Ok(Json(ApiResponse::success(report, false)))
}

/// The one action gated to `admin` specifically rather than any signed-in
/// staff account — it reaches out to a third party and leaves a permanent
/// public trace, unlike triaging a report's status.
pub async fn publish_to_github(
    State(state): State<AppState>,
    user: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<GitHubIssueRef>>> {
    user.require_admin()?;
    let report = state.crash_report_service.publish_to_github(id).await?;
    let issue = GitHubIssueRef {
        number: report.github_issue_number.unwrap_or_default(),
        url: report.github_issue_url.clone().unwrap_or_default(),
    };
    Ok(Json(ApiResponse::success(issue, false)))
}
