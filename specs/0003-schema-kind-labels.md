# 0003 — Schema Kind Labels

## Summary

When the tool watches its own knowledge base and notices things that keep appearing together, it proposes groups. Today a group becomes a proposal only by proving it is statistically real — dense, stable, seen before. But real is not the same as worth keeping: dogfooding on this project's own database surfaced one-time campaigns and whole-file inventories sailing through those checks alongside genuinely living concepts.

This feature adds a second judgment layer. Every gate-passing group also gets tagged with what kind of thing it is — a **schema** (a recurring practice with a trigger that keeps coming back), a **story** (a one-time burst that has ended), an **inventory** (a pile of files rather than an idea), or **unclear** (mixed signals; a human should look). Each tag comes with plain-language reasons, so a person skimming the proposal list can pass over junk in seconds instead of researching each group by hand.

The tags advise and restrain, but never decide: the list shows stories and inventories with their labels rather than hiding them, a person confirms schemas exactly as before, and the batch auto-promote option holds back anything not tagged schema — machine restraint is the only behavior that changes. Labeling quality is proven before anything else ships: the labeler must reproduce the human judgments already made over this project's own proposals during dogfooding.

## Acceptance criteria

### AC-1: Label every gate-passing candidate with a kind and reasons

**Given** gate-passing scan candidates
**When** the scan lists them
**Then** each candidate carries exactly one kind: schema, story, inventory, or unclear
**And** each candidate carries at least one plain-language reason supporting its kind

### AC-2: Order ready candidates schema-first

**Given** ready candidates of mixed kinds
**When** the scan output is produced
**Then** schema-kind candidates appear before story, inventory, and unclear candidates
**And** candidates of the same kind keep their existing order

### AC-3: Classify ended one-time bursts as stories

**Given** a candidate whose recorded activity falls inside a single contiguous time stretch with no re-activation after it
**When** it is labeled
**Then** it is labeled story
**And** the reason names the single-burst, no-reactivation pattern

### AC-4: Classify file-dominant groups as inventories

**Given** a candidate whose members are overwhelmingly file nodes with no checkable rules or decisions among them
**When** it is labeled
**Then** it is labeled inventory
**And** the reason names the member mix

### AC-5: Mark conflicting signals as unclear

**Given** a candidate whose signals conflict, such as recurring activity but no identifiable trigger
**When** it is labeled
**Then** it is labeled unclear
**And** the reasons for and against the conflicting kinds are all shown

### AC-6: Hold auto-apply back from non-schema kinds

**Given** ready candidates including story, inventory, and unclear kinds
**When** apply runs
**Then** only schema-kind candidates are promoted
**And** each held-back candidate appears in the skipped list with its kind named
**And** no schema rows or membership edges are created for held-back candidates

### AC-7: Produce identical labels on re-run

**Given** an unchanged knowledge base
**When** labeling runs twice
**Then** both runs produce identical kinds and identical reasons for every candidate

### AC-8: Reproduce the dogfood answer key

**Given** the hand-labeled evaluation set derived from this project's own scan output
**When** the labeler runs over it
**Then** every group the developer called a schema is labeled schema
**And** every group the developer called a story or inventory is labeled accordingly or unclear
**And** every disagreement between machine and human labels is listed for review

## Out of scope

- Removing, archiving, or auto-declining story/inventory candidates (labels inform; a human decides)
- Changing the formation gates themselves (density, stability, and reward thresholds are untouched; labeling sits after the gates)
- Learned or model-based classification (rule-based lexical and behavioral signals only; decline telemetry accumulates for a future adaptation release)
- Suggesting manual schemas for themes that never clustered (the release-routine miss is a separate feature)
- Kind categories beyond schema, story, inventory, and unclear
- Kind labels on confirmed schemas (promotion implies schema-kind under the covenant; explicit human confirmation needs no label)

## Open questions

- What silence gap separates two awake stretches of a group's activity?
- What file-node share marks a group as an inventory?
- Is one checkable rule or one shared file anchor enough to count as a trigger, or must shared vocabulary also confirm it?
- Should unclear candidates be held back from apply alongside story and inventory, or only warned about? (Current draft: held back.)
- Where does the hand-labeled replay corpus live — a test fixture, or custom-data in the live database?

## Architecture

