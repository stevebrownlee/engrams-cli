# 0002 — Schema Formation

## Summary

Every conversation with an LLM starts from scratch: the developer's knowledge base stores discrete facts — decisions, patterns, progress — linked pairwise, and retrieval can find a fact's neighbors but not a *concept*. This feature lets the knowledge base form concepts the way its user does. When items keep behaving as a group — committed together, retrieved together, anchored to the same code, created near in time — and that group is dense, stable across rebuilds, and repeatedly useful, the tool proposes it as a **schema**: a named, summarized meta-node that a human confirms into existence.

Confirmed schemas become first-class citizens of the graph: centrality ranks them as concepts rather than files, causal walks traverse them, and one fact may belong to several schemas at once. Retrieval changes shape accordingly — session priming leads with a short block of the most useful schemas, topic queries hit schema summaries before raw facts, and recording new knowledge suggests which schemas it belongs to, attaching only on explicit say-so.

Schemas remain projections, never replacements: every underlying detail stays one hop away, mechanically drafted summaries rank below agent-refined ones, and nothing is written without a confirming hand. This release ships the full formation, growth, and surfacing loop; adaptation — drift detection, contradiction flags, merge and split — is deliberately out of scope.

## Acceptance criteria

### AC-1: Detect recognizable concept clusters in this project's own history

**Given** the workspace's own knowledge base with its accumulated history
**When** the dogfood evaluation runs the detection stage over it
**Then** each benchmark concept — the policy engine, scoring and decay, and the graph/ontology workstream — is marked matched or unmatched
**And** every match cites the cluster and the member items that justify the match

### AC-2: Produce identical clusters for an unchanged graph on every run

**Given** an unchanged workspace graph
**When** the detection stage runs twice
**Then** both runs produce identical cluster membership and identical staging signatures

### AC-3: Propose only candidates clearing density, stability, and reward gates

**Given** staged candidates, some above and some below the gate thresholds
**When** schema candidates are listed
**Then** above-gate candidates appear as ready
**And** below-gate candidates appear as not-ready with the failed gate named
**And** only above-gate candidates can be applied

### AC-4: Write nothing without an explicit apply or confirm

**Given** staged candidates and existing schemas
**When** scanning or surfacing suggestions without apply or attach flags
**Then** no schema rows, membership edges, or member facts are created or changed

### AC-5: Promote schemas to first-class graph nodes without absorbing members

**Given** a confirmed schema with attached members
**When** graph analytics run
**Then** the schema participates as a node connected through membership edges
**And** every member fact remains individually retrievable and unchanged

### AC-6: Draft summaries mechanically and rank them below refined ones

**Given** a schema created with a mechanically drafted summary
**When** its summary is refined through the refinement command
**Then** the new summary is recorded as agent-authored
**And** it now ranks above comparable drafted schemas in every ranked surface

### AC-7: Surface schemas ahead of raw facts in prime, brief, and query

**Given** at least one confirmed schema
**When** priming, briefing, or querying runs
**Then** priming leads with a schema block ranked by usefulness, agent-authored first
**And** a query whose terms match a schema's territory returns that schema's summary and top members before raw facts
**And** a query with no schema match returns exactly what it would have before this feature

### AC-8: Assimilate new knowledge into existing schemas only on request

**Given** a confirmed schema and a newly logged item whose summary and tags fit its territory
**When** the item is recorded
**Then** the output suggests that schema
**And** no membership edge exists until the attach flag is used
**And** attaching updates the schema's confirmation timestamp

### AC-9: Keep schema identity stable across graph rebuilds

**Given** an applied schema
**When** the graph is rebuilt several times as knowledge grows
**Then** the schema persists with its identity intact
**And** membership changes beyond the staging seam surface as proposed confirmations rather than silent mutations

### AC-10: Record retrieval surfacing to reward schemas and bound storage

**Given** confirmed schemas in active use
**When** retrieval surfaces a schema alongside other results
**Then** the surfacing is recorded with its co-surfaced records
**And** older records are pruned to a rolling window so storage stays bounded

### AC-11: Round-trip schemas through export, import, and fresh-database setup

