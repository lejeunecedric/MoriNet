-- Phase 12 (REQ-VOPT-001, REQ-VOPT-002): per-vault display options.
--
--   keep_page_numbers  owner toggle: keep mdBook's chapter page numbers
--                      visible for this vault (0 = hidden, the global rule).
--   theme              mdBook theme name; '' means mdBook default (auto).
--                      Server-side whitelist: light|navy|rust|coal|ayu.

ALTER TABLE vaults ADD COLUMN keep_page_numbers INTEGER NOT NULL DEFAULT 0;
ALTER TABLE vaults ADD COLUMN theme TEXT NOT NULL DEFAULT '';
