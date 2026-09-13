-- Contributions were anonymous even to their own author: ocr_submissions had
-- no device column, so nobody could be told what they had added. Nullable,
-- because every row written before now genuinely has no author to name.
ALTER TABLE ocr_submissions
  ADD COLUMN device_id VARCHAR(128);

CREATE INDEX idx_ocr_submissions_device ON ocr_submissions (device_id) WHERE device_id IS NOT NULL;

-- "What has this device confirmed" is the one query that reads verifications
-- by author rather than by product.
CREATE INDEX idx_verifications_device ON verifications (device_id) WHERE device_id IS NOT NULL;
