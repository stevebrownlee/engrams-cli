# Agent Memory Tier 2 (v0.11.0) — Consolidation, Contradiction Detection, Causal Retrieval

**Status:** Proposed · 2026-08-16 · awaiting ratification
**Decision context:** #53 (tiered roadmap) · #55 (tier-1 shipped as v0.10.0) · improvement keys `t2-consolidate`, `t2-contradiction-detection`, `t2-causal-rels`
**Design decisions (2026-08-16):** consolidate = propose + `--apply` · confidence = read-time decay · contradiction gate = classify + resolve flags · causal surface = `graph why`

---

## 1. Summary

Three features turn tier-1 tracking columns into a self-maintaining memory:

1. **`engrams consolidate`** — promotes repeated, anchor/tag-clustered progress entries into candidate `system_patterns` with **evidence links** (`derived_from` → progress entries) and an initial **confidence** score. Re-running against new evidence **confirms** existing consolidated patterns (`last_confirmed_at` bump). Confidence **decays at read time** — no background jobs.
2. **Contradiction detection at `decision log`** — the existing FTS5 similarity gate now matches **active** decisions only, classifies each hit as a `supersedes` or `conflicts_with` candidate, and new resolution flags (`--supersedes <id>`, `--conflicts-with <id>...`) resolve the conflict in the same command that logged it.
3. **Causal relationships** — `causes`/`caused_by` in the relationship ontology (directed, transitive) and `engrams graph why <node>` for upstream causal-chain retrieval ("why did this happen") with `--down` for impact ("what does this affect").

Grounding in the codebase (verified 2026-08-16): `find_similar` (FTS5, BM25) and the blocking `--force` gate already exist in `src/ops/decision.rs`; `decision merge` and `decision supersede` exist; `RelSpec.transitive` + `transitive_reachable` (cycle-safe BFS) + `graph chain --rel` exist. The `t2-causal-rels` note's "recursive CTE already exists" claim is **wrong** — closure is Rust-side in `graph/model.rs`; no CTE is needed or added.

---

## 2. What's in scope

| # | Item | Notes |
|---|---|---|
| S1 | `causes` RelSpec in `RELS`: directed, transitive, inverse `caused_by`, domain/range any, `same_type: false`, no disjointness | `link add` validation, `normalize`, analytics all work via existing table |
| S2 | `derived_from` RelSpec: directed, inverse `derives`, domain `["system-pattern"]`, range `["progress-entry"]` | Evidence links; manual creation allowed (`--force` overrides constraints as usual) |
| S3 | Schema migration v7: `system_patterns` gains `confidence REAL NOT NULL DEFAULT 1.0`, `last_confirmed_at TEXT` (NULL → treat as creation timestamp) | Additive, transactional via existing `run_migrations` |
| S4 | `pattern log --confidence <0.0-1.0>` and `pattern update --confidence`; `last_confirmed_at` defaults to write time | Manual patterns keep confidence 1.0 semantics |
| S5 | `find_similar` filters to active, non-archived decisions | Superseded/archived decisions no longer trip the gate |
| S6 | Blocked-log response carries per-hit classification: `suggested_relation` = `supersedes` when hit shares ≥1 anchor with the new decision, else `conflicts_with`; plus `shared_anchors` count | Deterministic, explainable heuristic; non-binding suggestion |
| S7 | `decision log --supersedes <id>` — single transaction: insert new decision, mark old `superseded`, write `supersedes` link (reuses `Supersede` handler logic), return both | Resolution implies intent; similarity gate does not re-block |
| S8 | `decision log --conflicts-with <id>...` — insert + symmetric `conflicts_with` link, no status change | Multiple ids allowed |
| S9 | `engrams consolidate` (propose, default): cluster progress entries by shared anchor path and shared tag; clusters with ≥ `--min-repeats` (default 3) entries spanning ≥ `--min-days` (default 2) distinct calendar days become candidate patterns; also surfaces near-duplicate **decision** pairs (same-tag/same-anchor prefilter + `find_similar`) as merge suggestions — propose-only, never auto-merged | Deterministic output; JSON `{"candidates": [...], "merge_suggestions": [...]}` |
| S10 | `engrams consolidate --apply` — inserts candidates as patterns (tag `consolidated`, confidence = `min(1.0, 0.5 + 0.15 × (n − min_repeats))`), writes `derived_from` link per evidence entry; merge suggestions still never applied | No surprise writes; default run mutates nothing |
| S11 | Consolidate **confirmation**: existing consolidated patterns whose anchor/tag signature matches new progress entries (logged after `last_confirmed_at`) get `last_confirmed_at = now` | Evidence-grounded; confidence value itself unchanged |
| S12 | Read-time decay: `effective_confidence = confidence × exp(−LAMBDA × days_since(last_confirmed_at))` reusing v0.10.0 `LAMBDA`; prime/relevant pattern ranking multiplies by it; `pattern list`/`get` expose both stored and effective values | Decisions unaffected; no background decay job |
| S13 | `doctor` advisory: consolidated patterns never confirmed or unconfirmed > 180 days | surfaced alongside existing never_read/archived checks |
| S14 | Docs (`docs/memory/engrams.md`, `AGENTS.md`), version 0.11.0, dogfood: run consolidate + a real causal chain in engrams' own DB | Same proof bar as S11-dogfood in policy-engine spec |