**Given** schemas with memberships and telemetry
**When** export and import run, and a fresh database is initialized
**Then** schemas and memberships survive the round-trip
**And** the fresh database contains every table this feature defines

## Out of scope

- Tuning triggers: drift hook, contradiction-within-schema hook, member-decay detachment (deferred to the adaptation release)
- `needs_review` status surfacing in prime and doctor (status values exist in the data model now for forward-compatibility)
- Merge/split proposals with `supersedes` reuse
- Schema-to-schema `related` edges and nesting
- Report visualization schema layer
- Embedding- or vector-based similarity (rejected during design: behavioral and lexical signals only)
- Automatic member attachment or model-generated summaries inside the CLI (drafts are mechanical; refinement is agent-driven through a command)

## Open questions

None blocking. The five design-time questions (gate defaults, Jaccard identity
threshold, co-retrieval ramp, prime block size, assimilation threshold) were
resolved on 2026-09-04; their launch defaults are embedded in Architecture
below. All remain sweep constants owned by the phase-1 dogfood gate (AC-1),
which freezes them before phase 2 begins.

## Architecture

The feature lives in a new ops module `src/ops/schemas/` (`mod.rs` command handlers, `detect.rs` detection core, `draft.rs` mechanical name/summary generation). The module name is plural because `src/schema.rs` already means database schema in this codebase; the external CLI namespace `engrams schema` is unambiguous. Detection runs inside the binary as a real subcommand's machinery — no example binaries, no library-target extraction.

Formation pipeline, run by `engrams schema scan`:

1. **Union adjacency.** The existing in-memory graph projection (`src/ops/graph/model.rs`) contributes declared edges and anchor connections; behavioral overlays are computed at scan time — co-commit (shared commit references), co-retrieval (from `retrieval_surfaces` telemetry, weight scaled by co-observation count through the same normalize-and-cap curve as co-commit — empty telemetry contributes zero edges by construction, so the ramp needs no schedule constant and puts no wall-clock in the deterministic scan), and temporal proximity — each layer carrying a configurable weight.
2. **Deterministic community detection.** Hand-rolled Louvain modularity optimization with sorted node iteration and fixed pass order, over the weighted undirected adjacency. At the current scale (hundreds of nodes) this is milliseconds and roughly 150 lines.
3. **Staging upsert.** Each cluster's member-set signature is matched against `schema_candidates` by Jaccard ≥ 0.7; matches advance stability and reward counters, misses insert fresh candidates. Every match or miss records drift counts (members removed, members added) on the candidate, so dogfood stability resets are classified by drift type — boundary swaps, absorption growth, genuine churn — rather than guessed at. Staging is rebuilt state — cheap, self-healing, and carries no migration debt. At 0.7 the drift budgets are: additions ≤ 43% of cluster size, removals ≤ 30%, swaps ≤ ~18% per direction; growth (the dominant mode in an accumulating KB) is forgiving, boundary swaps are the fragile case.
4. **Gates.** A candidate is proposal-ready when density, stability across rebuilds, and reward hits all clear thresholds. Launch defaults: density ≥ 0.5, stability ≥ 3, reward ≥ 0 — the zero reward floor exists because `retrieval_surfaces` ships empty and any higher floor would block all formation until telemetry exists; structure proposes first, reward takes over as telemetry accumulates.
5. **Apply.** `--apply` creates the schema row, mechanical draft (name from dominant tag/anchor slug with collision handling, summary from centroid tags, top-central members, anchors, density — the `consolidate` precedent), and `member_of` edges through the existing links machinery.

Assimilation hooks into `decision`/`pattern`/`progress` log commands: new items are matched against schema centroids lexically (token overlap plus FTS — convention-aware, no model); fit ≥ 0.4 of the centroid's distinctive tokens fires a suggestion (top-2 matches, scores shown — early centroids are thin, so a stricter bar would rarely fire and would starve calibration); `--schema <id>` attaches `member_of` at write time and bumps confirmation recency. Every fired suggestion persists to `schema_suggestions` with status `suggested`; attach flips it to `accepted`; explicit `--schema none` flips it to `declined` — decline is observable only through opt-out, since a never-attached suggestion is ambiguous between declined, ignored, and session-ended, so unresolved rows stay `suggested`. The status history is the calibration corpus v0.14's decline-rate adaptation consumes.

