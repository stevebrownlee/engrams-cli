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

## 2026-09-05T08:43:31Z — phase 8 of spec 0002-schema-formation (assimilation hooks)

- MOD missing-test — new subset-bail branch (confirm.rs:384-394, covered == knowledge_members.len()) untested; existing tests only hit the J>=0.7 branch. Reviewer proved the branch end-to-end via real binary: staged {d1,d3} vs confirmed {d1,d2,d3} (J=2/3=0.667, covered=2/2) -> "candidate already confirmed as schema 1".
- MIN correctness — subset-bail is permanent: confirmed_schemas (confirm.rs:75-105) has no status filter and no retire/archive command exists; freeing territory is impossible by design accident. Guard should filter to active/needs_review or document permanence.
- MIN correctness — strict superset of small schema escapes both guards (containment check one-directional; J-dilution under 0.7). Behavioral repro: core-2 {d4,d5} then infra {d4,d5,d7} both promoted (J=2/3). Fix direction: mirror containment on the confirmed side's non-schema members; leave drifted shapes to the 0.7 rule.
- MIN process — claim "+7 unit tests", tree adds 6 (assimilate.rs only; bin 68->74, suite 157->163). Third count-type claim discrepancy this spec.

Verified clean: token overlap 0.4 / top-2 match decision 78; decline-only-via---schema-none matches 79; suggestions persist without membership (B1a: fired fit=1.0, edges unchanged); batch is_autocommit joins outer tx; pattern.rs S7 write-through restored; fit=0.0 on explicit attach without a fired suggestion (expected: no suggestion row existed). All behavioral probes on temp DBs (--db flag, never live).

Pre-existing observation (not a phase 8 finding, reported to Main): try_resolve_candidate (confirm.rs:114-140) is prefix-LIKE only despite its doc comment claiming "exact signature or unique prefix"; an exact match that is also another candidate's prefix errors as ambiguous. Lives in phase 6's extraction.

Ladder: fmt clean, clippy --all-targets 0 warnings, full suite 163 passed / 0 failed (74+56+12+6+15).

Incident note: an earlier review step in this session ran the binary without --db (and ENGRAMS_DB was created by commit 57a6d0b that day), promoting 13 schemas into the live workspace DB; rollback executed and verified (schemas/member_of/FTS=0, triggers byte-identical to source DDL, schema_candidates staging mutations left in place). Root cause both times: binary resolves the workspace DB when no explicit path is given; never run scan --apply without --db.

Counts: 4 findings = 0 severe, 1 moderate, 3 minor.

## 2026-09-05T09:02:28Z — re-review: phase 8 of spec 0002-schema-formation (retry 1 resolution check)

All 4 findings resolved-verified:
- MOD subset-bail: strict_subset_of_confirmed_bails_already_confirmed (confirm.rs:883-907) — stages {1,2,3}, confirms, stages {1,2} subset (J=1/3), asserts bail + 1 schema row. stage() helper pre-passes gates so only the covenant runs.
- MIN permanence: bail site documents it (confirm.rs:408-410) — v0.14 adaptation owns the retire path (decision 76).
- MIN superset: guard rewritten three-way (confirm.rs:416-433) — containment, mirrored full-coverage (schema-keys excluded both sides, per my recommendation), J>=0.7. Superset pinned (confirm.rs:910-936, cites my repro). Binary repro both orientations: superset {1,2,3} over {1,2} bails naming schema 1; legit overlap {1,3,4} over {1,2} (covered 1/3, covers 1/2, J=1/4) promotes as second schema.
- MIN count: suite now 166/0 as claimed (reviewer-run).

NEW minor noted: guard's safe direction unpinned — no test asserts overlap-without-containment still promotes (the over-fire risk of the new mirrored clause). Binary-repro-verified correct today; needs its own pin.

Side note closed: try_resolve_candidate now exact-query-first (confirm.rs:121-136), doc updated, exact_signature_wins_over_ambiguous_prefix passes — my phase-8 prefix-LIKE observation resolved.

Counts: 5 findings = 0 severe, 1 moderate, 4 minor; 4 resolved-verified, 1 new noted. Ladder: 3 new guard/resolver tests pass, full suite 166/0.

## 2026-09-05T10:05:00Z — phase 9 of spec 0002-schema-formation (export/import round-trip)

- minor / correctness — member_of provenance lost on round-trip: confirm edges are origin='manual' + source='schema_confirm', but no export SELECT or import INSERT carries context_links.source, so post-import source=NULL (verified: pre-export markers schema_confirm x3 + one manual NULL; post-import all NULL). Compounding it, the export.rs comment misstates the dedup design ("links export excludes confirm edges by design" is false — origin='manual' passes the filter; the members-array NOT EXISTS guard is the dedup and is load-bearing). No reader of source exists today, hence minor.
- minor / correctness — retrieval_surfaces import not idempotent: plain INSERT doubles rows on re-import (4→8 verified), unlike the rest of the importer (OR REPLACE + explicit ids + members guard). Natural-key WHERE NOT EXISTS is the fix shape.

Verified clean (no findings): s14 passes and asserts member_count==3 not 6, declined suggestion, FTS MATCH, list/show on target, post-import scan no-duplicate; both-exports overlap dedup proven with a hand-made manual member_of edge (4 edges after double import, not 8); populated-target id clash stays inside the established INSERT OR REPLACE convention and FTS external-content stayed consistent (zombie purged, core indexed, uuid preserved); schema_candidates exclusion matches spec line 131 and is re-staged soundly.

Ladder: fmt clean, clippy --all-targets 0 warnings, full suite 168 passed / 0 failed (78+56+13+6+15).

Counts: 2 findings = 0 severe, 0 moderate, 2 minor.

