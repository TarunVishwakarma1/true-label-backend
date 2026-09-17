use crate::error::AppError;
use crate::models::admin::{DashboardNotification, NotificationListResponse};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NotificationService {
    db: PgPool,
}

impl NotificationService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Creates a new in-app notification.
    pub async fn create(
        &self,
        user_id: Option<Uuid>,
        target_role: Option<&str>,
        title: &str,
        message: &str,
        category: &str,
        link: Option<&str>,
    ) -> Result<DashboardNotification, AppError> {
        let notification = sqlx::query_as::<_, DashboardNotification>(
            r#"
            INSERT INTO dashboard_notifications (user_id, target_role, title, message, category, link, is_read)
            VALUES ($1, $2, $3, $4, $5, $6, FALSE)
            RETURNING id, user_id, target_role, title, message, category, link, is_read, created_at
            "#,
        )
        .bind(user_id)
        .bind(target_role)
        .bind(title)
        .bind(message)
        .bind(category)
        .bind(link)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(notification)
    }

    /// Lists recent in-app notifications for the given user and role, along with unread count.
    pub async fn list_for_user(
        &self,
        user_id: Uuid,
        role: &str,
        limit: i64,
    ) -> Result<NotificationListResponse, AppError> {
        let limit = limit.clamp(1, 50);

        let items = sqlx::query_as::<_, DashboardNotification>(
            r#"
            SELECT id, user_id, target_role, title, message, category, link, is_read, created_at
            FROM dashboard_notifications
            WHERE user_id = $1 OR (user_id IS NULL AND (target_role IS NULL OR target_role = $2))
            ORDER BY created_at DESC
            LIMIT $3
            "#,
        )
        .bind(user_id)
        .bind(role)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let unread_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM dashboard_notifications
            WHERE is_read = FALSE
              AND (user_id = $1 OR (user_id IS NULL AND (target_role IS NULL OR target_role = $2)))
            "#,
        )
        .bind(user_id)
        .bind(role)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(NotificationListResponse {
            items,
            unread_count: unread_count.0,
        })
    }

    /// Marks a single notification as read for a given user.
    pub async fn mark_as_read(
        &self,
        notification_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> Result<(), AppError> {
        let result = sqlx::query(
            r#"
            UPDATE dashboard_notifications
            SET is_read = TRUE
            WHERE id = $1
              AND (user_id = $2 OR (user_id IS NULL AND (target_role IS NULL OR target_role = $3)))
            "#,
        )
        .bind(notification_id)
        .bind(user_id)
        .bind(role)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AppError::InvalidRequest(
                "Notification not found".to_string(),
            ));
        }

        Ok(())
    }

    /// Marks all unread notifications for a user/role as read.
    pub async fn mark_all_as_read(&self, user_id: Uuid, role: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE dashboard_notifications
            SET is_read = TRUE
            WHERE is_read = FALSE
              AND (user_id = $1 OR (user_id IS NULL AND (target_role IS NULL OR target_role = $2)))
            "#,
        )
        .bind(user_id)
        .bind(role)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }
}
