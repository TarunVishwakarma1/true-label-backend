-- In-app notification center for dashboard users.
CREATE TABLE dashboard_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Optional specific recipient user (NULL means broadcast or target_role-based)
    user_id UUID REFERENCES dashboard_users(id) ON DELETE CASCADE,
    -- Optional target role: 'admin', 'member', 'new-user', or NULL
    target_role VARCHAR(20),
    title VARCHAR(255) NOT NULL,
    message TEXT NOT NULL,
    category VARCHAR(50) NOT NULL, -- 'user_registration', 'access_request', 'role_change', 'permission_change', 'crash_report'
    link VARCHAR(255),
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dashboard_notifications_user ON dashboard_notifications (user_id, is_read, created_at DESC);
CREATE INDEX idx_dashboard_notifications_role ON dashboard_notifications (target_role, is_read, created_at DESC);