## 2026-09-05T10:01:54Z — re-review: phase 9 of spec 0002-schema-formation (retry 1 resolution check)

Both findings resolved-verified. Provenance: links files + members array now carry source, import writes it back; repro confirms 3/3 member_of edges on target with source='schema_confirm'. Comment corrected to name the real dedup mechanism. Idempotency: natural-key NOT EXISTS with null-safe `arg IS ?`; my original 4-to-8 repro now stays 5-to-5 (including a NULL-arg row I added to exercise the null path). s14 pins idempotency + dedup counts; residual: no assertion yet pins the source column on the target (noted in resolution). Ladder: fmt clean, clippy 0, 168/0. PASS — 0 remaining.

## 2026-09-15T14:05:46Z — phase 1 of spec 0004-schema-kind-surfacing (v14 migration)

- minor / verification — recorded Gate 3 command `cargo test --test cli migrate` never runs the phase's new test_migration_v13_to_v14: 'migrate' is not a substring of 'migration' (verified by running: only test_migrate_command runs, 1 passed / 56 filtered out); the CLI test passes when invoked by exact name, and the bin-side gate `cargo test --bin engrams upgrade` covers the unit v13-to-v14 test, so real coverage exists but the recorded CLI gate is green without exercising the phase's headline CLI addition. Same genre as spec-0002 phase 2's vacuous 'cargo test schemas' gate (specs/0004-schema-kind-surfacing.progress.json:32)

Verified by execution: `cargo test --bin engrams schemas` 51/51 pass (incl. upgrade_v13_to_v14_adds_kind_columns_to_schemas); exact-name run of test_migration_v13_to_v14 passes. Frozen-migration invariant holds: MIGRATION_V14 is ALTER-only additive, MIGRATION_V13 untouched, baseline SCHEMA carries the final shape, LATEST_VERSION=14 with arm 14. The unit test's hand-built v13 DDL matches MIGRATION_V12's CREATE exactly (columns, types, defaults); the CLI rewind trick is faithful because V14 is purely additive — DROP COLUMN restores the exact v13 column order — and the version pin is derived from migrate's self-reported JSON, matching the spec-0002 self-maintaining convention. Projection pattern id:11 checked: not violated — its regex targets hand-typed decisions-table projections (DECISION_COLS); schema_candidates/schemas have no shared projection constant and the diff extends the pre-existing positional lists consistently.

Counts: 1 finding = 0 severe, 0 moderate, 1 minor.

## 2026-09-15T14:05:46Z — phase 2 of spec 0004-schema-kind-surfacing (confirm copies kind)

- moderate / spec-conformance — spec 0004's API surface announces "schema confirm success payload: the schema object gains kind", but promote()'s schema JSON still omits kind, and no remaining phase owns confirm output (phase 3 covers prime/list/show, phase 4 export/import), so the announced contract item is currently orphaned. AC-1 itself (stored row copies kind and reasons as-is) is met and tested. Same genre as spec-0002 phase 6's missing blended-score field (src/ops/schemas/confirm.rs:559)

Verified by execution: `cargo test --bin engrams confirm` 14/14 pass including both new tests. AC-1 mechanics checked line-by-line: kind/kind_reasons_json flow from the sole resolution path into INSERT ?6/?7 in matching column order; unlabeled candidates copy the candidate DDL defaults ('unclear'/'[]') verbatim — proven by confirm_of_unlabeled_candidate_defaults_to_unclear, and the label copy (no remap) by confirm_copies_candidate_kind_and_reasons_into_schema_row with a 2-element reasons array. Diff contains no ordering/gate/territory/edge/draft changes. bump_existing_schema correctly not extended: it fires only when try_resolve_candidate returned None, so no candidate kind exists in that path, and the spec's after-confirm kind correction is explicitly a future spec (open question unresolved); a re-matching candidate bails via the covenant instead.

Counts: 1 finding = 0 severe, 1 moderate, 0 minor.

## 2026-09-15T15:04:03Z — phase 3 of spec 0004-schema-kind-surfacing (read-path kind surfacing)

- minor / verification — recorded Gate 3 command `cargo test --test cli schemas` runs only test_prime_leads_with_schemas_block (verified: 1 passed, 57 filtered out); the phase's new test_schema_confirm_payload_carries_kind and the kind-extended test_schema_list_show_refine_and_confirm_bump match no recorded filter and run in zero recorded gates. Both pass by exact name; the bin gate covers the same contract at unit level, so coverage exists but the recorded CLI gate is green while exercising only the prime entry assertion. Second occurrence in this spec of the vacuous-filter genre (phase 1: 'migrate' filter) that this phase was explicitly told to avoid (specs/0004-schema-kind-surfacing.progress.json:118)
- minor / verification — no recorded phase-3 gate targets the memory integration target, leaving s15_prime_list_show_surface_kind — the only end-to-end check of the pre-0004 'unclear'/'[]' as-is invariant across prime, list, and show — unexercised by every recorded gate; even `cargo test --test memory schemas` misses it by name (matches only s14, 13 filtered out). Passes by exact name; the invariant is also unit-covered by the bin gate (specs/0004-schema-kind-surfacing.progress.json:118)