Retrieval tiering: `prime` leads with a schema block (K=3, one line each, ranked by reward hits then centrality; agent-authored summaries first, drafts flagged and ranked below); `brief schema:<id>` composes summary, members by type, anchors, and schema neighborhood; `query` tries centroid match before today's FTS path, falling through unchanged on miss, with `miss_guidance` gaining schema centroids. Reinforce-on-read extends to schemas; `src/ops/scoring.rs`'s confidence multiplication applies unchanged.

Design choices:

1. **First-class schema entities over ephemeral computed clusters** — addressable nodes are what make centrality, causal walks, and (later) schema-to-schema structure expressible; a fact may belong to several schemas, mirroring how one memory belongs to multiple concepts.
2. **Deterministic Louvain over label propagation** — stability tracking requires that the same graph yields the same clusters every run; label propagation is iteration-order-unstable. Hand-rolled over a community-detection crate — dependency discipline outweighs ~150 lines at KB scale.
3. **Propose-confirm covenant over automatic promotion** — phantom schemas (confident-sounding clusters that were never real concepts) poison retrieval; propose-only, demoted drafts, reward gates, and drafts never outranking facts on contradiction are the mitigations.
4. **`member_of` as an ontology relationship over a dedicated join table** — membership rides existing link machinery, graph analytics, export/import, and the relationship ontology in `src/ops/graph/rel.rs` for free.
5. **Staging rebuilt with Jaccard identity over persisted candidate state** — ephemeral cluster bookkeeping should not outlive the graph it was derived from; only confirmed schemas persist.

## Data model

