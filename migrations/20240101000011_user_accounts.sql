-- Sign in with Apple turns a device row into an account, so the profile and
-- Plus survive a new phone. The device id stays the primary key: everything
-- still works signed out, and signing in only attaches an identity to the
-- row that already exists.
ALTER TABLE users
  ADD COLUMN apple_user_id VARCHAR(255),
  ADD COLUMN email VARCHAR(320),
  ADD COLUMN display_name VARCHAR(120),
  ADD COLUMN linked_at TIMESTAMPTZ;

-- One Apple identity owns at most one row; linking on a second device moves
-- it rather than duplicating it.
CREATE UNIQUE INDEX idx_users_apple_user_id ON users (apple_user_id) WHERE apple_user_id IS NOT NULL;