Verified by execution: all five recorded gates run and pass (fmt, build, clippy; bin schemas 54/54; cli schemas 1/1); filters were enumerated via -- --list before running — the bin filter genuinely selects the new prime/list/show/confirm unit tests, the cli filter does not select the phase's new/modified CLI tests. Exact-name runs: cli 2/2 pass, memory s15 1/1 passes. All four contract surfaces verified at emit sites and by tests: prime_block SELECTs s.kind from schemas only and prime.rs inserts the block verbatim (telemetry reads only id — no key-stripping dispatch); list row_json gains kind beside status; show gains kind plus reasons parsed from kind_reasons_json with unwrap_or(json!([])) ('[]' renders as an empty array); confirm promote echoes cand.kind and bump selects the stored kind — the spec-line-82 reviewer-binding item, resolving phase 2's moderate finding. Invariants hold: schemas-table-only projection, no N+1 (single SELECTs, reasons parsed in-row), 'unclear'/'[]' surface as-is (s15 plus bin unit tests). Projection pattern id:11 checked: not violated — DECISION_COLS covers decisions only; schema projections were hand-typed pre-diff and are extended consistently; `engrams check` reports zero violations on all five touched files. Out-of-scope surfaces (brief/query composites, ordering, candidate surfacing) untouched.

Counts: 2 findings = 0 severe, 0 moderate, 2 minor.

## 2026-09-15T15:36:36Z — phase 4 of spec 0004-schema-kind-surfacing (export/import round-trip)

- moderate / verification — recorded Gate 3 command `cargo fmt --check` fails on the committed phase-4 tree (exit 1, verified by running; working tree byte-identical to ecb6f42 for all four touched files): four phase-4 hunks are not rustfmt-clean — the kind/kind_reasons_json unwrap_or chain at src/ops/transfer/import.rs:387 and assertions at tests/cli.rs:3631, tests/memory.rs:1174 and tests/memory.rs:1183. The phase was committed in a state its own recorded verification rejects; `cargo fmt` rewrites the four hunks with no behavioral change. Unlike this spec's prior gate findings (green-but-vacuous), this recorded gate fails outright on the committed code (src/ops/transfer/import.rs:387)
- minor / verification — recorded Gate 3 command `cargo test --test memory schemas` runs only s14_export_import_preserves_schemas_with_identity_and_telemetry (verified: 1 passed, 14 filtered out); 'schemas' is not a substring of s16_export_import_round_trips_schema_kind (tests/memory.rs:1067), so the labeled-row round-trip identity at the center of AC-6 runs in no recorded phase-4 gate. Unlike phases 1/3 there is no bin-gate backstop (no src unit tests, no --bin gate); Gate 5's full `cargo test` is the only recorded execution. The sibling CLI gate is genuinely effective (verified: 5 passed, both new CLI tests included); s16 passes by exact name. Third vacuous-filter occurrence in this spec (phase 1 'migrate', phase 3 'schemas') (specs/0004-schema-kind-surfacing.progress.json:172)

Verified by execution: recorded gates run — fmt FAILS (finding above), build and clippy clean; `cargo test --test memory schemas` 1/1 (s14 only); `cargo test --test cli export` 5/5 including both new CLI tests. Filters enumerated via -- --list before running: cli 'export' genuinely selects both new tests plus three pre-existing export/import tests; memory 'schemas' does not select s16. Exact-name runs: s16 1/1 passes, both CLI tests 1/1 pass. Substance verified at emit/consume sites: export SELECT reads kind + kind_reasons_json at indices 14/15 and json! emits both keys verbatim for every schemas row; import's 16-column INSERT OR REPLACE writes both verbatim with unwrap_or('unclear')/unwrap_or('[]') fallbacks matching the house per-column default pattern and the MIGRATION_V14/SCHEMA column defaults exactly; grep confirms no labeler or inference call anywhere in import.rs (no re-derivation); process_dir reads only .md files, so retrieval_surfaces.json in the schemas dir is never parsed as a schema row (s16's comment and filter are accurate). Pattern 11 not violated — no shared projection exists for schemas; DECISION_COLS covers decisions only and remains in use. s14 and test_export_import both pass in the recorded gates, so the pre-existing export/import contract is unchanged. Frozen migrations untouched; no allow(dead_code) added. AC-6 fully covered: labeled round-trip identity (s16), legacy 'unclear'/'[]' as-is (s16 + fallback test), older exports lacking the fields (test_import_schema_kind_falls_back_for_older_exports).

Counts: 2 findings = 0 severe, 1 moderate, 1 minor.

## 2026-10-03T21:42:08Z — phase 1 of spec 0005-schema-guided-retrieval (schema show member enrichment + 20-cap)

