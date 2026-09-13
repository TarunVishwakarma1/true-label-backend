-- Our copy of a product was written once and never looked at again, so it
-- drifted from Open Food Facts forever. These two columns make "how far
-- behind are we" an answerable question, and let a read refresh a row that
-- has gone stale.
ALTER TABLE products
  ADD COLUMN off_synced_at TIMESTAMPTZ,
  ADD COLUMN off_last_modified BIGINT;

-- Existing rows were last in step with Open Food Facts when they were
-- created; claiming otherwise would hide how stale they already are.
UPDATE products SET off_synced_at = created_at WHERE source = 'open_food_facts';

CREATE INDEX idx_products_off_stale ON products (off_synced_at)
  WHERE source = 'open_food_facts';
