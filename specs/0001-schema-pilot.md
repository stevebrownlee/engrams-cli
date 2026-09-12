# 0001 — Schema Formation Pilot: Cluster Quality on the Live Graph

> **Status: superseded by [0002-schema-formation.md](0002-schema-formation.md).**
> The pilot was folded into 0002 as its dogfood phase (AC-1): detection
> quality is validated inside the feature's own pipeline instead of by a
> throwaway example binary. The crate has no library target, so the pilot's
> example-binary architecture was unbuildable without a production refactor.
> Kept for the record; do not implement this spec.

## Summary

The knowledge base today stores discrete facts — decisions, patterns, progress entries — linked pairwise. Retrieval finds neighbors of a query, but it cannot hand you a *concept*: "everything about how the policy engine fits together." The next major feature (schema formation) will promote behaviorally-related clusters of facts into first-class schema entities that retrieval surfaces as compressed understanding. That feature's entire value rests on one assumption: that the behavioral signals already in the database — co-commit history, shared anchors, time proximity — are strong enough to produce clusters a human recognizes as real concepts.

This pilot validates that assumption before any production surface is built. It runs a throwaway, read-only cluster detector over the project's own live knowledge graph and produces a human-readable report: every detected cluster with members, cohesion, dominant tags and anchors; an evaluation of whether expected concepts actually emerged; and a sensitivity sweep showing how clusters shift as signal weights vary.

The pilot writes nothing to the database, adds no CLI commands, and ships no migration. Its single deliverable is evidence: a recorded go/no-go verdict — with tuned weight defaults — that either feeds the full schema-formation spec or stops the feature before it's built on sand.

## Acceptance criteria

### AC-1: Running the pilot leaves the workspace database untouched

**Given** the project's live knowledge-base database
**When** the pilot runs to completion, or fails mid-run
**Then** every table holds exactly the same rows as before the run
**And** no schema-migration version changes

### AC-2: Repeat runs produce identical clusters

**Given** the workspace graph is unchanged between runs
**When** the pilot is executed twice
**Then** both cluster reports show identical cluster membership
**And** identical per-cluster scores

### AC-3: Report lists every cluster with identifying detail

**Given** the workspace graph contains at least 100 interconnected knowledge items
**When** the pilot runs
**Then** the report lists each detected cluster with its member items, a cohesion score, and its dominant tags and anchors
**And** clusters are ordered by cohesion, strongest first

### AC-4: Expected concepts are evaluated, not assumed

**Given** the three benchmark concepts: the policy engine, scoring and decay, and the graph/ontology workstream
**When** the pilot evaluates its cluster report
**Then** each benchmark concept is marked as matched or unmatched
**And** every matched concept cites the cluster and the member items that justify the match

### AC-5: Weight sensitivity sweep accompanies the report

**Given** the cluster report for the default signal weights
**When** the pilot completes its sweep
**Then** cluster count, size distribution, and membership overlap versus the default weights are reported for each weight preset

### AC-6: Go/no-go verdict recorded with evidence

**Given** the cluster report and sensitivity sweep
**When** the pilot concludes
**Then** a go/no-go verdict is recorded in the project's decision log
**And** the verdict cites benchmark match results and the chosen weight defaults

## Out of scope

- The schema-formation feature itself: the schemas entity and table, `member_of` ontology edges, candidate staging, promotion gates, and retrieval surfacing (separate spec, to follow a go verdict)
- Retrieval-surfaces telemetry and the co-retrieval signal: the pilot clusters on co-commit, shared anchors, and time proximity only; co-retrieval waits for the telemetry that ships with the full feature
- Any installed CLI command, JSON output contract, or shipped surface for the detector (throwaway example binary only)
- Tuning triggers (drift, contradiction-within-schema), merge/split restructuring, and schema-to-schema structure (future direction)
- Embedding- or vector-based similarity (rejected during design: behavioral and structural signals only)

## Open questions

- What are the initial weight presets for the sensitivity sweep, and how wide should the sweep range be?
- What minimum cluster size qualifies as a candidate for benchmark evaluation?

## Architecture

The pilot is a cargo example binary at `examples/schema_pilot.rs`, invoked as `cargo run --example schema_pilot -- --workspace .`, emitting a Markdown report to stdout. It loads the existing in-memory graph model from `src/ops/graph/model.rs` over the workspace database discovered by `src/db.rs`, opened read-only (SQLite `SQLITE_OPEN_READ_ONLY`); edge weighting follows the relationship ontology in `src/ops/graph/rel.rs`.

The adjacency is the union of two signal layers. The structural layer is what the model already loads: declared links and anchor connections through code-graph nodes. The behavioral layer is computed inside the example: co-commit (items sharing a commit reference), shared anchors (counted through the existing code-graph hubs), and time proximity (items created within a bounded window of each other). Each layer carries a configurable weight; the sensitivity sweep varies them.

Detection is deterministic Louvain modularity optimization, hand-rolled in the example (~150 lines at the current scale of roughly 250 nodes): sorted node iteration, fixed pass order, no randomness. The report orders clusters by internal cohesion and lists members, dominant tags, and anchors per cluster; a benchmark section matches the three named concepts to clusters; a sweep section tabulates cluster count, size distribution, and membership overlap across weight presets.

Design choices:

1. **Example binary over a hidden subcommand.** The pilot covenant is zero shipped surface — no CLI command, no migration. Cargo examples are the idiomatic home for compile-checked scratch tooling; a git-ignored standalone script was rejected because it loses type-checking and model reuse.
2. **Deterministic Louvain over label propagation.** Label propagation is iteration-order-dependent and unstable across runs, which would invalidate the determinism criterion outright. A community-detection crate was rejected to keep the spike dependency-free; at this graph size, modularity optimization is small enough to hand-roll.
3. **Read-only reuse of the production model over a bespoke loader.** The pilot must measure the graph the production feature will actually see; a bespoke loader could diverge silently and validate the wrong graph.

After the verdict, the example stays in-tree until the full feature's scan subsumes it, at which point it serves as the reference implementation and fixture generator for the schema test suite. Acceptance criteria for read-only behavior and determinism get real automated coverage in phase-scoped tests (a seeded temporary workspace, run twice, assert identical membership; hash table contents before and after to prove immutability).

## Data model

_No data model changes._

## API surface

_No API surface changes._ The example binary is a development artifact, not a contract; nothing it exposes is stable or installed.

## Dependencies

Internal:

- `src/ops/graph/model.rs` — in-memory graph model and analytics, loaded read-only
- `src/db.rs` — workspace discovery and read-only database connection
- `src/ops/graph/rel.rs` — relationship ontology (edge kinds, symmetry, weights)

External:

- None. Explicitly no new crates; the detector is hand-rolled to keep the spike dependency-free.

## Verification strategy

Per-phase verification commands (Gate 3):

- Build the example: `cargo build --examples`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Phase-scoped tests: `cargo test schema_pilot`

Full-suite verification (Gate 5):

- Full test suite: `cargo test` (all existing tests stay green)
- Lint all targets: `cargo clippy --all-targets`
- Format: `cargo fmt --check`
- Manual: run `cargo run --example schema_pilot` against the live workspace database; review the cluster report and confirm each benchmark concept (policy engine, scoring and decay, graph and ontology) is either matched by a recognizable cluster or explicitly marked unmatched; run the pilot twice and confirm identical output; record the go/no-go verdict with the chosen weight defaults in the project's decision log.
