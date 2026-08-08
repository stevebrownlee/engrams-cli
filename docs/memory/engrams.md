# Engrams CLI — Full Command Reference

Detailed reference for the `engrams` knowledge base. For the every-session operating loop and behavioral rules, see `AGENTS.md`. Everything below assumes the `engrams` binary is already built (`./target/debug/engrams`); the shorthand `engrams` in examples stands for that path or `cargo run --bin engrams --`. All output is **JSON**.

---

## Quick Reference: Store / Link / Retrieve

The three things you do with engrams: put facts **in**, wire them **together**, and pull them **back out**. Global flags available on most commands: `--db <path>` and `--workspace <path>` (before the subcommand), plus `--compact` and `--fields <a,b>` (on each subcommand).

### Store

| Goal | Command |
|---|---|
| Architectural decision | `engrams decision log --summary "..." --rationale "..." --details "..." --tags a,b --anchor src/foo.rs --pr 42 --status active` |
| Recurring pattern/convention | `engrams pattern log --name "..." --description "..." --tags a,b --anchor src/foo.rs --pr 42` |
| Task progress | `engrams progress log --status InProgress --description "..." --parent-id <n>` |
| Custom key/value | `engrams custom set --category <cat> --key <key> --value '<string-or-json>' --json` |
| Active-context track (full replace) | `engrams active-context update --name <track> --content '<json>'` |
| Active-context track (merge) | `engrams active-context update --name <track> --patch '<json>'` (use `"__DELETE__"` as a value to drop a key) |
| Product context (mission/scope) | `engrams product-context update --content '<json>'` or `--patch '<json>'` |
| File anchor on an item | `engrams anchor add --type decision --id <n> --path src/foo.rs --path src/bar.rs` |
| Bulk atomic insert | `engrams batch --type <decision|progress|pattern|custom-data> --items '<json-array>'` (all-or-nothing; use for seed/restore) |
| Initialize a workspace | `engrams init` (creates `engrams/context.db` and schema; idempotent) |
| Restore from markdown export | `engrams import --path ./engrams_export` (re-ingests exported markdown) |

Repeatable flags (`--anchor`, `--pr`, `--path`) take the same option many times. `decision log` checks for a near-duplicate first; pass `--force` to insert unconditionally. `progress log --check-similar` guards against duplicate progress entries.

### Link — the knowledge graph

| Goal | Command |
|---|---|
| Create a relationship | `engrams link add --source-type decision --source-id 7 --target-type system-pattern --target-id 2 --rel implements --description "JWT middleware implements #7"` |
| Override a constraint violation | add `--force` (the violation is recorded as an intentional override, not silently dropped) |
| List links for an item | `engrams link list --item-type decision --item-id 7 [--rel implements] [--linked-type system-pattern]` |
| Link a PR to an item | `engrams pr add --type decision --id <n> --pr 42` |
| Materialize derived graph edges | `engrams graph rebuild [--min-cochange 2] [--max-commits 500] [--no-git]` |
| Incremental co-change ingest | `engrams graph ingest` (resumes from the last ingested commit) |

