-- Who changed what: role changes, password resets, invites, removals,
-- crash-report status/GitHub events, product edits/verifications. Insert-
-- only, so no updated_at.
CREATE TABLE audit_log (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  -- Denormalized snapshot, not just a join through actor_id: survives the
  -- actor's row going away, and covers the one actor with no
  -- dashboard_users row at all — GitHub, for webhook-driven status syncs.
  actor_id UUID REFERENCES dashboard_users(id) ON DELETE SET NULL,
  actor_name TEXT NOT NULL,
  -- App-enforced, no CHECK — same convention as role/status/platform
  -- elsewhere in this schema.
  action VARCHAR(60) NOT NULL,
  target_type VARCHAR(40) NOT NULL,
  target_id UUID NOT NULL,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_log_created_at ON audit_log (created_at DESC);
CREATE INDEX idx_audit_log_target ON audit_log (target_type, target_id);