The labeler lives in a new `src/ops/schemas/kind.rs`: a pure pass that runs inside `schema scan` after the gates. Gate-passing candidates are classified before output; kind and reasons are stored on the staging row so they persist between scans with the rest of the candidate's derived state. No new subcommand — the labels ride the existing scan surfaces.

Four signals, all computed from data the tool already collects:

1. **Awake stretches.** A group's retrieval and creation timestamps, bucketed into stretches separated by a silence gap. One stretch with no re-activation after it leans story; two or more separated stretches mean recurrence — the calendar-neighborhood and same-idea cases diverge only here.
2. **Member mix.** Shares by member type: file nodes, checkable rules (patterns carrying check expressions), and narrative progress entries. Overwhelmingly file nodes with nothing checkable or deciding among them leans inventory.
3. **Trigger surface.** A group has a trigger if any member is a checkable rule, members share anchor paths, or their summaries share distinctive vocabulary above the existing assimilation overlap threshold.
4. **Ordered conservative rules.** The signals combine through a fixed rule order ending in unclear — muddy signals never guess.

Determinism: kind is a pure function of stored timestamps and member metadata, never wall-clock — the same discipline the scan's co-retrieval ramp already follows. "Time since last awake" may be displayed but never decides a kind.

Design choices:

1. **Labels over gates.** Extends the propose-confirm covenant: apply promotes only schema-kind candidates; everything else waits for explicit human confirmation. A blocking gate was rejected because a heuristic mislabel would make candidates silently disappear; a labeled list keeps human judgment final and errors visible.
2. **Ordered rules with an unclear escape, not a score.** No training corpus exists yet; fired-suggestion and decline telemetry accumulate as the calibration corpus for the adaptation release. Until then, conservative beats confident.
3. **Labels on staging only.** The covenant constrains the machine path; a human confirming a candidate needs no kind column, so the schemas table is untouched and labels die with the staging rebuild they belong to.
4. **Replay before wiring.** The first implementation phase labels the dogfood corpus and is judged against the hand-labeled answer key before any surface work merges — mirroring spec 0002's detection-quality exit gate.

## Data model

Migration to schema version 13 in `src/schema.rs` (`MIGRATION_V13`):

```sql
ALTER TABLE schema_candidates ADD COLUMN kind TEXT NOT NULL DEFAULT 'unclear';
ALTER TABLE schema_candidates ADD COLUMN kind_reasons_json TEXT NOT NULL DEFAULT '[]';
```

No new tables: retrieval timestamps, pattern check columns, and entity timestamps already exist. Confirmed schemas are unchanged.

## API surface

All output is JSON, per repo invariant.

- `engrams schema scan` — gate-passing candidates gain `kind` ("schema" | "story" | "inventory" | "unclear") and `kind_reasons` (list of strings); ready candidates ordered schema-first; not-ready candidates unchanged.
- `engrams schema scan --apply` — promotes only kind=schema candidates; every held-back candidate appears in the existing skipped list with its kind.
- `engrams prime`, `brief`, `query`, `schema list/show/confirm/refine` — unchanged.
- Errors: existing conventions unchanged (unknown schema references exit non-zero with a JSON error object).

## Dependencies

Internal:

- `src/ops/schemas/` — `detect.rs` staging upsert and `mod.rs` scan wiring the labeler hooks into
- `src/ops/graph/` — member types and anchor paths feeding member mix and trigger surface
- `retrieval_surfaces` telemetry (schema v12) — awake-stretch timestamps
- Pattern check columns (schema v5) — checkable-rule detection
- Assimilation centroid matching (token overlap) — vocabulary trigger reuse
- `src/schema.rs` — `MIGRATION_V13`
- `tests/schemas.rs` — BDD suite home for the new criteria

External:

- None. Lexical and behavioral signals only; no new crates.

## Verification strategy

Per-phase verification commands (Gate 3):

- Build: `cargo build`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Phase-scoped tests: `cargo test schemas`

Full-suite verification (Gate 5):

- Full test suite: `cargo test`
- Lint all targets: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Manual dogfood: run the labeler over this repository's live database and score it against the hand-labeled answer key (AC-8); confirm schema-first ordering in the scan list. On a copy of the live database — verify the echoed database path before running, per the project's live-db incident history — run apply and confirm only schema-kind candidates promote; then run the scan twice and diff for identical labels.
