-- Phase 18 (OKF-01, D-03): OKF conformance stats per vault.
--
--   okf_json  serialized crate::okf::OkfStats (version, concepts, total,
--             has_index, has_log, scanned_at) written at refresh/register.
--             NULL = never scanned. Opaque TEXT to the store — content is
--             validated in Rust via serde, never in SQL (D-03).

ALTER TABLE vaults ADD COLUMN okf_json TEXT;