`link add` validates `--rel` against the [Relationship Ontology](#relationship-ontology): symmetric/directed/transitive rules, domain/range, `same_type`, `functional_to` cardinality, and `disjoint_with`. `--rel` may be any canonical name or a custom label (custom labels are treated as symmetric and flagged by `doctor`).

### Retrieve

| Goal | Command |
|---|---|
| One-call startup briefing | `engrams prime [--budget <tokens>] [--paths p1,p2] [--tags a,b]` |
| What's anchored to these files | `engrams relevant <paths...>` or `engrams relevant --staged` |
| Cross-type free-text search | `engrams query "<topic>" [--types decision,pattern,custom] [--tags a,b] [--since <rfc3339>] [--limit 10]` |
| Decision FTS with highlights | `engrams decision search "<term>" --snippets --limit 10 [--all]` |
| Single item by ID | `engrams decision get <id>` / `engrams pattern get <id>` / `engrams progress get <id>` / `engrams custom get <key>` |
| List items | `engrams decision list [--tags a,b]` / `engrams pattern list [--tags a,b]` / `engrams progress list` / `engrams active-context list` |
| Recent activity digest | `engrams activity` (all recent modifications across types) |
| Context revision history | `engrams history <product-context|active-context> [--name <track>] [--version <n>] [--limit 50] [--all]` |
| Health audit | `engrams doctor` (anchors, links, drift, orphans, rebuild advice, cycles, vocab) |
| Structured report by topic | `engrams report <context|progress|decisions|patterns|links>` |
| HTML dashboard | `engrams report open [--no-browser] [--out <path>]` (knowledge-graph visualization) |
| Plain onboarding text | `engrams instructions` (raw markdown agent prompt) |

### Graph queries (`engrams graph ...`)

| Goal | Command |
|---|---|
| Topology stats | `engrams graph stats` (nodes, edges, density, components, orphans, degree) |
| Most central items | `engrams graph central` (PageRank) |
| Connected clusters | `engrams graph clusters` |
| Orphan nodes | `engrams graph orphans` (degree ≤ 1) |
| Neighbors within N hops | `engrams graph neighbors --node decision:7 --depth 2 [--rel <type>]` |
| Shortest path A→B | `engrams graph path --from decision:7 --to system-pattern:2` |
| Transitive closure ("what breaks if I revisit X?") | `engrams graph chain --node decision:7 --rel depends_on` |

`graph chain` only accepts the four transitive canonical rels: `supersedes`, `depends_on`, `part_of`, `refines`. Nodes are addressed as `type:id` (e.g. `decision:7`).


## Policy Engine — Checkable Patterns & Rule Export

Patterns can carry a machine-checkable expression that turns engrams into a policy engine for LLM-agent guidance. A pattern with a check is exportable as a harness rule file (omp TTSR) and runnable as a local CI check.

### Making a pattern checkable

| Goal | Command |
|---|---|
| Regex-checked pattern | `engrams pattern log --name "..." --description "..." --check-kind regex --check '<regex>' --severity error` |
| AST-checked pattern | `engrams pattern log --name "..." --description "..." --check-kind ast --check '<ast-grep pattern>' --severity warn` |

`--check-kind` is `regex` or `ast`; `--check` is the expression source; `--severity` is `info`, `warn`, or `error` (default `warn`). Regexes are compiled at write time — invalid regex is rejected before any row is inserted. Prose-only patterns (no `--check-kind`) are unaffected and never produce violations.

### Exporting rules to a harness

| Goal | Command |
|---|---|
| Export to workspace `.omp/rules/` | `engrams install --harness omp` (writes rule files + manifest + guidance) |
| Export to a specific dir | `engrams rules export --harness omp --out <DIR>` |

Each checkable pattern becomes `engrams-<slug>.md` with omp frontmatter: `name`, `description`, `condition` (regex array) or `astCondition` (ast-grep), `scope` (derived from anchors: `tool:edit(glob), tool:write(glob)`), `interruptMode` (severity→interrupt: `error`→`always`, `warn`→`never`, `info`→rulebook-only), `alwaysApply`. Prose-only patterns are skipped. A deterministic `.engrams-manifest.json` records every rule's `pattern_id`, `timestamp`, `check_kind`, `check_expr`, `severity`, and `sha256`, so `doctor` can detect drift. Re-exporting is byte-identical.

### Running checks locally (CI / session-end)

| Goal | Command |
|---|---|
| Check full workspace | `engrams check` |
| Check staged files only | `engrams check --staged` |
| Check specific paths | `engrams check --paths src/ops,src/main.rs` |

Scans files against all checkable patterns. Regex checks run in-process; AST checks shell out to `sg` (ast-grep) if present, otherwise skipped with a note. Output is JSON `{checks, files_checked, violations: [{pattern, pattern_id, file, line, severity, message}]}`. Exits 1 when violations are found, 0 otherwise. Patterns only fire within their anchor scope (no anchor = all files).

### Doctor: rule staleness

`engrams doctor` includes a `rules` key that compares the on-disk manifest against the current DB. If a pattern was modified after export, `stale` is `true` with `drifted` listing the affected patterns. New checkable patterns not yet exported appear under `unexported`. This is advisory — never affects the DB-integrity `ok` flag.

### Write-through

After `pattern log` or `pattern delete`, if `.omp/rules/.engrams-manifest.json` already exists (i.e. rules were previously installed), the rulebook is regenerated automatically so generated files never lag the database. No manifest = no write (opt-in by prior `install`/`export`).
---

## Relationship Ontology

Canonical `--rel` values and their rules (defined in `src/ops/graph/rel.rs`):

| Relationship | Symmetry | Transitive | Notes |
|---|---|---|---|
| `relates_to` | symmetric | — | generic link; any→any |
| `depends_on` | directed | ✓ | has `conflicts_with` disjoint sibling |
| `part_of` | — | ✓ | hierarchical composition |
| `refines` | — | ✓ | one item sharpens another |
| `supersedes` | directed | ✓ | functional: a decision has at most one successor |
| `implements` | directed | — | e.g. pattern implements decision |
| `implemented_in` | directed | — | range: `pr`/`commit` |
| `conflicts_with` | symmetric | — | mutually exclusive |
| `co_changes` | symmetric | — | derived from git co-change history |
| `anchored_to` | directed | — | derived from file anchors |

Declared **inverse** edges (e.g. `superseded_by`, `depended_on_by`) are materialized automatically by `graph rebuild`. Non-canonical rel labels you invent are allowed, treated as symmetric for analytics, and surfaced by `engrams doctor` for vocabulary review.

## Status Vocabularies

| Item | Valid status |
|---|---|
| Decision | `active`, `superseded`, `rejected`, `revisited` |
| Progress | `Todo`, `InProgress`, `InReview`, `Blocked`, `Done`, `Dropped` |

Legacy/misspelled values are normalized case-insensitively during `migrate` (schema v4). Unrecognized values are preserved as-is and flagged by `doctor`.