## 3. What's out of scope

- **Embeddings / semantic similarity** — FTS5 + BM25 only, per t2 entry ("no embeddings").
- **LLM-summarized pattern descriptions** — consolidate descriptions are deterministic templates; no model calls from the CLI.
- **Auto-merge of decisions** — merging deletes data; suggestions only.
- **Background decay/cron** — decay is read-time only.
- **Tier 3 memory-quality regression tests** (`tests/memory_quality.rs`) — 0.12.0.
- **Causal inference** — `causes` edges are user/agent-authored; engrams never proposes them in v0.11.0.
- **`causes` in graph derived-edge rebuild** — user links only; `rebuild` untouched.

## 4. Design details

### 4.1 Consolidation clustering (S9)

SQL-native, single pass, no N×N in Rust:

1. **Anchor clusters**: `SELECT anchor path, entry ids FROM item_anchors JOIN progress_entries GROUP BY path HAVING COUNT(DISTINCT entry) >= min_repeats AND COUNT(DISTINCT date(timestamp)) >= min_days` (Done/Dropped entries excluded).
2. **Tag clusters**: same aggregation over `json_each(progress_entries.tags)`.
3. **Union**: tag clusters whose evidence set is fully contained in an anchor cluster are dropped; remaining clusters sorted by (size desc, first evidence id) for deterministic output.
4. **Candidate shape**: `{name, description, tags, anchors, evidence: [progress-entry ids], first_seen, last_seen, initial_confidence}`. Dominant tag/anchor = most frequent in the cluster, ties broken by first evidence id. Name = `consolidated-<slug of dominant tag or anchor path>`; collision → numeric suffix. Description template: `Consolidated from {n} progress entries ({first_seen}..{last_seen}) touching {anchor/tag}; most recent: {latest entry description}`.
5. **Dedupe prefilter**: decision pairs sharing ≥1 tag or anchor → `find_similar` within pair → suggestion `{source, target, shared_terms}`; output capped at 10. Same-pair `decision merge` stays manual (see R2/S10).

### 4.2 Confidence decay (S12)

Reuse `scoring::LAMBDA` (60-day half-life). Pattern score in `prime`/`relevant` becomes `score_expr(...) × confidence × exp(−LAMBDA × days_since(last_confirmed_at))` via a new `confidence_expr(conf_col, confirmed_col)` helper next to `score_expr`. Export/import round-trip **stored** columns only — `effective_confidence` is always derived, keeping round-trips deterministic.

### 4.3 Causal retrieval (S1, S2 surface)

`engrams graph why <type:id>` (also `--node`): upstream walk — follows `caused_by` direction (i.e., `causes` edges into the seed) via existing `transitive_reachable`, returns `{chain: [{node, depth, via_edge_description}], roots: [...]}`. `--down` follows `causes` for impact analysis. `graph chain --rel causes` works automatically once S1 lands; `why` is the ergonomics layer (default rel, upstream direction, edge descriptions).

## 5. Files touched

