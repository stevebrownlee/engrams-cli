# Engrams CLI Tool

## Purpose

To maintain long-term context about the project so that valuable tokens aren't wasted between separate conversations with an LLM agent. It stored project patterns, architectural decisions, active context with progress, a linked knowledge graph, and any custom data the developer wants.

## Memory & Project Context (engrams)
This project uses the `engrams` CLI (a local SQLite knowledge base) to persist decisions, conventions, and progress between sessions. All output is JSON.

1. **On startup:** run `engrams prime` (add `--budget <tokens>` to cap output) for a one-call briefing: product context, active context, recent decisions, patterns, and progress.
2. **Before editing files:** run `engrams relevant <paths>` (or `engrams relevant --staged`) to fetch only the decisions and patterns anchored to the files you are about to touch.
3. **Before implementing:** search prior art with `engrams query "<topic>"`.
4. **When you make a design choice:** log it: `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> --pr <number-or-url>`.
5. **When a decision is replaced:** `engrams decision supersede <old-id> --by <new-id>`.
6. **On task progress:** `engrams progress log --status <status, e.g. InProgress or Done> --description "..."` (status is a free-form string).
7. **Before ending the session:** update the hand-off document: `engrams active-context update --content '<json>'`.

Use `--compact` on any command to minimize tokens. Run `engrams doctor` periodically to find stale or unanchored knowledge.

### Core Rules for Agent Memory
- **CLI-First Querying:** ALWAYS use the `engrams` CLI tool (e.g., `engrams decision search`, `engrams pattern list`, etc.) to query project history and context.
- **DO NOT read or grep exported files:** The files under `engrams_export/` are for human Git-tracking only. Reading/parsing them directly via `read` or `grep` is highly token-inefficient and prone to missing database-only state.
- **Run local builds:** Prioritize executing the compiled local binary (e.g., `./target/debug/engrams`) to query/write context directly.

---

## Project Overview & Tech Stack
- **CLI Language:** Rust (2021 edition)
- **Database:** SQLite (embedded via `rusqlite` with the `bundled` feature, including FTS5)
- **Documentation Site:** Astro (located in the `/docs` directory)
- **Packaging:** Homebrew formula (`Formula/engrams.rb`) and an installer script (`docs/public/install`)

---

## Codebase Directory Map
- `src/main.rs`: Entry point and initialization of DB connection
- `src/cli.rs`: Clap command-line parser and definitions
- `src/db.rs`: Database connection handling, schema definition, and migrations
- `src/schema.rs`: SQLite schema definitions and FTS5 triggers
- `src/ops/`: Subcommand implementation handlers (split by feature)
- `tests/cli.rs`: End-to-end integration tests for CLI commands
- `docs/`: Website documentation source

---

## Database Discovery & Workspace Resolution
`engrams` searches upwards from the current working directory for the closest workspace root (containing `.engrams`, `engrams/context.db`, `.git`, `Cargo.toml`, etc.). It stores its database in `<workspace-root>/engrams/context.db`.

You can override this discovery by passing global flags **before** the subcommand:
- `--workspace <PATH>`: Force workspace directory
- `--db <PATH>`: Force exact database path

---

## Developer Commands
- **Build:** `cargo build`
- **Run local CLI:** `cargo run --bin engrams -- <COMMAND>` (or `./target/debug/engrams <COMMAND>` after building)
- **Format:** `cargo fmt`
- **Lint:** `cargo clippy --all-targets`
- **Test:** `cargo test`
