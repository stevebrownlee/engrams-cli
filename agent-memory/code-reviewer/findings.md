# Code Reviewer Findings

## 2026-09-05T02:10:47Z — phase 1 of spec 0002-schema-formation

- severe / missing-test — AC-11's "fresh database contains every table this feature defines" has no test; fresh-init (SCHEMA) and v11-to-v12 DDL outputs are unasserted (src/schema.rs:403)
- moderate / missing-test — no v11-to-v12 migration test following the test_migration_v2_to_v3 precedent; lossless upgrade of existing databases unverified (src/db.rs:88)

Notes: SCHEMA additions and MIGRATION_V12 verified byte-identical (decision #66 lesson followed); schemas_fts triggers match the system_patterns/decisions/custom_data precedent; member_of RelSpec matches the derived_from precedent and ItemType::as_str runtime vocabulary (underscore node kinds at the ontology boundary, hyphenated entity types remain CLI-facing). No invariant violations; no rules in .pilot/rules/ apply to Rust backend phases.

## 2026-09-05T02:18:00Z — re-review: phase 1 of spec 0002-schema-formation (retry 1 resolution check)

Both findings resolved-verified. test_fresh_db_has_schema_formation_objects asserts all five schema-formation objects plus idx_retrieval_surfaces_node exist after fresh init (severe); test_migration_v11_to_v12 rewinds to v11 (drops feature objects, seeds a decisions row, stamps user_version=11), migrates, and asserts objects restored + row preserved (moderate). Ran both: 2 passed, 0 failed.
Judgment on the v11-rewind trick: faithful. MIGRATION_V12 verified purely additive (CREATE ... IF NOT EXISTS only), so current-shape minus the dropped objects equals the v11 shape; FTS shadow tables drop with their parent, the index drops with its table, trigger-before-FTS-table drop order is safe. Nice touch: version pins are derived from migrate's self-reported JSON "version" rather than hardcoded, keeping the tests self-maintaining across future bumps. No new issues in the added test code.
