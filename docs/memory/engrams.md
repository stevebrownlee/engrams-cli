# Engrams CLI — Full Command Reference

Detailed reference for the `engrams` knowledge base. For the every-session operating loop and behavioral rules, see `AGENTS.md`. Everything below assumes the `engrams` binary is already built (`./target/debug/engrams`); the shorthand `engrams` in examples stands for that path or `cargo run --bin engrams --`. All output is **JSON**.

---

## Quick Reference: Store / Link / Retrieve

The three things you do with engrams: put facts **in**, wire them **together**, and pull them **back out**. Global flags available on most commands: `--db <path>` and `--workspace <path>` (before the subcommand), plus `--compact` and `--fields <a,b>` (on each subcommand).

### Store

| Goal | Command |
|---|---|
| Architectural decision | `engrams decision log --summary "..." --rationale "..." --details "..." --tags a,b --anchor src/foo.rs --pr 42 --status active --importance 8` |
| Recurring pattern/convention | `engrams pattern log --name "..." --description "..." --tags a,b --anchor src/foo.rs --pr 42 --importance 7` |
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
| Pre-edit constraints + violations | `engrams advise <paths...>` or `engrams advise --staged` (compact: only patterns + decisions + current violations; no scores/progress/reinforcement) |
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
| Causal chain ("why did this happen?") | `engrams graph why --node decision:7` (upstream over `causes`; `--down` for downstream impact) |

`graph chain` only accepts the five transitive canonical rels: `supersedes`, `depends_on`, `part_of`, `refines`, `causes`. Nodes are addressed as `type:id` (e.g. `decision:7`).


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
| Export to workspace `.omp/rules/` | `engrams install --harness omp` (writes rule files + manifest + guidance). Add `--hooks` to also install a git pre-commit hook running `engrams check --staged` |
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
| `causes` | directed | ✓ | causal chain; `caused_by` normalizes to it (swapped); feeds `graph why` |
| `derived_from` | directed | — | domain: `system_pattern` → range: `progress_entry`; consolidation evidence edges; `derives` normalizes to it (swapped) |

Declared **inverse** edges (e.g. `superseded_by`, `depended_on_by`) are materialized automatically by `graph rebuild`. Non-canonical rel labels you invent are allowed, treated as symmetric for analytics, and surfaced by `engrams doctor` for vocabulary review.

## Status Vocabularies

| Item | Valid status |
|---|---|
| Decision | `active`, `superseded`, `rejected`, `revisited` |
| Progress | `Todo`, `InProgress`, `InReview`, `Blocked`, `Done`, `Dropped` |

Legacy/misspelled values are normalized case-insensitively during `migrate` (schema v4). Unrecognized values are preserved as-is and flagged by `doctor`.


---

## Retrieval Scoring & Memory Decay (v0.10.0)