| File | Change |
|---|---|
| `src/schema.rs` | `MIGRATION_V7` (2 additive columns on `system_patterns`) |
| `src/db.rs` | `LATEST_VERSION` 6 → 7 |
| `src/models.rs` | `Pattern` gains `confidence: f64`, `last_confirmed_at: Option<String>` |
| `src/ops/graph/rel.rs` | `causes`, `derived_from` RelSpecs + table-driven tests |
| `src/ops/graph/mod.rs` | `Why` handler (wraps `transitive_reachable`) |
| `src/cli.rs` | `Consolidate` command; `decision log --supersedes/--conflicts-with`; `pattern log/update --confidence`; `GraphCmd::Why` |
| `src/ops/decision.rs` | `find_similar` active filter; hit classification; resolution-flag transaction paths |
| `src/ops/pattern.rs` | carry new fields through log/update/list/get/parse |
| `src/ops/consolidate.rs` | **New.** clustering, candidates, apply, confirmation |
| `src/ops/scoring.rs` | `confidence_expr`; wire into pattern score expressions |
| `src/ops/prime.rs`, `src/ops/relevant.rs` | pattern ranking picks up confidence factor |
| `src/ops/doctor.rs` | unconfirmed-pattern advisory |
| `src/ops/transfer/export.rs`, `transfer/import.rs` | round-trip new columns |
| `tests/cli.rs` | integration tests per §8 |
| `docs/memory/engrams.md`, `AGENTS.md` | command reference + operating loop |
| `Cargo.toml` | version 0.11.0 |

## 6. Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **Migration damage on real DBs.** | Additive columns with defaults only; transactional `run_migrations`; BDD scenario migrates v6 fixture and verifies row integrity. |
| R2 | **Auto-generated patterns are low quality.** | Propose-by-default; `--apply` is explicit; `consolidated` tag + confidence < 1 distinguishes them; templated descriptions never pretend to be insights. |
| R3 | **Similarity-gate noise on every log.** | Active-only filter removes the biggest noise class (superseded decisions); classification makes the block actionable; resolution flags unblock in one command; `--force` retained. |
| R4 | **Time-dependent scores break reproducibility.** | Stored columns are the source of truth and round-trip verbatim; `effective_confidence` is derived-on-read by design (same as recency decay in v0.10.0). |
| R5 | **Cycles in `causes` chains.** | `transitive_reachable` already terminates on cycles (existing unit test `transitive_reachable_terminates_on_cycles`). |
| R6 | **Consolidate cost on large tables.** | GROUP BY aggregation in SQL; tag/anchor prefilter before any FTS; caps on dedupe output; no Rust-side pairwise loops. |
| R7 | **Confidence factor reorders prime output.** | Intended behavior; patterns only; called out in release notes; `LAMBDA` shared with tier-1 keeps semantics uniform. |
| R8 | **`derived_from` domain/range blocks legitimate manual links.** | Constraints warn + `--force` override (existing link.rs behavior for all constrained rels). |
| R9 | **Classification heuristic mislabels supersedes/conflicts.** | `suggested_relation` is advisory output, never auto-executed; agent reads the similar summaries before choosing a flag. |

## 7. Phases

Ordering: schema first (everything reads the columns), causal rels second (smallest, independent, ships value immediately), contradiction gate third, consolidate + decay fourth (largest), docs/dogfood/release last. Each phase leaves the tool fully working.

### Phase 1 — Schema v7 & pattern model
- `MIGRATION_V7`, `LATEST_VERSION = 7`, `Pattern` fields, `pattern log/update --confidence`, export/import round-trip.
- **Gate:** v6→v7 fixture migration BDD green; round-trip test green.

### Phase 2 — Causal rels & `graph why`
- `causes`/`derived_from` RelSpecs; `GraphCmd::Why` upstream/downstream.
- **Gate:** chain BDDs green; live dogfood — record a real causal chain in engrams' own DB and walk it.

### Phase 3 — Contradiction gate
- `find_similar` active filter; hit classification; `--supersedes` / `--conflicts-with` transactional paths.
- **Gate:** BDDs green (block, classify, resolve both ways, force bypass).

### Phase 4 — Consolidate & decay
- `src/ops/consolidate.rs` (propose/apply/confirm); `confidence_expr` + scoring integration; doctor advisory.
- **Gate:** seeded-fixture BDDs green; dogfood `consolidate` on engrams' own DB, review candidates by hand.

