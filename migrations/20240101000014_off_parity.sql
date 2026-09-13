-- Our response was a lossy flattening of what Open Food Facts publishes, and
-- some of what it dropped was safety information.
--
-- `traces` is the "may contain" line. It was not ingested at all, which for
-- someone with a peanut allergy is the most important line on the pack.
--
-- Tag arrays replace the comma-joined `allergens` string the client was
-- parsing itself. Nullable everywhere and kept apart from an empty array:
-- NULL is "Open Food Facts doesn't publish this", `[]` is "declared none",
-- and those are different claims to make about what is in food.
ALTER TABLE products
  ADD COLUMN allergens_tags JSONB,
  ADD COLUMN traces_tags JSONB,
  ADD COLUMN labels_tags JSONB,
  ADD COLUMN categories_tags JSONB,
  ADD COLUMN nutrient_levels JSONB,
  ADD COLUMN serving_size TEXT,
  ADD COLUMN serving_quantity DOUBLE PRECISION,
  ADD COLUMN quantity TEXT,
  ADD COLUMN nutriscore_score INT,
  ADD COLUMN ecoscore_grade VARCHAR(20),
  ADD COLUMN completeness REAL;

-- Products certified gluten-free, organic and so on are worth finding
-- directly rather than by reading every ingredient list.
CREATE INDEX idx_products_labels ON products USING gin (labels_tags);
