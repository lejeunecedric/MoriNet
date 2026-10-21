-- Where a vault came from (Phase 8 follow-up): lets a loaded vault be
-- refreshed from its upstream without re-entering the URL (REQ-VLT-005).
-- NULL = local/legacy vault with no upstream to pull.
ALTER TABLE vaults ADD COLUMN source_url TEXT;