Migration to schema version 12 in `src/schema.rs` (v8–v11 shipped with v0.12.0; this feature's tables land as `MIGRATION_V12`):

```sql
CREATE TABLE schemas (
  id                INTEGER PRIMARY KEY,
  uuid              TEXT NOT NULL UNIQUE,
  name              TEXT NOT NULL UNIQUE,
  summary           TEXT NOT NULL,
  summary_source    TEXT NOT NULL DEFAULT 'drafted',  -- 'drafted' | 'agent'
  status            TEXT NOT NULL DEFAULT 'active',   -- 'proposed' | 'active' | 'needs_review' | 'archived'
  centroid_json     TEXT NOT NULL,                    -- tag/anchor/token centroid for matching
  confidence        REAL NOT NULL DEFAULT 0.0,        -- scoring.rs multiplication extends here
  importance        REAL NOT NULL DEFAULT 0.0,
  access_count      INTEGER NOT NULL DEFAULT 0,
  last_accessed_at  TEXT,
  last_confirmed_at TEXT,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);

CREATE TABLE schema_candidates (          -- staging; rebuilt by each scan
  cluster_sig       TEXT PRIMARY KEY,     -- sorted member-key signature
  member_keys_json  TEXT NOT NULL,
  density           REAL NOT NULL,
  stability_count   INTEGER NOT NULL DEFAULT 1,
  reward_hits       INTEGER NOT NULL DEFAULT 0,
  last_drift_removed INTEGER NOT NULL DEFAULT 0, -- last match: members departed
  last_drift_added   INTEGER NOT NULL DEFAULT 0, -- last match: members arrived
  first_seen_at     TEXT NOT NULL,
  last_seen_at      TEXT NOT NULL
);

CREATE TABLE retrieval_surfaces (         -- telemetry; rolling-window pruned
  ts         TEXT NOT NULL,
  cmd        TEXT NOT NULL,
  arg        TEXT,
  node_kind  TEXT NOT NULL,
  node_id    INTEGER NOT NULL
);
CREATE TABLE schema_suggestions (        -- fired-suggestion history; v0.14 calibration
  ts         TEXT NOT NULL,
  schema_id  INTEGER NOT NULL,
  item_kind  TEXT NOT NULL,              -- 'decision' | 'pattern' | 'progress-entry'
  item_id    INTEGER NOT NULL,
  fit        REAL NOT NULL,
  status     TEXT NOT NULL DEFAULT 'suggested',  -- 'suggested'|'accepted'|'declined'
  PRIMARY KEY (schema_id, item_kind, item_id)
);
CREATE INDEX idx_retrieval_surfaces_node ON retrieval_surfaces(node_kind, node_id, ts);
```

- `schemas` mirrors the v0.10/v0.11 entity columns so scoring, decay, and prune reuse them as-is.
- FTS index over `schemas(name, summary)` follows the existing FTS5 trigger pattern.
- `member_of` joins the relationship ontology (`src/ops/graph/rel.rs`): any entity type — `decision`, `progress-entry`, `system-pattern`, `custom-data`, `schema` — to `schema`, many-to-many, stored as rows in the existing links machinery.
- `schema_suggestions` persists every fired suggestion and its resolution — the calibration corpus v0.14's decline-rate adaptation consumes. Day-one recording mirrors the co-retrieval count-ramp logic: adaptation lands with history instead of starting cold.

## API surface

All output is JSON, per repo invariant.

- `engrams schema scan [--apply]` — run detection and staging upsert; report ready and not-ready candidates with gate detail; `--apply` creates schemas, `member_of` edges, and mechanical drafts for ready candidates only.
- `engrams schema list [--status <status>]` — schemas with blended score, summary source, member count.
- `engrams schema show <id|name>` — summary, members grouped by type, anchors, schema neighborhood.
- `engrams schema refine --id <n> --summary "..." [--name "..."]` — agent-authored update; sets `summary_source='agent'`.
- `engrams schema confirm <id>` — bump `last_confirmed_at`.
- `engrams decision|pattern|progress log ... [--schema <id>|none]` — output gains `schema_suggestions: [{id, name, fit}]` above threshold; `--schema <id>` attaches `member_of` at write time and marks the matching suggestion row accepted; `--schema none` marks fired suggestions declined.
- `engrams prime` — gains a leading schema block (top-K blended score, agent-authored first, drafts flagged).
- `engrams brief schema:<id>` and topic matches — schema composite read.
- `engrams query <q>` — centroid-match tier before the existing FTS path; unchanged fall-through on miss; `miss_guidance` includes schema centroids.

Errors: unknown schema id/name in `show`/`refine`/`confirm` exits non-zero with a JSON error object, matching existing command conventions.

## Dependencies

Internal:

- `src/ops/graph/model.rs` — in-memory graph projection and analytics the detection runs over
- `src/ops/graph/rel.rs` — relationship ontology; `member_of` registration
- `src/ops/scoring.rs` — blended scoring, confidence multiplication, reinforce-on-read
- `src/ops/consolidate.rs` — precedent for mechanical drafts (slug, collision-free names)
- `src/schema.rs` — migration to schema version 8
- `tests/memory.rs` — S-numbered BDD precedent for the new suite

External:

- None. Deterministic Louvain is hand-rolled; matching is lexical via existing FTS5. No new crates.

## Verification strategy

Per-phase verification commands (Gate 3):

- Build: `cargo build`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Phase-scoped tests: `cargo test schemas`

Full-suite verification (Gate 5):

- Full test suite: `cargo test` (all 110 existing tests plus the new S-numbered suite green)
- Lint all targets: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Manual dogfood: run `engrams schema scan` over this repository's own live database; confirm each benchmark concept (policy engine, scoring and decay, graph/ontology) is matched by a recognizable cluster or explicitly unmatched — tune weights here before trusting anything downstream. Apply one schema, run `engrams prime`, confirm the schema block leads and an agent-refined schema outranks a drafted one. Run the scan twice and diff for identical output. Log a decision touching a confirmed schema's territory and confirm the suggestion appears with no write until `--schema` is passed.

The dogfood evaluation is the first implementation phase's exit gate: detection quality is judged on the live graph before promotion logic is built. The new BDD suite (`tests/schemas.rs`) covers gate arithmetic, determinism, identity survival across rebuilds, zero surprise writes, telemetry recording and pruning, prime/brief/query integration, export/import round-trip, and fresh-database schema parity (the migration lesson from decision #66).
