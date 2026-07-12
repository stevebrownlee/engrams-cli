# Engrams CLI Tool

## Purpose

To maintain long-term context about the project so that valuable tokens aren't wasted between separate conversations with an LLM agent. It stored project patterns, architectural decisions, active context with progress, a linked knowledge graph, and any custom data the developer wants.

## Memory & Project Context (engrams)
You have access to the `engrams` CLI tool, which maintains a local SQLite database of project decisions, conventions, and progress.

1. **On Startup:** Run `engrams activity` to see what has changed recently. Get the `product-context` and `active-context` to orient yourself.
2. **Before Implementing:** Search `engrams decision search "<topic>"` and `engrams pattern list` to make sure your approach aligns with established decisions and codebase conventions.
3. **When making design choices:** Log them with `engrams decision log` so they are documented for future sessions.
4. **On Task Progress:** Track your progress using `engrams progress log`.
5. **On Exit:** Update the `active-context` to summarize where you left off for the next agent/developer.

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
- `--format <human|json>`: Force output format (defaults to `json` for easy machine parsing)

---

## Developer Commands
- **Build:** `cargo build`
- **Run local CLI:** `cargo run --bin engrams -- <COMMAND>` (or `./target/debug/engrams <COMMAND>` after building)
- **Format:** `cargo fmt`
- **Lint:** `cargo clippy --all-targets`
- **Test:** `cargo test`
