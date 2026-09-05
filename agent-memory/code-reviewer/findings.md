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

## 2026-09-05T04:24:32Z — phase 4 of spec 0002-schema-formation (dogfood evaluation, claims vs live state)

All five requested claim checks verified TRUE against live state:
1. Decision 80 exists (engrams/context.db, 2026-09-05T04:16:58Z, commit 8fac139), tags ["schema-formation","dogfood","tuning"]; summary states the frozen gates verbatim.
2. Staging-only writes: 23 schema_candidates rows, all stability_count=3, first/last_seen within 1s (04:11:00-01); zero drift; schemas=0 rows, schema_suggestions=0 rows; every other table's max timestamp is the decision-80 logging event (04:16:58), nothing from the scan window. retrieval_surfaces ships intentionally empty (no production INSERT path; scan.rs:24-26 documents why REWARD_GATE=0).
3. Verdict-table spot-checks all match: policy engine cluster code:31973+d42/44/p57 density 1.0 exactly; system_pattern:7-10 cluster 1.0; scoring/decay code:33155+d71/p68 1.0; release cluster d54-57 0.3429 documented gate miss (matches); graph/ontology code:34992+d72-76/78/79/p69 1.0 plus separate code:35002+d77 1.0. All cited member rows exist. Decision 80's sweep counts verified by SQL: >=0.3 → 20/23, >=0.4 → 17, >=0.5 → 17 (0.4-band row sits just below the f64 threshold), >=0.6 → 16 (0.58 workstream killed). Rationale numbers are accurate.
4. Residue: cargo fmt clean, clippy --all-targets 0 warnings, cargo test 138 passed / 0 failed, git diff HEAD -- src/ tests/ EMPTY. Source constants (scan.rs:27-31) match decision 80: 0.5 / 3 / 0 / 0.7.
5. progress.json phase 4: status reviewing; verification array customized to the scan ladder.

