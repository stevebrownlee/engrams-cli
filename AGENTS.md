# Engrams CLI Tool

## Purpose

Maintain long-term context about the project so valuable tokens aren't wasted between separate LLM conversations. It persists architectural decisions, recurring patterns, task progress, named workstream tracks, a linked knowledge graph, and any custom data the developer wants. All output is **JSON**.

Everything below assumes the `engrams` binary is already built (`./target/debug/engrams`); the shorthand `engrams` in examples stands for that path or `cargo run --bin engrams --`.

---

## Memory & Project Context — The Operating Loop

> **Mandatory rules — read [`.agents/rules/`](.agents/rules/) first.** `001-memory.md` is a session invariant applied before any other step: run `engrams prime` before anything else. `007-relevant-before-edit.md` is a pre-edit gate: run `engrams relevant <paths>` before modifying source files. Rules `002`–`006` are on-demand — read a rule only when its scope matches the work at hand (e.g. `002`–`006` govern Rust edits under `src/`); do **not** read the whole directory up front.

Treat engrams as an **active advisor**: consult it before acting, log decisions as you make them, and check code against registered patterns before committing.

### When to Consult Engrams

- **Session start:** `engrams prime [--budget <n>] [--paths p1,p2] [--tags a,b]` — load context.
- **Before editing files:** `engrams advise <paths>` — get only actionable constraints and violations for those files (compact, machine-readable; `--staged` for `git add`ed). For full context with scores, use `engrams relevant <paths>`.
- **Before designing or fixing:** `engrams query "<topic>"` · `engrams decision search "<term>" --snippets` — find prior decisions and patterns so you don't re-litigate settled choices.
- **When you make a design choice:** `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> [--pr <n>]` — log immediately, not at session end. A contradiction gate blocks near-duplicate active decisions and suggests `supersedes`/`conflicts_with`; resolve inline with `--supersedes <id>` / `--conflicts-with <id>` (or `--force` to bypass). To supersede after the fact: `engrams decision supersede <old-id> --by <new-id>`.
- **When you spot a recurring convention:** `engrams pattern log --name "..." --check-kind regex --check '<expr>' --severity error --anchor src/ops` — make it machine-enforceable, not just prose.
- **Before committing:** `engrams check --staged` — scan staged files for violations against registered patterns (exits 1 on violations). Use `--paths src/ops` for specific paths.
- **Install enforceable rules for omp sessions:** `engrams install --harness omp` (writes `.omp/rules/`). Add `--hooks` to also install a git pre-commit hook running `engrams check --staged`.

### Before Editing Source Files — Mandatory

Run `engrams advise <paths>` (or `--staged`) for the files you are about to edit, **before** editing them. This returns only actionable constraints — checkable patterns and decisions anchored to those files — plus any current violations from `engrams check`. When constraints are empty, proceed. For full context (scores, reinforcement, progress), use `engrams relevant <paths>` instead. Skipping this risks duplicating conventions or violating constraints a previous session already decided. Run once per batch of related edits.

### Other Commands

| Goal | Command |
|---|---|
| Relate items (graph) | `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical> [--description "..."]` |
| Log task progress | `engrams progress log --status <Status> --description "..."` |
| Hand off context | `engrams active-context update --patch '<json>'` (merge) · `--content` (replace) |
| List patterns | `engrams pattern list [--tags a,b]` |
| Attach file anchors | `engrams anchor add --type <decision|progress-entry|system-pattern> --id <n> --path <path>` |
| Attach PR reference | `engrams pr add --type <decision|system-pattern> --id <n> --pr <n_or_url>` |
| Promote repeated progress into patterns | `engrams consolidate [--apply] [--min-repeats <n>] [--min-days <n>]` — propose by default; `--apply` inserts with evidence links + confidence; re-runs confirm |
| Causal chain ("why"/impact) | `engrams graph why --node decision:7 [--down]` — transitive walk over `causes` |
| Bulk operations | `engrams batch --type <decision|progress|pattern|custom-data> --items <json_or_->` |
| Export to Markdown | `engrams export [--path <dir>]` |
| Export rules (no install) | `engrams rules export --harness omp [--out <DIR>]` |
| Health check | `engrams doctor` |
| Schema migration | `engrams migrate` |