Three tier-1 agent-memory features from the arXiv survey roadmap (decision #53): blended retrieval scoring, prune-decay archiving, and read-path observability.

### Retrieval Scoring

Every `prime`, `relevant`, and `query` result is ranked by a blended score combining recency-decay (Ebbinghaus exponential over age) with normalized importance:

```text
score = W_RECENCY * exp(-LAMBDA * age_days) + W_IMPORTANCE * (importance / 10)
```

Full-text `query` results fold in an FTS5 BM25 term. Set an item's importance (0–10, default 5) with `--importance` on `decision log` / `pattern log`, or update it with `decision update <id> --importance <n>`. The score appears in JSON output as `score`.

### Reinforce-on-Read

Reading is learning. Every time `prime`, `relevant`, or `query` surfaces a record, its `access_count` is bumped and `last_accessed_at` is updated. This feeds prune strength and observability — records that are never surfaced decay faster and are flagged by `doctor`.

### Prune-Decay

```text
engrams prune                    # archive decayed records (actual)
engrams prune --dry-run          # preview what would be archived
engrams prune --threshold 0.05   # custom retention threshold (default: 0.1)
```

Archives decisions and patterns whose Ebbinghaus retention `exp(-age_days / strength)` has decayed below the threshold. Strength = `(importance + access_count) * 30 days` — important and frequently-read records survive longer. Archived records (`archived = 1`) are excluded from `prime`, `relevant`, and `query` by default; use `--all` on `relevant` or `query` to include them (`prime` always excludes archived items).

### Read Observability

`engrams doctor` now reports:

- **`never_read`**: records where `access_count = 0` (written but never surfaced by a read path). Advisory — identifies decisions that exist but may not be influencing retrieval.
- **`archived`**: count of archived records per table (`decisions` / `system_patterns`).

### Pre-Edit Advisory (`engrams advise`)

Purpose-built pre-edit command returning only actionable constraints and current violations for the given paths:

```text
engrams advise src/ops/scoring.rs    # constraints + violations for one file
engrams advise --staged              # for all git-staged files
```

Returns `{"constraints": [...], "violations": [...]}`. Constraints are decisions and checkable patterns anchored to those paths (patterns first, they're enforceable). Violations come from `engrams check`. No scores, no progress, no reinforcement-on-read — compact and fast for automatic harness injection. When both arrays are empty, proceed with no constraints.

Distinct from `relevant`, which returns full structs with scores and reinforces-on-read — use `relevant` for *understanding* context, `advise` for *checking constraints before editing*.

### Git Pre-Commit Hooks (`engrams install --hooks`)

```text
engrams install --harness omp --hooks   # write rule files + pre-commit hook
```

Installs `.git/hooks/pre-commit` which runs `engrams check --staged` before each commit. Violations at `error` severity block the commit (exit 1); `warn`/`info` violations are printed but don't block. Bypass with `git commit --no-verify`. The hook finds the engrams binary automatically by resolving the workspace root; delete the file to uninstall.

## Consolidation, Contradiction Gate & Causal Retrieval (v0.11.0)

Three tier-2 agent-memory features (spec `specs/agent-memory-0.11.0.md`, decision #53 roadmap): progress-entry consolidation into candidate patterns, a contradiction gate on `decision log`, and causal-chain retrieval.

### Consolidation (`engrams consolidate`)

```text
engrams consolidate                   # propose only — reports candidates, writes nothing new
engrams consolidate --apply           # insert candidate patterns (with evidence links + confidence)
engrams consolidate --min-repeats 4   # minimum distinct evidence entries per cluster (default: 3)
engrams consolidate --min-days 3      # minimum distinct calendar days spanned (default: 2)
```

Clusters `Done` progress entries that share anchor paths and span enough distinct days into a **candidate** `system_pattern` named `consolidated-<path-stem>`, tagged `consolidated`, with `initial_confidence = min(1.0, 0.5 + 0.15 × (n − min_repeats))` where `n` is the cluster's evidence count. `--apply` inserts the pattern plus:

- `derived_from` evidence links (pattern → each progress entry) — the provenance graph
- the shared anchor paths on the new pattern

Re-running `consolidate` **confirms** existing consolidated patterns: new evidence on a pattern's anchors logged after its confirm anchor attaches fresh `derived_from` links and bumps `last_confirmed_at`. Confirmations run in both propose and `--apply` mode; re-runs are idempotent.

The proposal also reports `merge_suggestions`: near-duplicate decision pairs (shared tag/anchor prefilter + FTS similarity). Suggestions are never auto-merged — resolve with `decision supersede <id> --by <id>` or `--conflicts-with`.

### Pattern confidence & read-time decay

Every pattern carries `confidence` (stored, `(0, 1]`, default 1.0) and `last_confirmed_at` (NULL until first confirmation). `pattern update <id> --confidence 0.65` sets the stored value; `--confirm` stamps `last_confirmed_at` (both are returned by `pattern list`/`get` alongside the decayed value):

```text
effective_confidence = confidence × exp(-LAMBDA × days_since(coalesce(last_confirmed_at, timestamp)))
```

Decay reuses the v0.10.0 recency `LAMBDA` (60-day half-life) and is computed **at read time** — no background jobs. `prime` and `relevant` multiply pattern ranking scores by `effective_confidence`, so recently confirmed patterns outrank stale ones with equal recency/importance.

`engrams doctor` reports `unconfirmed_patterns`: consolidated patterns (with `derived_from` evidence) never confirmed or unconfirmed for more than 180 days.

### Contradiction gate (`decision log`)

`decision log` without `--force` blocks near-duplicate inserts (`inserted: false`) and classifies each similar **active** decision as a suggested resolution:

- `supersedes` — the similar decision should give way to the new one
- `conflicts_with` — both can stay; link them as mutually exclusive

Classification prefers `supersedes` when the similar decision has no anchors, or shares an anchor with the new decision (`--anchor`), or shares a tag; `conflicts_with` otherwise. Superseded/archived near-duplicates are excluded from the gate.

Resolve inline, skipping the gate:

```text
engrams decision log --summary "..." --supersedes 7      # insert + supersedes link; decision 7 flips to superseded
engrams decision log --summary "..." --conflicts-with 7  # insert + conflicts_with link; both stay active
```

Both accept repeats (`--supersedes 7 --supersedes 9`). Targets are validated up front; missing ids fail before any write.

### Causal retrieval (`graph why`)

```text
engrams graph why --node decision:7            # upstream: "why did this happen" (walk causes backwards)
engrams graph why --node decision:7 --down     # downstream: "what does this affect"
```

Transitive walk over `causes` (use `caused_by` in `link add`; it normalizes to canonical `causes` with source/target swapped). Chain entries carry `depth`, `node`, and `via_edge_description` (the `--description` from the parent edge, when present). `roots` lists the chain's origin nodes — upstream roots are the ultimate causes, downstream roots the furthest impacts.

Anchors can now attach to progress entries too: `engrams anchor add --type progress-entry --id <n> --path src/foo.rs` (required for consolidation clustering).

## Strategy-First Retrieval & Telemetry (v0.12.0)

The v0.12.0 release eliminates the 5,000–15,000 token "file-reading tax" by making the knowledge graph self-sufficient for strategy formation, expanding search recall across naming conventions, and introducing automated usage telemetry and PR gates.

### Decision contracts (`--contract`)

```text
engrams decision log --summary "..." --contract "fn connect() -> Result<Conn>" --anchor src/db.rs
```

Decisions can declare an explicit interface contract (`contract` column, Schema v10) capturing signatures, struct shapes, and error tuples. When a decision introduces or modifies an abstraction, Engrams prompts with a write-time nudge if `--contract` is omitted.

### One-call architectural brief (`engrams brief`)

```text
engrams brief <node|query> [--depth 1..3]
```

Replaces 5 separate CLI calls (`decision get` + `anchor list` + `pr list` + `link list` + `graph neighbors`) with a single token-lean composite read:

- Full decision summary and contract
- PR references and tag list
- File anchors with extracted symbols, module docstrings, and line counts
- Git staleness drift (`stale: true/false`)
- Connected 1-hop neighbors with summaries

### Cross-convention search & filtering

FTS queries automatically expand compound tokens to `("phrase" OR concat*)` — searching for `system_user_id` matches `systemUserId`, `system-user-id`, and `system_user_id`. Empty result sets return structured `miss_guidance` (top tags, per-token hits, recent decisions, graph hubs) to guide agents without table dumps. Fast server-side filtering is supported via `engrams decision list --filter <text>`.

### Usage telemetry & curation loop (`engrams usage`)

```text
engrams usage [--since <timestamp|2w>] [--daily]
engrams usage --misses
```

Every retrieval call (`query`, `relevant`, `advise`, `brief`) is logged to `usage_log`. `--misses` ranks zero-hit searches, turning vocabulary gaps into actionable curation targets.

### Knowledge coverage (`engrams coverage`)

```text
engrams coverage src/
engrams coverage --diff main...HEAD
```

Audits the percentage of files with live anchored knowledge, flags dead file anchors, and measures median hop distance across the graph.

### Session closure & PR validation (`engrams session close`)

```text
engrams session close --reads-skipped 14 --reads-required 2 --tokens-saved 70000 --pr 42
engrams session history
```

Records reads skipped vs required and token ROI rollups. When `--pr` is provided, Engrams validates that the PR node is linked to at least one decision and connected to anchored code files before blessing the session.

### PR reverse lookup (`engrams pr find`)

```text
engrams pr find 42
```

Finds all decisions, patterns, or progress entries that reference a PR number or URL.