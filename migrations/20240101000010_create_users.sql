-- One row per device. There are no accounts in this app: identifierForVendor
-- is the identity, so the profile survives reinstall-in-place and nothing
-- personal is collected to key it on.
--
-- Plus has no payments yet. `plus_since` set with a NULL `plus_expires_at`
-- means "on, no expiry" — that is the free period. A paid activation will
-- set a real expiry, and `plus_source` records which of the two it was.
CREATE TABLE users (
  device_id VARCHAR(128) PRIMARY KEY,
  country VARCHAR(2) NOT NULL DEFAULT 'IN',
  dietary_preferences JSONB NOT NULL DEFAULT '[]'::jsonb,
  plus_since TIMESTAMPTZ,
  plus_expires_at TIMESTAMPTZ,
  plus_source VARCHAR(20),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- "How many people are on Plus right now" is the only question we ask across
-- rows rather than by key.
CREATE INDEX idx_users_plus ON users (plus_since) WHERE plus_since IS NOT NULL;
