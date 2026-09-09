CREATE TABLE products (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  barcode VARCHAR(20) NOT NULL UNIQUE,
  country VARCHAR(2) NOT NULL,
  product_name VARCHAR(255) NOT NULL,
  brand VARCHAR(255),
  image_url TEXT,
  nutrition_facts JSONB NOT NULL,
  ingredients TEXT,
  allergens TEXT,
  source VARCHAR(50) NOT NULL,
  verified BOOLEAN DEFAULT FALSE,
  verification_count INT DEFAULT 0,
  confidence_score FLOAT,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_products_barcode ON products(barcode);
CREATE INDEX idx_products_barcode_country ON products(barcode, country);
CREATE INDEX idx_products_verified ON products(verified);
