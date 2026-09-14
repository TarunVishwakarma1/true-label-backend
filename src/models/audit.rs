use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    pub actor_name: String,
    pub action: String,
    pub target_type: String,
    pub target_id: Uuid,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct ListAuditLogQuery {
    pub action: Option<String>,
    pub target_type: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

#[derive(Debug, Serialize)]
pub struct AuditLogPage {
    pub items: Vec<AuditLogEntry>,
    pub total: i64,
}
