-- Until now every per-user endpoint was addressed by device id in the URL,
-- and a device id is an identifier, not a secret: it travels in access logs,
-- proxies and analytics. Anyone holding one could read that person's
-- profile, cancel their Plus or delete their account.
--
-- A device now proves who it is with a bearer token it alone holds. Only the
-- SHA-256 of the token is stored, so a database leak does not hand over the
-- credentials themselves.
ALTER TABLE users
  ADD COLUMN token_hash CHAR(64),
  ADD COLUMN token_issued_at TIMESTAMPTZ;

CREATE UNIQUE INDEX idx_users_token_hash ON users (token_hash) WHERE token_hash IS NOT NULL;

-- Rows created before tokens existed were addressed purely by a guessable
-- identifier and can never be authenticated. There are no real users yet,
-- and leaving them would leave unreachable rows that nobody can delete.
DELETE FROM users WHERE token_hash IS NULL;
