-- A four-state status ('open'/'investigating'/'resolved'/'wont_fix') read
-- as too coarse once real triage started — no room for "we've seen it but
-- nobody's picked it up yet" vs "someone's actively confirming it" vs
-- "someone's actively fixing it". Replaced with a JIRA/Bugzilla-shaped
-- funnel instead of just bolting new values onto the old set, since half
-- the old names would've been redundant with the new ones (open vs
-- submitted, resolved vs done).
--
-- submitted    -- filed, untouched (was 'open')
-- pending      -- triaged, waiting on more info or a decision
-- in_review    -- someone is actively confirming/reproducing it
-- in_progress  -- confirmed, being fixed (was 'investigating')
-- done         -- fixed (was 'resolved')
-- wont_fix     -- closed, won't be addressed (unchanged)
UPDATE crash_reports SET status = 'submitted' WHERE status = 'open';
UPDATE crash_reports SET status = 'in_progress' WHERE status = 'investigating';
UPDATE crash_reports SET status = 'done' WHERE status = 'resolved';

ALTER TABLE crash_reports ALTER COLUMN status SET DEFAULT 'submitted';
