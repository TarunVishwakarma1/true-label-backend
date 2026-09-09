CREATE TABLE ocr_submissions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  barcode VARCHAR(20) NOT NULL,
  country VARCHAR(2) NOT NULL,
  image_url TEXT NOT NULL,
  extracted_text TEXT NOT NULL,
  parsed_nutrition JSONB,
  confidence_score FLOAT,
  status VARCHAR(50) DEFAULT 'pending_verification',
  final_product_id UUID REFERENCES products(id) ON DELETE SET NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_ocr_submissions_barcode ON ocr_submissions(barcode);
CREATE INDEX idx_ocr_submissions_status ON ocr_submissions(status);
