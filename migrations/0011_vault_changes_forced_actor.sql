-- Phase 15 (CONF-04, D-05): force-overwrite audit columns.
--
--   forced  1 when the write overwrote a conflicting file after the user
--           explicitly confirmed "Upload anyway" (D-02/D-03); 0 otherwise.
--           Force-overwrites are recorded as `modified` rows — the kind
--           vocabulary from migration 0010 stays exactly added/modified/
--           removed (D-05, CHECK constraint untouched).
--   actor   email of the user who performed the write, populated on WebDAV
--           PUTs, upload updates and force-overwrites (D-06); '' when the
--           actor is unknown (e.g. git refresh paths).

ALTER TABLE vault_changes ADD COLUMN forced INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vault_changes ADD COLUMN actor TEXT NOT NULL DEFAULT '';
