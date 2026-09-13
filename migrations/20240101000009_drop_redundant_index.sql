-- products.barcode is already UNIQUE, which carries its own index, so this
-- one only cost write throughput.
DROP INDEX IF EXISTS idx_products_barcode;
