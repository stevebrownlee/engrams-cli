# 0004 — Schema Kind Surfacing

## Summary

When the tool promotes a proposed group into a confirmed schema, the kind label that explained what the group is — recurring practice, ended story, file pile, or mixed signals — currently stays behind in the proposal pipeline. The confirmed pack then surfaces in every new session's opening context, but without its kind: a reader sees a pile of facts, not whether the practice is living or closed.

This feature makes kind a permanent attribute of a confirmed schema. Confirming a proposal keeps its label unless the person confirming chooses otherwise; the opening context block presents each pack with its kind, so a session starts already knowing which recurring practices are alive and worth consulting and which were one-time efforts; and the schema listing and detail views show kind alongside status. No extra retrieval calls are spent learning what a pack is.

## Acceptance criteria

### AC-1: Confirm persists the candidate's kind

**Given** a gate-passing candidate carrying a kind and reason sentences
**When** that candidate is confirmed
**Then** the new schema row stores the same kind and its reasons

### AC-2: Session-opening schemas block reports kind

**Given** one or more confirmed schemas carrying kinds
**When** the session-opening context is requested
**Then** each schema entry includes its kind

### AC-3: Migration backfills pre-existing schemas to unclear

**Given** a database whose confirmed schemas predate kinds
**When** the database is migrated
**Then** every existing schema row carries kind `unclear`
**And** no other column data changes

### AC-4: Schema list shows kind alongside status

**Given** confirmed schemas with differing kinds
**When** schemas are listed
**Then** each entry displays its kind next to its status

### AC-5: Schema detail shows kind and reasons

**Given** a confirmed schema with kind and reasons
**When** its detail view is requested
**Then** the output includes the kind and the reason sentences

### AC-6: Export and import round-trip kinds

**Given** a database with kind-carrying schemas
**When** it is exported and imported elsewhere
**Then** every schema's kind and reasons survive identically

## Out of scope

- Surfacing staged candidates or a proposal queue in the session-opening block (considered; excluded at scoping)
- Kind enrichment of composite node reads (`brief schema:N` and query-hit payloads)
- Overriding the kind at confirm time (a flag) — the machine label is copied as-is
- Correcting a kind after confirm (refine-style command) — future spec
- New kinds, labeler rule changes, scan gate changes, or apply-restraint changes

## Open questions

- When an already-confirmed schema's candidate re-matches with a different machine label, should re-confirmation refresh the kind or leave it stable (the confirm was a human act on that pack)?
- Should the session-opening schemas block order entries by kind (living practices first), or keep current ordering?
- Does this ship with spec 0003 as the same release, or as its own version?

## Architecture

Snapshot persistence at the confirmation boundary. Migration v14 adds kind columns to `schemas` following the v13 pattern: the baseline `SCHEMA` in `src/schema.rs` carries the final columns, `MIGRATION_V14` is ALTER-only, and `LATEST_VERSION` in `src/db.rs` advances to 14 (frozen-migration convention — prior `MIGRATION_V*` constants never change). `src/ops/schemas/confirm.rs` copies the label from the resolved candidate row into the new schema row; `try_resolve_candidate`'s loaded candidate struct gains the kind fields. Read surfaces project the column: `src/ops/schemas/retrieval.rs::prime_block` entries gain `kind`; `src/ops/schemas/list.rs` list entries gain `kind` and show gains `kind` plus the reason sentences. Export/transfer paths include the new columns so round-trips preserve them.

Two design choices. **Snapshot-at-confirm over derive-at-read** (joining schemas back to staged candidates at display time): staged candidate rows are mutable and prunable, so a derived view would silently change under scan drift and lose the label entirely on export; a copied column is stable, migrates, and round-trips. **`unclear` backfill over join-backfill** for pre-existing schemas: the stored evidence for already-confirmed packs does not retroactively decide a kind — the same honesty rule the 0003 dogfood answer key applied.

## Data model

```sql
-- MIGRATION_V14 (ALTER-only; baseline SCHEMA carries the final columns)
ALTER TABLE schemas ADD COLUMN kind TEXT NOT NULL DEFAULT 'unclear';
ALTER TABLE schemas ADD COLUMN kind_reasons_json TEXT NOT NULL DEFAULT '[]';
```

`kind` vocabulary mirrors `schema_candidates.kind`: `schema` | `story` | `inventory` | `unclear`. `kind_reasons_json` holds the JSON array of reason sentences captured at confirm time.

## API surface

No new commands or flags. Changed output contracts:

- `schema confirm` success payload: the `schema` object gains `kind`
- `prime`: each `schemas[]` entry gains `kind`
- `schema list`: each entry gains `kind`
- `schema show`: gains `kind` and `reasons`

Error cases unchanged.

## Dependencies

Internal:

- Spec 0002 (schema formation: `schemas` table, confirm covenant, prime block)
- Spec 0003 (kind labeler, `schema_candidates` v13 kind columns that feed the copy)

External: none — rusqlite (bundled) and serde_json are existing dependencies.

## Verification strategy

Gate 3 (per-phase):

```
cargo build
cargo clippy --all-targets
cargo fmt --check
cargo test --bin engrams schemas
```

Gate 5 (full-suite):

```
cargo test
```

Manual (Gate 5): on a copy of the live database — verify the echoed database path before running, per the project's live-db incident history — migrate to v14 and confirm the two pre-existing schemas read `unclear`; run `prime` and confirm each schemas entry carries its kind; export and import the database and confirm kinds and reasons survive identically.
