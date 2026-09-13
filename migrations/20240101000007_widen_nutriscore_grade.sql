-- Widen nutriscore_grade from VARCHAR(1) to VARCHAR(20) to handle arbitrary grades or statuses (e.g., 'not-applicable', 'unknown')
ALTER TABLE products
  ALTER COLUMN nutriscore_grade TYPE VARCHAR(20);