Notes: `--status` ∈ `Todo, InProgress, InReview, Blocked, Done, Dropped`. Valid `--rel` values: see [Relationship Ontology](docs/memory/engrams.md#relationship-ontology); pass `--force` to record an intentional constraint override (disjointness, cardinality, domain/range). Add `--compact` to any command, `--fields <list>` to project specific keys.

### Core Rules for Agent Memory

- **CLI-First:** query all history/context through the `engrams` CLI (`decision search`, `pattern list`, `prime`, `relevant`, `query`, `graph ...`). Never improvise.
- **Don't read `engrams_export/`:** for human Git-tracking only — token-inefficient and misses database-only state (links, graph edges, history). The CLI is the source of truth.
- **Accuracy discipline:** verify any command/flag/output you describe against the live CLI (`--help`) or source. Decision logs describe intent, not implementation.
- **Entity types use hyphens:** `decision`, `progress-entry`, `system-pattern`, `custom-data` (underscores rejected).
- **Session End Protocol:** before declaring done, run: (1) `decision log` each design choice → (2) `link add` new items to related ones (`implements`/`depends_on`/`supersedes`) → (3) `progress log --status Done` → (4) `active-context update --patch '<json>'` → (5) `engrams export` → (6) commit & push `engrams_export/`.
- **TTS Vocalization:** on "Talk to me" / "What should I work on?" / "What did we get done yesterday?" (or similar recent-work prompts): query active context + progress from the past 48h (filter `timestamp`), shape it to the phrasing (plain for "Talk to me"; verbose/technical for "Explain {x}"), synthesize via `tts`, play with `afplay`, confirm.

---

## Project Overview & Tech Stack
- **CLI Language:** Rust (2021 edition)
- **Database:** SQLite (embedded via `rusqlite` with the `bundled` feature, including FTS5)
- **Documentation Site:** Astro (located in the `/docs` directory)
- **Packaging:** Homebrew formula (`Formula/engrams.rb`) and an installer script (`docs/public/install`)

---

## Codebase Directory Map
- `src/main.rs`: Entry point and initialization of DB connection
- `src/cli.rs`: Clap command-line parser and command definitions
- `src/db.rs`: Database connection handling, workspace discovery, and migrations
- `src/schema.rs`: SQLite schema definitions and FTS5 triggers
- `src/models.rs`: Shared data models (e.g. link `Direction` enum)
- `src/ops/`: Subcommand handlers, split by feature
- `src/ops/rules/`: Policy engine — checkable pattern export to omp rule files + manifest
- `src/ops/check.rs`: Policy engine — local check runner (regex + ast-grep shell-out)
- `src/ops/install.rs`: Policy engine — `install --harness omp` orchestration
- `src/ops/graph/`: Knowledge + code graph (model, rebuild, relationship ontology)
- `tests/cli.rs`: End-to-end integration tests for CLI commands · `tests/policy.rs`: Policy engine acceptance tests (S1–S10)
- `docs/`: Website documentation source

---

## Database Discovery & Workspace Resolution
`engrams` searches upwards from the current working directory for the closest workspace root that contains an `engrams` sub-directory, and stores its database in `<workspace-root>/engrams/context.db`.

Override discovery by passing global flags **before** the subcommand:
- `--workspace <PATH>`: Force workspace directory
- `--db <PATH>`: Force exact database path

---

## Developer Commands
- **Build:** `cargo build`
- **Run local CLI:** `cargo run --bin engrams -- <COMMAND>` (or `./target/debug/engrams <COMMAND>` after building)
- **Format:** `cargo fmt`
- **Lint:** `cargo clippy --all-targets`
- **Test:** `cargo test`

---

For the full command reference — Store/Link/Retrieve tables, `graph` queries, the relationship ontology, and status vocabularies — read [`docs/memory/engrams.md`](docs/memory/engrams.md).
