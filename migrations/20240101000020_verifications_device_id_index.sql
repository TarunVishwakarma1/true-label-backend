-- find_needs_verification's NOT EXISTS subquery filters on this column for
-- every call to the crowd-verification queue — previously unindexed, so
-- every call was a sequential scan over the whole verifications table.
CREATE INDEX idx_verifications_device_id ON verifications (device_id) WHERE device_id IS NOT NULL;
