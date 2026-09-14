-- 20240101000020 added idx_verifications_device_id, not realizing
-- idx_verifications_device (20240101000012_attribute_contributions.sql)
-- already indexes the exact same column with the exact same partial
-- predicate — a genuine duplicate, not two different indexes. Drop the
-- redundant one rather than edit 20240101000020 after it already applied.
DROP INDEX IF EXISTS idx_verifications_device_id;