- minor / spec-conformance — custom_data member payload emits `key` + full parsed `value` where spec Architecture item 1 enumerates `id`, `schema`, `value_preview` for that kind; custom_data has no `schema` column (schema.rs:57-61), so the spec list is unsatisfiable as written and the substitution (brief's key/value shape; full value, not a bounded preview) is unrecorded anywhere (src/ops/schemas/list.rs:314)

Verified clean (no findings): all binding Gate-1 clarifications honored — F-5 stored kind `code` (not `code_node`) with code_nodes.kind deliberately unprojected to avoid the member discriminator collision; F-7 member_count stays the true total and total_members appears only when the cap bites (both pinned at 25-vs-20); F-8 nested `schema` members enriched (name + summary); F-9 rules conformance — 002 borrowed Vec<&i64> + params_from_iter (no boxing), 004 fold moves out of the dying detail maps via get_mut+remove (comment cites the rule), 005 Vec/HashMap with_capacity + sql_placeholders (whose doc supersedes the collect-join idiom), 006 enrichable_kind returns &'static str with no .to_string() on statics. Invariants: frozen migrations untouched (no DDL), JSON-only output preserved, enrich_members is pub(crate) and live (phase-2 brief seam, no allow(dead_code)). AC-1 fully covered by the diff's two CLI tests: per-kind details for all six enrichable kinds, pass-through pr, dangling decision:9999 stays bare with empty anchors, truncation pins member_count 25 / truncated true / total_members 25 / exactly 20 shown / per-id detail mapping (guards against id scrambling). Cross-boundary consumers of the members shape all compatible: cli.rs:2420 and list.rs:502-503 assert length only, memory.rs:876 repinned to member objects (accepted deviation per spec API example), export.rs:540 builds its own array. Not re-run per protocol: Gate 3 verification (fmt/clippy/bin+cli schema tests) already green. Review was static against commit c4bf4ba.

Counts: 1 finding = 0 severe, 0 moderate, 1 minor.

## 2026-10-03T22:19:16Z — phase 2 of spec 0005-schema-guided-retrieval (brief schema member enrichment; machinery moved list.rs -> retrieval.rs)

- moderate / acceptance-gap — AC-2 "the response includes the schema summary and kind" is met only via the envelope's kind:"schema" node marker plus payload summary; node_payload omits the schema's v14 classification kind that spec 0004 surfaced on list/show/prime/confirm (show selects it at list.rs:190), so if AC-2's kind means the classification, brief is the one read path not carrying it and no test disambiguates the reading (src/ops/schemas/retrieval.rs:234)
- moderate / missing-test — resolve_schema's doc comment promises exact-name-never-shadowed-by-slug and first-fold-match-wins on colliding folds, but no test seeds a slug collision or exact-vs-slug-shadow case; a reorder of the id/name/slug arms or a slug_fold change passes silently (src/ops/schemas/list.rs:128)
- minor / spec-conformance — resolve_schema is shared with schema show (list.rs:186) and refine (list.rs:234), so the slug fallback quietly widens both beyond the spec API line "schema ID or exact name"; no spec_deviations entry records the extension (src/ops/schemas/list.rs:130)
- minor / convention-drift — slug scan collects the full Vec<(i64, String)> only to iterate once with early return; rule 005 favors looping directly off query_map (src/ops/schemas/list.rs:156)

Verified clean (no findings): the enrichment move is verbatim — enrichable_kind, enrich_members, fetch_details, MEMBER_ENRICHMENT_CAP bodies byte-identical between the list.rs deletion and retrieval.rs addition; show's member SQL and cap/truncation logic unchanged (member_refs is the phase-1 inline query, same text ORDER BY), so schema show behavior is preserved and phase-1 tests untouched. Cross-boundary confirmed: brief.rs:342 dispatches kind "schema" to retrieval::node_payload, node_exists has the schema arm (brief.rs:176), and record_surface consumes the new resolve output. F-5 stored kind `code` preserved with code_nodes.kind deliberately unprojected; F-7 member_count stays the true total with total_members only when the cap bites (pinned 25-vs-20); F-4 binding — brief caps at 20 with truncated flag (pinned); bare-number targets stay FTS/decision-first (pinned by test_brief_schema_unknown_target_errors). members_by_type removal is a true clean cutover: zero consumers in src/, tests/, docs/ (grep), and no docs assert the old grouped shape. Slug fold is exact-name-first, id-first, slug-last with documented first-fold-wins; ASCII-only fold matches the doc comment's intent. Rules: 002 borrowed params (Vec<&i64> + params_from_iter), 004 get_mut+remove out of the dying detail maps, 006 enrichable_kind returns &'static str — all honored in the moved code. No DDL, JSON-only output, no new allow(dead_code). Recorded gates genuinely select the new tests: bin filter "schemas" matches via the ops::schemas module path (incl. composite_helpers_return_members_anchors_and_neighborhood), CLI filter "schema" matches all four new tests by name. started_at is implementer-disclosed [INFERENCE]; treated as metadata hygiene, not a finding.

Counts: 4 findings = 0 severe, 2 moderate, 2 minor.

## 2026-10-03 — phase 3 of spec 0005-schema-guided-retrieval

- moderate / missing-test — no-flags arm of refine_text ("nothing to refine")
  is the only new error branch without a unit or CLI test; the implementer's
  no-flag-bail judgment call is unpinned while all sibling arms are pinned
  (src/ops/schemas/list.rs:273)
- minor / spec-conformance — starts_with byte-prefix match admits openings
  beyond the four enumerated phrases ("Whenever...", "Whence..." pass via
  "When"); accepted vocabulary is a superset of spec 0005:97's list,
  proven live (src/ops/schemas/list.rs:247)
- minor / unstated-assumption — synthesized path never checks non-empty
  --trigger/--rule, so empty condition stores the degenerate summary
  "Fires when : run tests"; codebase convention bails on empty user text
  at command boundaries (custom.rs:81, decision.rs:290)
  (src/ops/schemas/list.rs:255)

## 2026-10-03 — phase 4 of spec 0005-schema-guided-retrieval

- moderate / missing-test — advise --staged empty-stage early return ({"schemas": [], ...}) was extended by this phase but is unpinned: no test exercises the staged path, so the three-key shape and schemas-first key order on that branch can drift silently; git-fixture precedent exists (tests/cli.rs:3091) (src/ops/advise.rs:34)
- minor / verification — recorded gates never execute tests/memory.rs (verified: --test cli advise matches 2 tests / 70 filtered; --bin gate is bin-only), so the s14 summary repair runs in no recorded gate; Gate 5 full cargo test is the sole backstop. Fourth occurrence of the green-without-exercising genre (0002 p2; 0004 p1/p3/p4) (specs/0005-schema-guided-retrieval.progress.json:199)
- minor / documentation — docs/memory/engrams.md:194 documents advise's return contract as {"constraints", "violations"}; the new schemas key leaves the canonical reference stale and no remaining phase owns that line (phase 6 is scoped to the new cognitive-schemas page) (docs/memory/engrams.md:194)

Verified clean (no findings): AC-5 both Then clauses pinned with load-bearing negatives — active+anchored surfaces, active-anchored-elsewhere and proposed-status both excluded (exclusion shape directly subsumes the dormant/needs_review statuses absent from any status filter), serialized schemas-before-constraints order asserted in both tests (preserve_order confirmed in Cargo.toml:15). Rule compliance: 002 borrowed Vec<&dyn ToSql> + params_from_iter + with_capacity mirroring the rule's own reference implementation (query_relevant_ids); 004 N/A (no dying maps); 005 conditions vec is pre-sized and matches the reference shape — sql_placeholders supersedes only the "?" IN-list idiom, not composite OR conditions. Vocabulary audit: item_anchors stores RefItemType::as_str() underscore forms identical to context_links.source_item_type member vocabulary (kind.rs:197) — no silent kind mismatch. TEXT<->INTEGER affinity join (l.target_item_id = s.id) matches in-file precedent (neighborhood :533) and confirm.rs:88; CAST(? AS TEXT) matches member_anchors :518. Entry shape {id,name,summary,kind} matches spec API item 3 exactly, key order included. Cross-boundary: sole in-repo consumer (.omp/engrams-advisor runAdvise) reads constraints/violations defensively — additive schemas key is backward-compatible; the extension not surfacing schemas is a spec-scope observation for a future phase/spec, not a phase defect. --staged early-return shape consistent with main path (same keys, schemas first). s14 repair statically proven justified: TRIGGER_PREFIXES starts_with (list.rs:234) provably rejects the old "Gateway architecture: ..." summary post-phase-3; repair confined to summary string + assertion, export/import contract untouched.

Ran: cargo test --test cli advise (2 passed) and cargo test --test memory s14 (1 passed) at HEAD.

Counts: 3 findings = 0 severe, 1 moderate, 2 minor. PASS — continue.

## 2026-10-04T00:08:42Z — phase 5 of spec 0005-schema-guided-retrieval (prime block context ranking)

- moderate / missing-test — path_match_counts' third resolution arm (target path nested under an anchor directory, `? LIKE a.path || '/%'`) unpinned; unit test pins exact + anchor-under-target only, and query_relevant_ids has no arm-level tests to backstop the mirrored SQL (src/ops/schemas/retrieval.rs:196)

Verified clean (no findings): AC-6 all four pins real — path ranking (unit + CLI `--paths`), tag ranking (unit + CLI `--tags`, convention-aware render_pipeline vs render-pipeline, overlap count 2 pinned), summary present on every entry (CLI asserts non-empty across all three invocations), and the no-flag regression is load-bearing (`sums[0].contains("policy") && !contains("gateway")` fails on any order flip; telemetry self-bump preserves the 2-vs-1 reward gap because the assertion runs first). Ranking determinism: total order context desc -> reward desc -> centrality desc -> agent_rank asc -> id asc; with no flags context is 0 everywhere and the chain is byte-identical to the pre-0005 order. Rules: 002 borrowed Vec<&dyn ToSql> + params_from_iter + with_capacity; 004 remove() out of the dying path/tag maps with citing comment; 005 pre-sized conditions Vec + join matches the query_relevant_ids reference shape; 006 N/A (no static vocabulary). SQL mirrors query_relevant_ids arm-for-arm; join shape matches active_for_members/neighborhood precedent; production member_of writers (confirm.rs:551, assimilate.rs:229) store ids as TEXT, consistent with the CASTs. centroid_json is NOT NULL in both DDL copies so the String get cannot hit NULL. Cross-boundary: schema_block inserted verbatim into the payload; prime telemetry (prime.rs:406-409) reads only id, so the new context_matches key passes harmlessly. clean_path is idempotent strip-./; prime.rs cleans once and shares the vec between prime_block and query_relevant_ids. repeat_n (stable since 1.82) introduces no finding: no rust-version declared anywhere, CI pins dtolnay/rust-toolchain@stable, so no MSRV contract exists to break. Gates genuinely select the new tests this time (bin prime_block 4/4, cli prime 5/5 — first spec in memory with zero vacuous filters). fmt --check exit 0 on the committed tree; working tree byte-identical to 7c30ae8 for all three files_touched (only progress.json dirty, orchestrator-owned).

Counts: 1 finding = 0 severe, 1 moderate, 0 minor.

## 2026-10-04T00:27:56Z — phase 6 of spec 0005-schema-guided-retrieval (cognitive schemas guide page)

- moderate / quality-smell — schema show JSON example shows "status": "confirmed",
  but the shipped status vocabulary is active/needs_review/retired; schema confirm
  stores 'active' and only sets last_confirmed_at (src/ops/schemas/confirm.rs:532,598),
  so the guide teaches an output value the CLI never emits
  (docs/src/pages/docs/guide/cognitive-schemas.astro:153)
- minor / quality-smell — retrieval-surface table claims brief schema:<id> returns
  enriched details for "all immediate members", but node_payload caps enrichment at
  20 with truncated/total_members (src/ops/schemas/retrieval.rs:341-360), contradicting
  the cap the same table documents one row above for schema show
  (docs/src/pages/docs/guide/cognitive-schemas.astro:79)

Verified clean (no findings): AC-7 all three Then clauses covered — human-schema
theory section (organizes/activates-conditionally), agent-design section with
authoring principles, and three worked examples (software architecture
error-envelope, research survey-then-decide, planning what-to-build-next) — plus
design bullet 4's LLM-retrieval explanation (retrieval-surface table). Every cited
command/flag exists in src/cli.rs: schema refine --trigger/--rule/--summary
(cli.rs:427-441) with the four accepted openers matching the page verbatim, schema
show (422), schema scan/confirm (402-415), advise positional paths (261-267),
prime --paths/--tags (126-138), brief schema:<id> dispatch (brief.rs:342-344).
Example member shapes match enrich_members: system_pattern name/description/
severity/anchors, decision summary/status/anchors, code path/symbol/line_count,
custom_data key+value (JSON when parseable) matching the research example's claim.
advise "schemas" key serializes above constraints (advise.rs:140-144). Sidebar
Guides-section href /docs/guide/cognitive-schemas resolves to the built route
(docs/dist/docs/guide/cognitive-schemas/index.html present); DocsLayout renders
Sidebar on every docs page; callout tip/warn/note and reveal/lead classes all
exist in docs/src/styles/global.css. Convention match with exemplar
for-agent/retrieving.astro confirmed (lead, callout, reveal, pre/code escapes).
Gate 1 F-6 (guide/ section absent from sidebar) resolved by this phase.

Counts: 2 findings = 0 severe, 1 moderate, 1 minor. PASS — continue.

## 2026-10-21T16:25:00Z — phase 1 of spec 0006-workspace-scoped-git-and-safe-import (Workspace resolver + scoped commit, PR, and graph git calls)

_No findings._

Verified clean (no findings):
- All Phase 1 ACs (`AC-1`, `AC-2`, `AC-4`, `AC-5`, `AC-6` PR-number resolution, `AC-7`) met in code and pinned with behavioral CLI tests in `tests/cli.rs` (including negative assertions: no shell-repo commit recorded, no shell-repo file in `code_nodes`, no `code::upsert_file` enrichment from shell CWD when `root` is `None`, non-zero exit + exact error message on numeric PR resolution without workspace, `"cochange_skipped": "no workspace"` on `graph rebuild`, and `"reason": "no workspace named"` on `graph ingest`).
- `Workspace::head_sha` caches via `OnceLock<Option<String>>` per invocation; global `HEAD_SHA` `LazyLock` removed.
- `workspace_root_from_db` strictly requires `<root>/engrams/context.db` and normalizes relative `engrams/context.db` parent `""` to `"."` so `git -C` never receives an empty path.
- `PrCmd::Remove` propagates the missing-workspace error when given a numeric PR instead of silently falling back to `unwrap_or_else`.
- Rules 002–006 and `engrams check --paths src` (0 violations across 59 files) satisfied; `git.rs` and `decision.rs` also cleaned up pre-existing rule 003/004 sites.
- Recorded Gate 3 verification filters (`workspace_`, `test_pr_provenance`) verified non-vacuous via `-- --list` (6 and 2 tests matched respectively).

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.

## 2026-10-21T16:45:00Z — phase 2 of spec 0006-workspace-scoped-git-and-safe-import (staged checks, coverage, hooks, doctor scoped to Workspace root)

_No findings._

Verified clean (no findings):
- `db::workspace_root()` CWD fallback removed from `src/db.rs`; all remaining callers (`advise.rs`, `anchor.rs`, `brief.rs`, `check.rs`, `coverage.rs`, `doctor.rs`, `install.rs`, `rules/mod.rs`, `rules/export.rs`, `pattern.rs`) now take `ws.root.as_deref()` from `ops::dispatch`.
- All Phase 2 ACs (`AC-3`, `AC-6`, `AC-8`) and `doctor` `skipped_checks` contract met and pinned with behavioral tests in `tests/cli.rs` and `tests/policy.rs`:
  - `AC-3`: `workspace_staged_checks_only_named_workspace_files_ac3` verifies `check --staged` checks only repository A's staged file when invoked from repository B, both when A has a violation and after A is cleaned while B retains a staged violation.
  - `AC-6`: `workspace_refuses_git_only_operations_without_workspace_ac6` and `s10_install_hooks_without_workspace_fails_closed` verify `check --staged`, `advise --staged`, `relevant --staged`, `coverage --diff HEAD`, and `install --harness omp --hooks` fail with the exact missing-workspace error message and leave the shell repository's hooks untouched.
  - `AC-8`: `workspace_installs_hooks_only_into_named_workspace_ac8` and `s10_install_hooks_writes_into_named_workspace_only` verify `install --harness omp --hooks` writes `.git/hooks/pre-commit` and `.omp/extensions/engrams-advisor/index.ts` into the named workspace A and leaves shell repository B untouched.
  - `doctor`: `test_doctor` and `workspace_doctor_skips_stale_decisions_without_workspace` verify `"skipped_checks": ["stale_decisions: no workspace"]` and empty `stale_decisions` when no workspace is known, and absence of `skipped_checks` when a workspace is provided.
- Rules 002–006 satisfied: `git.rs` (`staged_files`, `changed_since`) and `coverage.rs` (`diff_files`) keep `Cow<str>` borrowed through `.lines().map(str::trim).filter(...)` before `.map(str::to_owned)` (rule 003), `changed_since` eliminates the intermediate `args` `Vec` (rule 005), and `doctor.rs` `skipped_checks` holds `&'static str` (rule 006).
- All recorded Gate 3 verification filters (`workspace_`, `test_doctor`, `--test policy`) match the phase's new/updated tests (zero vacuous filters).

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.

## 2026-10-21T17:02:00Z — phase 3 of spec 0006-workspace-scoped-git-and-safe-import (export and restore code references by path)

_No findings._

Verified clean (no findings):
- `AC-9` and API surface item 7 met in code and pinned with `import_restores_code_references_by_path_ac9` in `tests/memory.rs`:
  - `export.rs` builds a `code_nodes` `id -> path` lookup once and attaches `path` to `code` schema members and `code` `retrieval_surfaces` rows, and `source_path` / `target_path` (plus YAML frontmatter keys) to manual `context_links` whose source/target endpoint is `code`.
  - `import.rs` accepts `root: Option<&Path>` (wired from `ws.root.as_deref()` in `src/ops/mod.rs`) and resolves `code` endpoints carrying a non-empty `source_path`, `target_path`, or `path` through `graph::code::upsert_file(&tx, ..., root)`, rewriting the stored ID in `context_links`, schema `member_of` links, and `retrieval_surfaces` while falling back to the raw ID when no path is present (backward-compatible with older exports).
  - `import_restores_code_references_by_path_ac9` seeds dummy `code_nodes` in the fresh target DB so the restored `src/ops` node gets a different `code_nodes.id` (`3 != 31975`), verifies `schema show performance-rules` surfaces the `code` member with `id == new_code_id` and `path == "src/ops"`, and verifies re-importing is idempotent across `member_of` links, `relates_to` target links, and `retrieval_surfaces`.
- Rules 002–006 and `engrams check --paths src/ops/mod.rs,src/ops/transfer/export.rs,src/ops/transfer/import.rs` (0 violations across 3 files) satisfied: `import.rs` keeps non-remapped IDs borrowed via `Cow::Borrowed` and allocates only when `upsert_file` remaps a `code` ID; `export.rs` borrows `&str` from `code_paths` across three consumer loops without `.get().clone()`.
- Recorded Gate 3 verification filters (`import_`, `--test memory s14`) verified non-vacuous via `-- --list` (matched 6 tests including `import_restores_code_references_by_path_ac9`, and `s14_export_import_preserves_schemas_with_identity_and_telemetry`).

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.

## 2026-10-21T17:22:00Z — phase 4 of spec 0006-workspace-scoped-git-and-safe-import (validate import references before write and report skipped_references)

_No findings._

Verified clean (no findings):
- `AC-10`, `AC-11`, and API surface item 8 met in code and pinned with `import_skips_missing_references_preserves_existing_rows_and_scans_ac10_ac11` in `tests/memory.rs`:
  - `kind_table` moved from `src/ops/schemas/confirm.rs` to `src/models.rs` returning `Option<&'static str>` (rule 006) and re-exported in `confirm.rs`.
  - `import.rs` parses all export files up front (sorted deterministically by filename), inserts entity rows (`decisions`, `progress_entries` with `parent_id = NULL`, `system_patterns`, `custom_data`, `schemas`), and queries actual post-insert IDs via `existing_ids_in_table(&tx, ...)` using borrowed `Vec<&dyn rusqlite::ToSql>` + `rusqlite::params_from_iter` and `crate::ops::sql_placeholders` in 900-ID chunks (rules 002, 005).
  - Querying post-insert IDs in the transaction accurately handles `ON CONFLICT(name)` / `ON CONFLICT(category, key)` updates where a colliding pattern/custom_data row keeps its pre-existing ID rather than creating the exported ID (pinned by `patterns/2.md` id 50 colliding with pre-existing id 5 and skipping `anchors/50.md`).
  - Missing `progress_entries.parent_id` imports the entry with `parent_id = NULL` and records `{"file": ..., "reference": "progress_entry:<pid>", "reason": "parent missing; entry imported without its parent link"}`.
  - Reference rows (`context_links`, `item_anchors`, schema `members`, embedded and standalone `schema_suggestions`, and `retrieval_surfaces`) validate all endpoints before write; omitted rows are never inserted, so `INSERT OR REPLACE` never displaces pre-existing rows at the same ID (pinned for `item_anchors` id 41 and `context_links` id 42), and `imported` counts exclude skipped rows.
  - `engrams schema scan` completes and reports candidates on the imported database after skipping dangling references (`AC-11`).
- Rules 002–006 and `./target/debug/engrams check --paths src/models.rs,src/ops/schemas/confirm.rs,src/ops/transfer/import.rs` (0 violations across 3 files) satisfied.
- Recorded Gate 3 verification filters (`import_`, `--bin engrams schemas`) verified non-vacuous via `-- --list` (7 and 74 tests matched respectively, including `import_skips_missing_references_preserves_existing_rows_and_scans_ac10_ac11`).

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.

## 2026-10-22T13:10:00Z — phase 1 of spec 0007-reviewed-schema-proposals (migration v16, test book fixture, and schema propose/reject/proposals)

_No findings._

Verified clean (no findings):
- All `AGENTS.md` architecture invariants satisfied:
  - Frozen migrations: `MIGRATION_V16` is ALTER/CREATE-only, published `MIGRATION_V2`..`MIGRATION_V15` untouched, baseline `SCHEMA` carries byte-identical `statement`/`scope_json` columns on `schemas`, `schema_proposals` table + indexes, and updated `'active' | 'needs_review' | 'retired'` status comment, and `LATEST_VERSION = 16`.
  - `#[allow(dead_code)]` seam attributes in `src/ops/schemas/propose.rs` (`ProposalStatus::Confirmed`, `MatchVia`, `PENDING_IN_PRIME`) each cite the exact future phase in spec 0007 (`Phase 2` / `Phase 3`) that wires the symbol.
  - JSON-only output on all new subcommands (`schema propose`, `schema reject`, `schema proposals`).
- All Phase 1 ACs (`AC-1`, `AC-2`, `AC-4`, `AC-7`) and API surface items 1, 3, and 4 met in code and pinned with behavioral unit tests (`src/db.rs`, `src/ops/schemas/propose.rs`) and CLI integration tests (`tests/cli.rs` over `tests/fixtures/manuscript-book.json`):
  - `AC-1`: `test_proposal_ac1_records_pending_without_surfacing_as_schema` and `proposal_records_pending_with_default_scope_and_no_schema_created` verify pending proposal persistence, default and explicit scope calculation, computed evidence counts, enriched `schema proposals` listing, zero `schemas` rows, zero `member_of` links, and absence from `advise` and `query`.
  - `AC-2`: `test_proposal_ac2_refuses_each_failing_evidence_or_wording_check` and `proposal_validation_rejects_each_failing_check` pin each of the 10 individual failure checks (fewer than 3 notes, 1 file and 1 day, missing note, non-current/superseded note, `code:` cited as note, missing contrary, retired contrary, contrary also in evidence, claim restating a cited note at Jaccard >= 0.8 without tag dilution, and instruction missing a trigger phrase) and verify zero rows written to `schema_proposals`.
  - `AC-4`: `test_proposal_ac4_refuses_pending_rejected_and_confirmed_territory` and `proposal_overlap_and_reject_lifecycle` pin pending overlap (`inserted: false` + `similar`), `schema reject proposal:<id> --reason`, non-pending reject error, rejected overlap with no newer note (`inserted: false` + `rejected_before` with `id`, `decided_at`, `reason`, `overlap`), and active-schema territory refusal (`claimed_territory` even with `--force`, filtering `code:` members from pre-v16 schemas).
  - `AC-7`: `test_proposal_ac7_seeds_from_detector_group_drops_code_and_snapshots_evidence` pins `--from <prefix>` seeding knowledge notes while dropping and reporting `dropped_code_members`, recording `source: {"kind": "candidate", "ref": <full_sig>}`, and keeping `members` and `source.ref` immutable across subsequent candidate mutations and `schema scan` runs.
- Rules 002–006 and `engrams check` (0 violations across 10 touched `src/` files) satisfied: `ProposalStatus::as_str`, `MatchVia::as_str`, and `normalize_note_kind` return `&'static str` (rule 006), `parse_note_key` and `parse_proposal_target` keep slices borrowed (rule 003), `opportunities::suggest_base_schema_name` iterates `query_map` directly without intermediate `Vec` collection (rule 005) and uses `BTreeMap` with deterministic tie-breaking.
- Recorded Gate 3 verification filters (`--bin engrams proposal`, `--test cli proposal`, `--test policy s1_fresh_db`) verified non-vacuous via `-- --list` (4, 4, and 1 tests matched respectively).

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.

## 2026-10-22T13:55:00Z — phase 2 of spec 0007-reviewed-schema-proposals (confirm proposals into schemas and cut over scan and opportunities)

_No findings._

Verified clean (no findings):
- All `AGENTS.md` architecture invariants satisfied:
  - JSON-only output on all modified subcommands (`schema confirm`, `schema scan`, `schema list`, `schema show`, `brief schema:<id>`, `session close`).
  - Phase 1's `#[allow(dead_code)]` seam on `ProposalStatus::Confirmed` in `src/ops/schemas/propose.rs` removed now that `confirm_proposal` wires it; no new `#[allow(dead_code)]` attributes introduced.
  - Explicit `--db` used across all CLI and memory integration tests; no live database or commit-excluded paths touched.
- All Phase 2 ACs (`AC-3`, `AC-8`) and API surface items 2, 6, 7, and 8 (`statement`, `scope`, and `origin` on schema payloads) met in code and pinned with behavioral unit tests (`src/ops/schemas/confirm.rs`, `src/ops/schemas/scan.rs`, `src/ops/schemas/opportunities.rs`, `src/ops/schemas/assimilate.rs`) and CLI/memory integration tests (`tests/cli.rs`, `tests/memory.rs`):
  - `AC-3`: `confirm_proposal` re-validates checks 1–6 (`validate_proposal`) and check 9 (`claimed_territory`) on merged inline edits (preserving the unedited half of `scope` when only `--scope-tags` or `--scope-paths` is supplied), inserts the `schemas` row (`summary_source = 'agent'`, `kind = 'schema'`, `status = 'active'`, `statement`, `scope_json`, `centroid_json`), writes `member_of` links, and marks the proposal `confirmed` with `schema_id` and `decided_at` in one transaction. Collision handling via `unique_name` suffixes (`mara-2`, `mara-broken-leader-2`) and emits `renamed_from`. `schema show` and `brief schema:<id>` join `origin` from `schema_proposals` (`null` for pre-v16 schemas), and `prime`, `advise`, `query`, `list`, `show`, and `brief` include `statement` and `scope`. Pinned by `confirm_proposal_creates_agent_schema_links_members_and_records_origin`, `confirm_proposal_applies_inline_edits_and_preserves_unedited_scope_half`, `confirm_proposal_suffixes_taken_name_and_reports_renamed_from`, and `test_schema_proposal_ac3_confirm_creates_schema_with_origin_edits_and_rename`.
  - `AC-8`: `SchemaCmd::Scan` removes `--apply` and `scan::scan(conn)` returns `{"status": "success", "candidates": [...]}` without creating schemas; `schema confirm <detector-sig>` fails with `"detector groups are evidence, not schemas: run engrams schema propose --from <sig> --statement ... --summary ..."`; `confirm <schema id|name>` preserves re-confirmation bump and rejects proposal-only edit flags; `opportunities::find_opportunities` emits `propose_command` in place of `confirm_command`. Pinned by `scan_stages_all_kinds_and_creates_no_schemas`, `confirm_refuses_detector_candidate_signature_with_propose_command`, `confirm_existing_schema_bumps_and_rejects_edit_flags`, `eligible_unconfirmed_candidate_is_returned`, `test_session_close_opportunity_workflow_actionable`, and `test_schema_proposal_ac8_scan_and_confirm_cutover_and_session_close_propose_command`.
- Rules 002–006 and `./target/debug/engrams check --paths src` (0 violations across 60 files) satisfied; obsolete candidate-promotion helpers (`Member::key`, `dominant_tag`, `kind_histogram`, `draft_summary`, `promote`) cleanly removed from `src/ops/schemas/confirm.rs`.
- Recorded Gate 3 verification filters (`--bin engrams schemas`, `--test cli schema`, `--test memory schema`) match all new and updated unit, CLI, and memory integration tests for Phase 2.

Counts: 0 findings = 0 severe, 0 moderate, 0 minor. PASS — continue.
