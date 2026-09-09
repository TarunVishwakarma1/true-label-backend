ALTER TABLE products
  ADD COLUMN additives JSONB,
  ADD COLUMN nova_group SMALLINT,
  ADD COLUMN nutriscore_grade VARCHAR(1),
  ADD COLUMN is_vegan BOOLEAN,
  ADD COLUMN is_vegetarian BOOLEAN,
  ADD COLUMN is_palm_oil_free BOOLEAN;
