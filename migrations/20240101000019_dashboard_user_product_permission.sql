-- Admins can always edit products; this is the "selected users" part —
-- an admin can additionally grant a specific member the same edit rights
-- without handing them the admin role itself. A flat app-enforced boolean,
-- same convention as `role` (no CHECK constraint) and `products.verified`.
-- It never covers the separate `/verify` action, which stays admin-only.
ALTER TABLE dashboard_users
  ADD COLUMN can_edit_products BOOLEAN NOT NULL DEFAULT FALSE;