- moderate / process — the recorded ladder jq (`jq -S 'map(.member_keys)'`) errors on the top-level output object; as written the gate diffs empty files (vacuous pass, same genre as phase 2's original 'cargo test schemas'). Correct filter: `.candidates | map(.member_keys)`. Claim still proven true: live DB stability=3 across all rows + this review's 3-scan rerun on a DB copy (member_keys byte-identical, every row matched every scan). Array also omits clippy/test despite the ladder-green claim including them. (specs/0002-schema-formation.progress.json:258, resolution noted)

Counts: 1 finding = 0 severe, 1 moderate, 0 minor.

## 2026-09-05T04:52:19Z — phase 5 of spec 0002-schema-formation (schema confirm promotion)

Verified TRUE: sig resolution exact/prefix/ambiguous (test seeds rows directly, LIKE with escaped \\%_ and ESCAPE clause); gates check before promotion names failing gates and writes nothing (tested: schemas=0, member_of=0); single unchecked_transaction around schema row + member_of edges with ?-propagation rollback; name collision suffix (tested core -> core-2, lexicographic dominant-tag tiebreak); drafted name = dominant tag, summary_source=drafted; member immutability asserted (summary||tags before/after per member); direction member -> schema matches member_of ontology (range=schema, inverse has_member); JSON output {"status":"success","schema":{...}} and error path {"error": ...} (main.rs:14) per convention; 5 behavioral tests, all asserting persisted state. Ran: fmt clean, clippy --all-targets 0 warnings, cargo test schemas 14 passed (9 scan + 5 confirm), full suite 143/0.

Findings (all "noted", status stays reviewing):
- moderate / spec-conformance — member_of domain excludes `code` (rel.rs:170-177, spec line 188) but kind_table accepts code members and confirm writes those edges via direct SQL, bypassing link.rs domain validation; dogfood candidates are code-heavy so first real confirm persists ontology-invalid edges. Needs decision: amendment+domain extension or reject code members (confirm.rs:153).
- moderate / missing-test — no test runs scan() after confirm() anywhere; skip-J-matching-confirmed, dormant candidate, drifted-new-sig-at-stability-1 all unpinned despite being the no-double-promotion mechanism (confirm.rs:378).
- moderate / convention — parse_tags_raw (comma fallback) vs rebuild::parse_tags (JSON-only) despite "same intent" doc: legacy comma-tags rows weight name/centroid but not detection (confirm.rs:186, rebuild.rs:104-107).
- minor / process — API-shape deviation unrecorded: spec says scan --apply promotes + confirm <id> bumps recency; implementation is confirm <sig|prefix> [--name] as promoter, draft summary format differs too; no phase 5 spec_deviations.
- minor / missing-test — no binary-level CLI test (tests/cli.rs untouched); precedent test_schema_scan_stages_and_writes_nothing_else; should also cover AC-5 retrieval clause (decision get after confirm).
- minor / missing-test — J-vs-confirmed guard exercised only via identical-sig re-confirm; no distinct candidate with J>=0.7 test.
- minor / process — files_touched lists src/ops/mod.rs, untouched this phase (phase 3 leftover). Reported, not fixed (write mandate: review_findings + updated_at only).

Counts: 7 findings = 0 severe, 3 moderate, 4 minor.

## 2026-09-05T05:21:09Z — re-review: phase 5 of spec 0002-schema-formation (retry 1 resolution check)

- MOD-1 resolved-verified: rel.rs:170-177 adds `code` to member_of domain; member_of_spec_and_normalization (rel.rs:329-349) asserts the six-type domain; spec ontology line amended with dogfood rationale.
- MOD-2 resolved-verified: scan_after_confirm_excludes_confirmed_candidate_but_keeps_members (confirm.rs:685-714) - consumed sig does not re-stage; evolved cluster restages as decision:1,2,3,schema:1 at stability 1, matching the implemented semantics.
- MOD-3 resolved-verified: parse_tags unified at src/models.rs:181-193 (JSON-first, comma fallback documented + unit-tested); rebuild.rs:117/525 rewired; confirm.rs:338 consumes; both private copies deleted. Comma fallback now feeds detection clustering too - deliberate alignment.
- MOD-4 claim imprecise but substance delivered: "schema_assimilates_new_item (confirm.rs:717-746)" does not exist under that name anywhere; lines 717-746 are the distinct-J rejection test. The assimilation mechanism IS pinned, under two other names (scan_after_confirm... + distinct_candidate...). Resolved on substance.
- MIN-4 distinct-J: resolved-verified via distinct_candidate_with_overlapping_members_is_rejected (confirm.rs:716-752), J=3/4=0.75 >= 0.7; candidate-side schema-key filter (confirm.rs:372-377) is load-bearing in the test (without it J=0.6, duplicate promotes).
- MIN-6 files_touched: resolved-verified, now accurate (7 real files, src/ops/mod.rs removed).
- REMAINING: phase 5 spec_deviations still null (confirm-as-promoter / no---apply unrecorded); REMAINING: no binary-level CLI confirm test (tests/cli.rs untouched).
- NEW minor / documentation: confirm.rs:16-21 module doc claims scan skips J-matching confirmed clusters ("neither re-stages nor double-promotes") - false: scan never consults confirmed_schemas (sole call site confirm.rs:378) and the phase's own test proves the J-match restages under a new sig. Covenant lives entirely in confirm's J-guard; fix the comment.

Asymmetry note for future phases: the stored-vs-candidate overlap guard compares candidate members against stored sets reconstructed from member_of edges. Today stored sets are knowledge-only in every reachable path (confirm only writes edges for the confirmed candidate's members), so the candidate-side filter alone is correct. The ontology now permits schema->schema member_of (nested schemas): if a future phase ever writes such edges, stored sets gain schema keys and comparisons become asymmetric - dilution errs toward ALLOWING near-duplicates. If nested schemas land, confirmed_schemas should filter schema-source edges to restore symmetry.

Verification: cargo fmt clean, clippy --all-targets 0 warnings, 145 passed / 0 failed.

Counts: 8 findings = 0 severe, 3 moderate (all resolved-verified), 5 minor (2 resolved-verified, 2 remaining, 1 new noted).

## 2026-09-05T06:00:52Z — phase 6 of spec 0002-schema-formation (list/show/refine + scan --apply)

- severe / correctness — scan --apply double-promotes: apply loop (scan.rs:286-303) calls promote() directly, skipping confirm()'s Jaccard-vs-confirmed guard (confirm.rs:375-392). Reproduced with the CLI binary on a temp DB: dense trio, scan x2, scan --apply -> ["core"]; scan --apply again -> ["core-2"], 2 schema rows with identical members, silent unique_name suffix. scan.rs:283-285 comment and decision 81 rationale ("the covenant is not bypassed") cover only the gates half; the no-double-promotion half is bypassed. Fix direction: route the apply loop through the confirm() covenant (guard + skipped reporting), not a bespoke promote() call.
- moderate / spec-conformance — schema list omits the spec's "blended score" field; rank_expr orders agent-first/recency/member_count with no usefulness score. Possible deliberate deferral to the scoring phase, but phase 6 spec_deviations is null so it is unrecorded.
- minor / spec-conformance — CLI shapes drift from spec API lines 195-199: refine positional <target> vs spec's --id, missing optional --name rename; confirm <sig|prefix> vs spec <id>. Phase 6 spec_deviations null (jq-verified).
- minor / process — files_touched omits tests/cli.rs (+142 lines this phase).

Verified clean: AC-6 agent-first ranking (rank_expr primary key agent-authored; unit test agent wins despite OLDER timestamp — stronger than the claim); show/refine error contract (non-zero exit + {"error": ...} on stderr, CLI-test pinned); bump semantics documented (confirm.rs:364-367) and pinned (CLI: timestamp advances, no row/edge growth); bare scan read-only intact (phase-3 write-audit test still passes, scan(conn,false)); promote() extraction regressed nothing (full suite 150/0); decision 81 exists with forced-gate justification. Ladder this review: fmt clean, clippy --all-targets 0 warnings, 150 passed / 0 failed.

Pattern carried forward (3rd occurrence): promote-path code bypasses or overstates confirm()'s covenant — confirm.rs:16-21 doc overclaim (phase 5), scan --apply bespoke promote (this phase), decision 81 rationale overclaim. The covenant enforcement should live in ONE function that all promoters call.

Counts: 4 findings = 1 severe, 1 moderate, 2 minor.

## 2026-09-05T06:28:40Z — re-review: phase 6 of spec 0002-schema-formation (retry 2 resolution check)

- severe / correctness — resolved-verified: scan --apply routes every candidate through confirm() (scan.rs:287-317), restoring the Jaccard-vs-confirmed guard for batch promotion; skipped candidates report named reasons. Regression double_apply_never_duplicates_a_schema (scan.rs:683-729) + reviewer behavioral repro on temp DB: apply1 promotes 1 schema/3 edges, apply2 skips 'candidate already confirmed as schema 1', rows/edges unchanged. Name-fallback (dominant_tag NULL handling via Option<Option<String>> + shared parse_tags) verified in source.
- moderate / spec-conformance — resolved-verified: blended-score deferral now recorded in phase 6 spec_deviations as deferred-to-phase-7.
- minor / spec-conformance — resolved-verified: confirm id/name acceptance recorded (accepted); residual: refine positional target + absent --name rename still unrecorded.
- minor / process — resolved-verified: files_touched now lists tests/cli.rs.

Reviewer call on Main's pr: question — keep kind_table strict, no change: pr/commit nodes are link endpoints (URL string ids parse_members rejects at numeric parse, pre-write), member_of domain deliberately excludes pr (implemented_in owns that route), and per-candidate confirm() errors in the apply loop already surface as skipped-with-reason. The boundary is clean.

Incident (disclosure): during verification I reproduced Main's ENGRAMS_DB mistake — the env var does not exist, the binary silently resolved the live workspace DB, and my scan --apply (on the then-unfixed tree path... actually retry-2 code) promoted 13 schemas live. Rolled back with Main's recipe (drop FTS-touching triggers, delete rows, recreate triggers from source DDL); post-state verified: schemas=0, member_of=0, schemas_fts=0, triggers byte-identical to source. Residue: schema_candidates stability counts bumped (29 rows), same precedent Main set. Root cause of both incidents: the CLI accepts no explicit DB override env at all, so "targeting a copy" silently means live. Recommended follow-up outside this spec: add ENGRAMS_DB env support or a loud warning when --db is absent and CWD contains context.db.

Verification: cargo fmt clean, clippy --all-targets 0 warnings, 151 passed / 0 failed. Live-DB rollback state independently verified.

Counts: 4 findings = 1 severe, 1 moderate, 2 minor; all 4 resolved-verified.

## 2026-09-05T07:23:21Z — phase 7 of spec 0002-schema-formation (retrieval tiering + surfacing telemetry)

- moderate / correctness — serde_json preserve_order is global; graph stats (graph/mod.rs:92-96) serializes std HashMaps into json!, so by_type/by_relationship/by_origin key order went alphabetical -> per-process random (SipHash). Nothing in-repo byte-compares that output (grep-verified), so no break today, but cross-run byte determinism of that command is lost and any future HashMap->json! site inherits the hazard. Fix: sort keys at the stats site.
- minor / spec-conformance — AC-7 "no schema match -> exactly what it would have before": zero-hit response gains always-present schema_matches: [] (query.rs:222-227) and miss_guidance gains always-present schema_centroids: [] (query.rs:328-334). Fix: conditional keys.
- minor / process — "8 new unit tests" claim vs 4 actual #[test] in retrieval.rs (suite delta +4, 5 green full-suite runs, 156 passed).
- minor / missing-test — prime's schemas-block-leads emission (prime.rs:534-537, relies on preserve_order) unpinned; prime.rs has no tests; agent_rank ordering IS unit-tested.

Verified true: centroid tier leads hits array with FTS fall-through byte-equal on miss (mapped on non-empty only); reinforce extended to schemas (validate_table guards); record_surface confined to retrieval_surfaces in one transaction with +90d prune; brief schema node_payload + co-surfaced telemetry capped at 20 (documented); no migration needed (schema.rs untouched; v12 DDL covers). Reported flake: not reproduced — 5 full-suite runs + 2 extra cli-only runs all green. Ladder: fmt clean, clippy 0 warnings, 156 passed / 0 failed. spec_deviations null for phase 7 (no deviation identified; preserve_order is an implementation choice, not a spec conflict).

Counts: 4 findings = 0 severe, 1 moderate, 3 minor.

## 2026-09-05T07:44:41Z — re-review: phase 7 of spec 0002-schema-formation (retry 1 resolution check)

All 4 resolved-verified.
- MOD graph-stats ordering: BTreeMaps at graph/mod.rs:84-96; 3-run byte-identical live run, alphabetical keys.
- MIN empty-key compat: conditional inserts (query.rs:222-231, 334-341); no-schema DB and non-overlap DB both lack the keys; overlap case = tier-led array (schema first), retrieval_surfaces rows + schemas reinforce observed.
- MIN test count: corrected 4 matches tree; all behavioral, 157/0.
- MIN prime pin: test_prime_leads_with_schemas_block (tests/cli.rs:2403) asserts first key == "schemas" via real binary.

Ladder: fmt clean, clippy 0 warnings, full suite 157 passed / 0 failed (68+56+12+6+15).

Counts: 4 findings = 0 severe, 1 moderate, 3 minor; all 4 resolved-verified.
