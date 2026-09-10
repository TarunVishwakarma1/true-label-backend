ALTER TABLE products
  ADD COLUMN category TEXT;

-- Same-shelf alternatives always filter by category + country together.
CREATE INDEX idx_products_category_country ON products (category, country);
