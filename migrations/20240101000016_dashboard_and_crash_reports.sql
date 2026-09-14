-- The internal admin dashboard (web/apps/dashboard) is a second, unrelated
-- identity system from `users` — that table is one row per anonymous
-- device with no password, this one is a handful of named staff accounts
-- with real credentials. Keeping them apart means a bug in one can never
-- leak into the other's authority.
--
-- Auth follows the same shape already proven by device tokens: a bearer
-- token is minted at login, only its SHA-256 is stored, and the token
-- itself is never recoverable from the database. A password on top of that
-- (this identity is worth phishing, a device token is not) so it is hashed
-- with Argon2, not SHA-256.
CREATE TABLE dashboard_users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(120) NOT NULL,
  email VARCHAR(255) NOT NULL,
  date_of_birth DATE,
  occupation VARCHAR(120),
  password_hash TEXT NOT NULL,
  -- 'admin' | 'member'. Checked in the application layer, same convention
  -- as every other enum-shaped column in this schema (see: products.source,
  -- ocr_submissions.status) — no CHECK constraint anywhere else here either.
  role VARCHAR(20) NOT NULL DEFAULT 'member',
  token_hash CHAR(64),
  token_issued_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Case-insensitive: "Tarun@x.com" and "tarun@x.com" are the same account,
-- and a login form that lowercases inconsistently must not create two.
CREATE UNIQUE INDEX idx_dashboard_users_email ON dashboard_users (LOWER(email));
CREATE UNIQUE INDEX idx_dashboard_users_token_hash ON dashboard_users (token_hash) WHERE token_hash IS NOT NULL;

-- Crash reports: submitted by the app itself (unauthenticated — a crash can
-- happen before a device even finishes registering, so this cannot require
-- a token) or filed by staff reading a TestFlight/App Store Connect log by
-- hand. `device_id` is a loose reference the way `verifications.device_id`
-- is elsewhere in this schema, not a foreign key — a crash report must be
-- storable even for a device row that no longer exists.
CREATE TABLE crash_reports (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  -- 'ios' | 'backend' | 'web'
  platform VARCHAR(20) NOT NULL,
  title VARCHAR(255) NOT NULL,
  description TEXT,
  stack_trace TEXT,
  app_version VARCHAR(50),
  os_version VARCHAR(50),
  device_model VARCHAR(100),
  -- 'low' | 'medium' | 'high' | 'critical'
  severity VARCHAR(20) NOT NULL DEFAULT 'medium',
  -- 'open' | 'investigating' | 'resolved' | 'wont_fix'
  status VARCHAR(20) NOT NULL DEFAULT 'open',
  -- 'app' (self-reported by a crash handler) | 'manual' (filed by staff)
  source VARCHAR(20) NOT NULL DEFAULT 'app',
  device_id VARCHAR(255),
  reported_by UUID REFERENCES dashboard_users(id) ON DELETE SET NULL,
  -- Set once, by the "publish to GitHub" button. NULL means not published
  -- yet — the thing that stops a second click from opening a duplicate
  -- issue.
  github_issue_number INT,
  github_issue_url TEXT,
  -- Free-form extra context a client wants to attach (locale, network
  -- state, feature flags) without a migration for every new field.
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  resolved_at TIMESTAMPTZ
);

CREATE INDEX idx_crash_reports_status ON crash_reports (status);
CREATE INDEX idx_crash_reports_platform ON crash_reports (platform);
CREATE INDEX idx_crash_reports_created_at ON crash_reports (created_at DESC);
CREATE INDEX idx_crash_reports_device_id ON crash_reports (device_id) WHERE device_id IS NOT NULL;
