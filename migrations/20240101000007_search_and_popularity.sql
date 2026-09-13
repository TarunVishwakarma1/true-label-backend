-- Name/brand search and "popular near you" need two things the schema
-- didn't have: a trigram index for fuzzy text matching, and a per-product
-- look-up counter.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

ALTER TABLE products
  ADD COLUMN lookup_count INT NOT NULL DEFAULT 0;

CREATE INDEX idx_products_name_trgm ON products USING gin (product_name gin_trgm_ops);
CREATE INDEX idx_products_brand_trgm ON products USING gin (brand gin_trgm_ops);
CREATE INDEX idx_products_country_popularity ON products (country, lookup_count DESC);
