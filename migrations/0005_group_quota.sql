-- Per-group personal-vault quota (Phase 8 — access-model, REQ-GRP-005..008).
--
-- `groups.personal_vault_limit` caps how many PERSONAL vaults a member may
-- CREATE. NULL = unlimited. Shared/global vaults are granted per group and
-- never count toward this limit (REQ-VLT-008).
--
-- The effective limit for a user is the most permissive across their groups:
-- a NULL (unlimited) group beats any number, otherwise the max wins. A user
-- in no group at all has no configured limit (unrestricted).
ALTER TABLE groups ADD COLUMN personal_vault_limit INTEGER;
