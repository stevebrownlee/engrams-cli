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

## 2026-09-05T03:28:12Z — phase 3 of spec 0002-schema-formation (staging, gates, scan command)

- moderate / spec-conformance — AC-3 "failed gate named" unmet: candidate output has only gates_pass bool + raw metrics; thresholds are private consts so consumers can't name the failed gate (src/ops/schemas/scan.rs:242)
- moderate / missing-test — scan_writes_only_schema_candidates enumerates 11 of 18 tables; omits code_nodes (in scan's own read path), active_contexts(+history), product_context(+history), session_closes, all FTS tables; sqlite_master enumeration would be exhaustive (src/ops/schemas/scan.rs:407)
- moderate / missing-test — assign() tie-break (Jaccard desc → cluster members asc → stored sig asc), one-to-one constraint, and drift-removed path implemented but untested; refactor could silently break identity selection (src/ops/schemas/scan.rs:132)
- minor / missing-test — no CLI-level test of `engrams schema scan` (parse→dispatch→print glue unexercised; cli.rs precedent covers other families at binary level) (src/ops/schemas/scan.rs:215)
- minor / robustness — staging upserts not in a transaction; mid-scan crash inflates stability_count of early rows on next scan; import.rs tx precedent (src/ops/schemas/scan.rs:227)
- minor / process — phase 3 spec_deviations null; orchestrator-approved UnionGraph.index removal (binary search over sorted nodes) unrecorded (src/ops/graph/louvain.rs:61)

Verified clean: writes confined to schema_candidates (full SQL audit: 2 SELECTs + UPDATE/INSERT on schema_candidates only; louvain SELECT-only); Jaccard tie-break correct and deterministic on read; post-upsert gate evaluation judged consistent with spec, not a contradiction — DDL inserts at stability_count=1 (current scan counts as a sighting) and gates_pass matches the row's stored state; flip at exactly 3 pinned by test. Drift columns match DDL semantics (last-match, skipped rows retain values). Output shape matches repo convention ({"status":"success",...}). clippy --all-targets 0 warnings after allow(dead_code) removal; cargo test schemas now matches 4 real tests (was vacuous for phase 2). Phase 2's two leftover bookkeeping minors since fixed: verification now "cargo test louvain", spec_deviations recorded, phase marked complete.

## 2026-09-05T03:52:00Z — re-review: phase 3 of spec 0002-schema-formation (retry 1 resolution check)

- moderate / spec-conformance — resolved-verified: per-gate gates objects {value, threshold, pass} + failed_gates array in scan output (scan.rs:258-275); AC-3 Then met. Note: no test pins failed_gates contents, only gates_pass flips.
- moderate / missing-test — resolved-verified: snapshot() enumerates all tables from sqlite_master with full row content (blob hex) and a natural-order fallback for WITHOUT ROWID FTS5 shadows; presence assert guards schema_candidates exclusion. 7/7 tests pass.
- moderate / missing-test — resolved-verified: all three requested cases covered (higher-J wins both orders; one-to-one with loser staging fresh, order-independent; drift-removed asserted at DB level: sig refreshed, drift 1/1, stability 2->3). Residual: equal-Jaccard comparator tie levels (members asc / sig asc) still unexercised — implementer's "exact ties" wording overstates.
- minor / missing-test — REMAINING: no binary-level test; grep confirms no CLI invocation of schema scan anywhere; tests/cli.rs untouched this retry.
- minor / robustness — resolved-verified: single unchecked_transaction wraps the upsert loop, commit at end, error path rolls back (scan.rs:231-279).
- minor / process — REMAINING: phase 3 spec_deviations still null (jq-verified); approved UnionGraph.index removal unrecorded.
- NEW minor / convention — retry introduced clippy noise (was 0-warning): unused `use std::fmt::Write as _;` in production scope (scan.rs:14; the only write! is in tests, which get the trait via use super::*), plus 3 clippy::cloned_ref_to_slice_refs in new tests (540, 541, 554).

Counts: 7 findings = 0 severe, 3 moderate (all resolved), 4 minor (2 remaining, 1 resolved, 1 new). Run: cargo test schemas 7 passed; clippy --all-targets 4 warnings total (1 bin, 3 test).

## 2026-09-05T04:22:00Z — re-review: phase 3 retry 2 (final scoped check)

- MOD-1 caveat closed: failed_gates contents pinned at scan.rs:433 (["density","stability"]) with gate value/threshold/pass pinned 434-441. CLI test asserts gates_pass only; JSON-shape pinning is unit-level.
- MOD-2 confirmed again: snapshot (scan.rs:448-503) is sqlite_master-driven, no hand list, FTS shadows covered incl. WITHOUT ROWID (%_config/%_idx) natural-order fallback, full row content with blob hex.
- MOD-3 residual closed: assign_tiebreaks_equal_jaccard_distinct_clusters_by_member_set (scan.rs:608-626) — genuinely J-equal distinct sets (left={1..5}, right={2..6}, row={1..6}; both 5/6), lexicographic member-set comparator (scan.rs:145) decides, both caller orders asserted. cargo test schemas: 9 passed.
- MIN-4 resolved-verified: test_schema_scan_stages_and_writes_nothing_else (tests/cli.rs:2176) drives the real binary, stability 1->2->3 with gates_pass flip only at 3, stable sig, zero-delta write audit (4 tables). Nuances: CLI write audit is a 4-table spot check, not exhaustive (exhaustive audit stays unit-level); gates flip via gates_pass not failed_gates; SQL-seeded anchors with documented rationale.
- MIN-6 REMAINS: phase 3 spec_deviations still null after retry 2 (jq re-verified); not claimed fixed this round.
- MIN-7 clippy resolved: 0 warnings verified.

Fabrication incident: retry 1's MOD-4 claim (CLI test exists) was fabricated by the implementer — I verified absence that round (grep, untouched tests/cli.rs) and marked MIN-4 remaining. Retry 2 delivered the genuine test; verified in-tree, run, passed. My retry-1 record was accurate against the then-tree; Main independently confirmed the retry-1 claim was false.

Counts: 7 findings = 0 severe, 3 moderate, 4 minor; 6 resolved-verified, 1 remaining (spec_deviations bookkeeping).
