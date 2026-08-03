# Engrams CLI Tool

## Purpose

Maintain long-term context about the project so valuable tokens aren't wasted between separate LLM conversations. It persists architectural decisions, recurring patterns, task progress, named workstream tracks, a linked knowledge graph, and any custom data the developer wants. All output is **JSON**.

Everything below assumes the `engrams` binary is already built (`./target/debug/engrams`); the shorthand `engrams` in examples stands for that path or `cargo run --bin engrams --`.

---

## Memory & Project Context — The Operating Loop

Run in every session (local SQLite — cheap).

| Goal | Command |
|---|---|
| Get oriented (session start) | `engrams prime [--budget <n>] [--paths p1,p2] [--tags a,b]` |
| Context anchored to files you'll edit | `engrams relevant <paths>` · `--staged` for `git add`ed files |
| Search prior art | `engrams query "<topic>"` · `engrams decision search "<term>" --snippets` |
| Log a design choice | `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> [--pr <n>]` |
| Decision replaces another | `engrams decision supersede <old-id> --by <new-id>` |
| Relate items in the graph | `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical> [--description "..."]` |
| Log task progress | `engrams progress log --status <Status> --description "..."` |
| Release / hand-off | `engrams active-context update --patch '<json>'` (merge) · `--content` (replace) |
| Health check | `engrams doctor` |
| Schema migration | `engrams migrate` |

Notes: `--status` ∈ `Todo, InProgress, InReview, Blocked, Done, Dropped`. Valid `--rel` values: see [Relationship Ontology](docs/memory/engrams.md#relationship-ontology); pass `--force` to record an intentional constraint override (disjointness, cardinality, domain/range). Add `--compact` to any command, `--fields <list>` to project specific keys.

### Core Rules for Agent Memory

- **CLI-First:** query all history/context through the `engrams` CLI (`decision search`, `pattern list`, `prime`, `relevant`, `query`, `graph ...`). Never improvise.
- **Don't read `engrams_export/`:** for human Git-tracking only — token-inefficient and misses database-only state (links, graph edges, history). The CLI is the source of truth.
- **Accuracy discipline:** verify any command/flag/output you describe against the live CLI (`--help`) or source. Decision logs describe intent, not implementation.
- **Entity types use hyphens:** `decision`, `progress-entry`, `system-pattern`, `custom-data` (underscores rejected).
- **Session End Protocol:** before declaring done, run: (1) `decision log` each design choice → (2) `link add` new items to related ones (`extends`/`uses`/`supersedes`) → (3) `progress log --status Done` → (4) `active-context update --patch '<json>'` → (5) `engrams export` → (6) commit & push `engrams_export/`.
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
- `src/ops/graph/`: Knowledge + code graph (model, rebuild, relationship ontology)
- `tests/cli.rs`: End-to-end integration tests for CLI commands
- `docs/`: Website documentation source

---

## Database Discovery & Workspace Resolution
`engrams` searches upwards from the current working directory for the closest workspace root (containing `.engrams`, `engrams/context.db`, `.git`, `Cargo.toml`, etc.) and stores its database in `<workspace-root>/engrams/context.db`.

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

## UI & Content Verification
- **Verification Tool:** When self-verifying UI or content changes (e.g. updates to the `/docs` site) after implementation, LLM agents MUST use the `agent-browser` CLI tool.
- **No IDE Tooling:** DO NOT use the IDE's built-in browser or built-in UI verification tools.

---

For the full command reference — Store/Link/Retrieve tables, `graph` queries, the relationship ontology, and status vocabularies — read [`docs/memory/engrams.md`](docs/memory/engrams.md).
