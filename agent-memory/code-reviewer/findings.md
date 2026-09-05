# Code Reviewer Findings

## 2026-09-05T02:10:47Z — phase 1 of spec 0002-schema-formation

- severe / missing-test — AC-11's "fresh database contains every table this feature defines" has no test; fresh-init (SCHEMA) and v11-to-v12 DDL outputs are unasserted (src/schema.rs:403)
- moderate / missing-test — no v11-to-v12 migration test following the test_migration_v2_to_v3 precedent; lossless upgrade of existing databases unverified (src/db.rs:88)

Notes: SCHEMA additions and MIGRATION_V12 verified byte-identical (decision #66 lesson followed); schemas_fts triggers match the system_patterns/decisions/custom_data precedent; member_of RelSpec matches the derived_from precedent and ItemType::as_str runtime vocabulary (underscore node kinds at the ontology boundary, hyphenated entity types remain CLI-facing). No invariant violations; no rules in .pilot/rules/ apply to Rust backend phases.

## 2026-09-05T02:18:00Z — re-review: phase 1 of spec 0002-schema-formation (retry 1 resolution check)

Both findings resolved-verified. test_fresh_db_has_schema_formation_objects asserts all five schema-formation objects plus idx_retrieval_surfaces_node exist after fresh init (severe); test_migration_v11_to_v12 rewinds to v11 (drops feature objects, seeds a decisions row, stamps user_version=11), migrates, and asserts objects restored + row preserved (moderate). Ran both: 2 passed, 0 failed.
Judgment on the v11-rewind trick: faithful. MIGRATION_V12 verified purely additive (CREATE ... IF NOT EXISTS only), so current-shape minus the dropped objects equals the v11 shape; FTS shadow tables drop with their parent, the index drops with its table, trigger-before-FTS-table drop order is safe. Nice touch: version pins are derived from migrate's self-reported JSON "version" rather than hardcoded, keeping the tests self-maintaining across future bumps. No new issues in the added test code.

## 2026-09-05T02:49:56Z — phase 2 of spec 0002-schema-formation (detection core)

- severe / correctness — declared edges keyed by base-node insertion indices but consumed as sorted-union positions; scrambled whenever load order != sort order (any custom_data/code_nodes/link-endpoint kinds); overlay pairs correctly translated via NodeKey (src/ops/graph/louvain.rs:132)
- moderate / correctness — collapse self-loop double-push inflates internal mass 2x beyond the symmetric-replication convention (a==b hits both push statements); distorts the two-level objective though determinism/monotonicity hold (src/ops/graph/louvain.rs:336)
- minor / verification — recorded gate 'cargo test schemas' matches zero tests anywhere (tests/schemas.rs is phase 3); phase 2's real tests are inline under the binary target (specs/0002-schema-formation.progress.json:80)
- minor / spec-conformance — spec names src/ops/schemas/detect.rs; implementation is src/ops/graph/louvain.rs, unamended and unnoted (src/ops/graph/louvain.rs:1)
- minor / robustness — retrieval_surfaces ghosts (no FK, no uniqueness) enter the node universe via key extension from overlay groups; dedup/unknown-skip decision owed before phase 3 telemetry (src/ops/graph/louvain.rs:113)

Notes: all 13 inline tests pass (cargo test --bins louvain); test quality is behavioral — exact aggregated weights, known two-cluster topology with weak bridge, run-twice-identical at label and DB level, exact density 0.4 hub-spoke, empty-telemetry zero-edge assertion. Determinism mechanics verified: sorted/BTreeMap iteration throughout, HashMap used for lookups only, canonical label renumbering by first appearance. Derived edges never persisted (SELECT-only module). Dead-code seam matches repo pattern (allow + caller note naming phase 3). canonical_kind covers store spellings ('pattern', 'progress-entry', underscore forms) but omits parse_node's 'progress'/'custom' short aliases — no store path writes those, not flagged.

## 2026-09-05T03:04:15Z — re-review: phase 2 of spec 0002-schema-formation (retry 1 resolution check)

- severe / correctness — resolved-verified: declared edges translated through NodeKey-to-union index at the sole raw-index site (louvain.rs:137-146); new regression declared_edges_attach_to_true_endpoints_when_orders_diverge asserts true pairs carry weights and raw-reuse pairs stay 0.0. 15/15 pass.
- moderate / correctness — resolved-verified: single push for a==b behind `if a != b` (louvain.rs:348-352) yields textbook two-level objective. 15/15 pass.
- minor / robustness — resolved-verified: overlay group members filtered via base.contains() (louvain.rs:113-123); new regression surface_ghosts_do_not_extend_the_universe asserts ghost vanishes, real pair strength = 1.0 + co_weight(1). 15/15 pass.
- minor / verification — REMAINING: phase 2 verification still 'cargo test schemas', filter matches zero tests across all targets; claimed fix not in tree.
- minor / spec-conformance — REMAINING: no spec_deviations field in phase 2, spec unamended; claimed recording not in tree.

Notes: run was `cargo test --bins louvain` (15 passed, was 13; +2 regressions). The two remaining items are progress.json/spec bookkeeping edits, not code — Main's relayed claim that they were done contradicts verified repo state (grep + jq + git log all agree).
