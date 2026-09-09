CREATE TABLE verifications (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
  barcode VARCHAR(20) NOT NULL,
  country VARCHAR(2) NOT NULL,
  device_id VARCHAR(255),
  verified BOOLEAN NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_verifications_product_id ON verifications(product_id);
CREATE INDEX idx_verifications_barcode_country ON verifications(barcode, country);