### Phase 5 — Docs, dogfood wrap-up, release
- `docs/memory/engrams.md`, `AGENTS.md`, version bump, `engrams export`, decision anchors, release notes.
- **Gate:** fresh-workspace smoke (init → log → consolidate → why); full `cargo test`.

## 8. Acceptance criteria

### S3 — Migration

```gherkin
Given a workspace database at schema v6 with 2 existing patterns
When the engrams binary next opens the database
Then PRAGMA user_version is 7
And system_patterns has columns confidence (REAL, default 1.0) and last_confirmed_at (TEXT, NULL)
And both pre-existing patterns are intact with confidence 1.0 and NULL last_confirmed_at
```

### S4/S12 — Confidence fields & decay

```gherkin
Given a pattern logged with --confidence 0.7 twenty days ago and last_confirmed_at set at log time
When I run `engrams pattern get <id>`
Then the response includes confidence 0.7
And effective_confidence is 0.7 * exp(-0.01155 * 20) within 1e-6
```

```gherkin
Given two patterns with equal importance and timestamps but one confirmed today and one last confirmed 120 days ago
When I run `engrams prime`
Then the recently confirmed pattern outranks the stale one
```

### S5/S6/S7/S8 — Contradiction gate

```gherkin
Given an active decision "Use Vec<&dyn ToSql for parameters" and a superseded decision with near-identical summary
When I log a new decision with summary "Use Vec<&dyn ToSql for SQL parameters"
Then the response is inserted=false with the active decision listed and the superseded one absent
And the active hit carries suggested_relation "supersedes" when it shares an anchor with the new decision's anchors
And a hit sharing no anchors carries suggested_relation "conflicts_with"
```

```gherkin
Given an active decision 42 similar to my new summary
When I run `engrams decision log --summary "..." --supersedes 42`
Then the new decision is inserted
And decision 42 has status "superseded"
And a supersedes link (new → 42) exists
And the response includes both decisions
```

```gherkin
Given an active decision 42 with no shared anchors
When I run `engrams decision log --summary "..." --conflicts-with 42`
Then the new decision is inserted with status "active"
And a conflicts_with link joins 42 and the new decision
```

### S1/S2 — Ontology

```gherkin
When I run `engrams link add --source-type decision --source-id 5 --target-type decision --target-id 7 --rel caused_by`
Then the link is stored canonically as causes from 7 to 5
And `engrams graph stats` counts it under canonical rel causes
```

### S2 surface — graph why

```gherkin
Given links A causes B and B causes C
When I run `engrams graph why decision:C`
Then the chain lists B at depth 1 and A at depth 2 with roots [A]
When I run `engrams graph why decision:A --down`
Then the chain lists B at depth 1 and C at depth 2
```

### S9/S10/S11 — Consolidate

```gherkin
Given 4 Done progress entries across 3 distinct days all anchored to src/ops/scoring.rs sharing tag "performance"
When I run `engrams consolidate`
Then output is JSON with one candidate whose evidence lists those 4 entry ids
And initial_confidence is 0.65
And nothing was written to system_patterns
```

```gherkin
Given the same workspace
When I run `engrams consolidate --apply`
Then a system_pattern exists tagged "consolidated" with confidence 0.65
And derived_from links join it to all 4 evidence progress entries
```

```gherkin
Given a consolidated pattern last confirmed 10 days ago
And a new matching progress entry logged yesterday
When I run `engrams consolidate`
Then the pattern's last_confirmed_at is updated to now
And no duplicate candidate is proposed for its existing evidence
```

```gherkin
Given two active decisions with near-identical summaries sharing tag "retrieval"
When I run `engrams consolidate`
Then merge_suggestions lists the pair
And running consolidate --apply creates no merge
```

### Round-trip

```gherkin
Given a database containing patterns with confidence 0.65 and last_confirmed_at set
When I run `engrams export` then `engrams import` into a fresh workspace
Then confidence and last_confirmed_at values are preserved exactly
```

### S13 — Doctor

```gherkin
Given a consolidated pattern with last_confirmed_at 200 days old
When I run `engrams doctor`
Then the JSON includes an unconfirmed-patterns advisory naming that pattern
```
