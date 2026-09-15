-- Sibling of apple_user_id (20240101000011_user_accounts.sql): a second
-- sign-in provider on the same device row, not a separate accounts table.
ALTER TABLE users
  ADD COLUMN google_user_id VARCHAR(255);

CREATE UNIQUE INDEX idx_users_google_user_id ON users (google_user_id) WHERE google_user_id IS NOT NULL;
