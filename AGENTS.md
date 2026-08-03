# Engrams CLI Tool

## Purpose

Maintain long-term context about the project so valuable tokens aren't wasted between separate LLM conversations. It persists architectural decisions, recurring patterns, task progress, named workstream tracks, a linked knowledge graph, and any custom data the developer wants. All output is **JSON**.

Everything below assumes the `engrams` binary is already built (`./target/debug/engrams`); the shorthand `engrams` in examples stands for that path or `cargo run --bin engrams --`.

---

## Memory & Project Context — The Operating Loop

Run this loop in every session. It is cheap (local SQLite), keeps you grounded, and leaves the knowledge base richer than you found it.

1. **Start of session — get oriented:** run `engrams prime` (add `--budget <tokens>` to cap output). This one call returns the product context, the active-context **track** matching your scope (plus a one-line focus for every other track), recent decisions, patterns, progress, and a compact graph summary. Add `--paths <p1,p2>` or `--tags <a,b>` to scope it.
2. **Before editing files — fetch what's anchored there:** run `engrams relevant <paths>` (or `engrams relevant --staged` to match your `git add`ed files). Returns only the decisions and patterns anchored to those paths.
3. **Before implementing — search prior art:** `engrams query "<topic>"` searches across decisions, patterns, and custom data; `engrams decision search "<term>" --snippets` gives FTS-highlighted hits.
4. **When you make a design choice — log it:** `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> --pr <number-or-url>`. Anchor every decision to the file(s) it governs so `relevant` and the graph can find it.
5. **When a decision replaces another:** `engrams decision supersede <old-id> --by <new-id>` — marks the old one superseded and records its successor.
6. **Relate items into the graph:** `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical> --description "..."`. See [Relationship Ontology](docs/memory/engrams.md#relationship-ontology). Pass `--force` to record an intentional override when a link violates a constraint (disjointness, functional-cardinality, domain/range).
7. **On task progress:** `engrams progress log --status <Status> --description "..."` — status must be one of `Todo, InProgress, InReview, Blocked, Done, Dropped` (free-form strings are normalized; unrecognized values are preserved but flagged by `doctor`).
8. **When creating a release or handing off:** update the hand-off document: `engrams active-context update --patch '<json>'` (merge) or `--content '<json>'` (replace), optionally `--name <track>` for a non-default workstream.
9. **Keep it healthy:** run `engrams doctor` periodically to surface broken anchors, dangling links, stale drift, orphan graph nodes, transitive cycles, and non-canonical vocabulary.
10. **On schema changes:** `engrams migrate` brings an old DB up to the latest version. The agent never edits schema files manually.

Add `--compact` to any command to minimize tokens. Add `--fields <comma-list>` to project only specific JSON keys.

### Core Rules for Agent Memory

- **CLI-First Querying:** ALWAYS use the `engrams` CLI to query project history and context (`engrams decision search`, `engrams pattern list`, `engrams prime`, `engrams relevant`, `engrams query`, `engrams graph ...`). Never improvise.
- **DO NOT read or grep the exported files:** the files under `engrams_export/` exist for **human Git-tracking only**. Reading/parsing them via `read` or `grep` is token-inefficient and misses database-only state (links, graph edges, history). The CLI is the single source of truth.
- **Accuracy discipline:** before describing any command, flag, check, or output shape in your own writing (docs, decisions, summaries), verify it against the live CLI (`--help`) or source. Do not synthesize behavior from memory of a decision log — decision logs describe intent, not necessarily implementation.
- **Entity-type spelling:** type values use **hyphens**, not underscores — `decision`, `progress-entry`, `system-pattern`, `custom-data`. Underscores are rejected.
- **Session End Protocol & Git Sync:** before concluding the session or declaring a task/effort complete, the agent MUST run the full update sequence:
  1. Log all architectural/design decisions made during the session with `engrams decision log`.
  2. Link newly logged decisions or patterns to any relevant existing items (e.g. a new decision `extends`, `uses`, or `supersedes` an older one) with `engrams link add`.
  3. Log final progress status as `Done`: `engrams progress log --status Done --description "..."`.
  4. Update the active context document: `engrams active-context update --patch '<json>'`.
  5. Export the database back to the workspace: `engrams export`.
  6. Stage, commit, and push the `engrams_export/` markdown files so remote Git tracking stays in sync.
- **TTS Active-Context Vocalization:** when the user starts a prompt with "Talk to me", or asks "What should I work on today?", "Where did we leave off yesterday?", "What did we get done yesterday?", or asks about recent/yesterday's accomplishments:
  1. Query active context and all progress items from the past 48 hours (filter by the `timestamp` field in `engrams` progress output) via the CLI.
  2. Shape the summary to the phrasing — plain language for "Talk to me"; more verbose/technical for "Explain {x} to me".
  3. Synthesize the summary to an audio file (e.g. `/tmp/status.wav`) with the `tts` tool.
  4. Play it back on macOS with `afplay`, and confirm to the user that it has been vocalized.

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
