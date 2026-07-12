# Engrams CLI

A standalone, high-performance Rust CLI for managing contextual memory, architecture decisions, and system patterns without consuming LLM context windows with tool schemas. It stores everything in a local, self-contained SQLite database with full-text search (FTS5) capabilities.

---

## Why Engrams?

As software systems grow in complexity, keeping track of architectural decisions, conventions, progress, and project constraints becomes a major challenge—especially when working with AI agents. 

### The Problem: LLM Context Bloat
Traditionally, developers feed AI agents context by pasting massive `README` files, design docs, or full-code directories. This leads to:
- **High Token Consumption:** Storing static context in prompts costs money and eats up the LLM's active window.
- **Attention Decay:** Long prompts degrade the agent's reasoning accuracy and lead to missed details.
- **Stale Context:** Project documentation easily drifts from actual implementation.

### The Solution: On-Demand Memory
`engrams` acts as a local database for your project's memory. Instead of cramming everything into the context window, the AI agent dynamically queries the SQLite database using **Full-Text Search (FTS5)** or **Knowledge Graph Links**. 

- **Saves 80%+ of Context Tokens:** Only pull relevant decisions or patterns when needed.
- **Agentic Native:** Structured JSON inputs and outputs designed for direct tool/agent consumption.
- **Git-Friendly:** Clean markdown export/import lets you track context changes inside git history without binary db bloat.

---

## Agentic Integration Workflow

You can supercharge your development by instructing your AI agents to use `engrams` directly. Here is a typical workflow and integration guide.

### 1. System Prompt Configuration
Add the following instructions to your `AGENTS.md` file or `CLAUDE.md`, or even a rule or skill in your tool of choice:

```markdown
## Memory & Project Context (engrams)
You have access to the `engrams` CLI tool, which maintains a local SQLite database of project decisions, conventions, and progress.

1. **On Startup:** Run `engrams activity` to see what has changed recently. Get the `product-context` and `active-context` to orient yourself.
2. **Before Implementing:** Search `engrams decision search "<topic>"` and `engrams pattern list` to make sure your approach aligns with established decisions and codebase conventions.
3. **When making design choices:** Log them with `engrams decision log` so they are documented for future sessions.
4. **On Task Progress:** Track your progress using `engrams progress log`.
5. **On Exit:** Update the `active-context` to summarize where you left off for the next agent/developer.
```
---

## Feature Set

- **Context (Product & Active):** Maintain a high-level overview of the product constraints and the current active sprint or task.
- **Architectural Decisions (ADRs):** Log and search design decisions with rationales, implementation details, and custom tags.
- **System Patterns:** Standardize codebase conventions (e.g., error handling, folder structure) and make them searchable.
- **Progress Tracking:** Maintain nested task lists to track what is complete and what's in progress.
- **Links (Knowledge Graph):** Establish typed relationships between decisions, patterns, progress, and custom data (e.g. decision A *implements* pattern B).
- **Activity Digest:** Fetch a reverse-chronological stream of modifications to quickly get up to speed.
- **Git-Sync (Export/Import):** Export database state to structured markdown files that can be committed to git, and import them back to reconstruct the SQLite database.

---

## Installation

### Via Installer Script (Linux & macOS)
You can download and install the latest pre-compiled binary automatically:
```bash
curl -fsSL https://engrams.sh/install | bash
```

### Via Homebrew (macOS)
You can install the CLI using Homebrew by tapping the official repository:
```bash
brew install stevebrownlee/engrams/engrams
```

### Building from Source
Requires a standard Rust toolchain.
```bash
cargo build --release
```
The binary will be located at `target/release/engrams`. You can copy it to your `PATH` or run it via `cargo run --bin engrams -- [ARGS]`.

---

## Workspace & Database Discovery

By default, `engrams` looks for the closest workspace root (indicated by `.engrams`, `engrams/context.db`, `.git`, `pyproject.toml`, `Cargo.toml`, etc.) and stores its database in `<workspace>/engrams/context.db`.

You can override this explicitly:
- `--workspace <PATH>`: Use the specified directory as the workspace root.
- `--db <PATH>`: Provide a direct path to the SQLite `.db` file.

---

## Output Format

All commands output clean JSON to `stdout` for programmatic consumption by AI agents or `jq`.

---

## Commands Reference

All commands automatically initialize the schema if the database does not exist.

### Initialization
```bash
engrams init
```
Forces database creation and reports the resolved database path.

### Schema Migration
```bash
engrams migrate
```
Applies any outstanding database schema migrations. The CLI checks the schema version on startup and will request this command if the database is out of date.

### Context (Product & Active)
Manages the core context documents (supports `product-context` and `active-context`).
```bash
# Get the current context
engrams product-context get

# Completely replace the context
engrams product-context update --content '{"stack": "rust", "database": "sqlite"}'

# Patch the context (merges keys, removes keys set to "__DELETE__")
engrams product-context update --patch '{"database": "PostgreSQL", "old_key": "__DELETE__"}'

# View history of the context
engrams history product-context --limit 10
```

### Decisions
Log and search architectural decisions.
```bash
# Log a new decision
engrams decision log \
  --summary "Migrate to standalone Rust CLI" \
  --rationale "Prevents context window bloat from tool schemas" \
  --tags "architecture,rust,cli"

# List decisions (optionally filter by tags)
engrams decision list --tags rust

# Search decisions using full-text search
engrams decision search "context window"

# Get a specific decision by ID
engrams decision get 1

# Update a decision
engrams decision update 1 --summary "Migrated to standalone Rust CLI"
```

### Progress
Track task execution and sub-tasks.
```bash
# Log progress
engrams progress log --status "InProgress" --description "Implementing Context commands"

# Log a sub-task (linked via parent-id)
engrams progress log --status "Done" --description "Schema defined" --parent-id 1

# List progress entries
engrams progress list
```

### System Patterns
Log recurring patterns and conventions to ensure codebase consistency. Patterns are upserted by their unique `--name`.
```bash
engrams pattern log --name "CLI args" --description "Use clap for parsing" --tags "cli,rust"
```

### Custom Data
Store arbitrary configuration or key-value pairs (upserted by `category` + `key`).
```bash
# Set a string value
engrams custom set --category "config" --key "api_host" --value "localhost:8080"

# Set a JSON value
engrams custom set --category "config" --key "retries" --value "3" --json

# Get and search
engrams custom get --category "config" --key "api_host"
engrams custom search "localhost"
```

### Links (Knowledge Graph)
Relate different items in the database to form a knowledge graph. Item types are: `decision`, `progress_entry`, `system_pattern`, `custom_data`.
```bash
# Link a progress entry (ID 2) to a decision (ID 1)
engrams link add \
  --source-type progress_entry --source-id 2 \
  --target-type decision --target-id 1 \
  --rel "implements"

# List all links for a specific decision
engrams link list --item-type decision --item-id 1
```

### Activity Digest
Get a recent summary of all modifications across the memory store.
```bash
# Get activity from the last 24 hours (default)
engrams activity

# Get activity from the last 48 hours, limited to 10 items per category
engrams activity --hours 48 --limit-per-type 10
```

### Batch Operations
Perform multiple operations in a single atomic transaction. Pass a JSON array of items matching the respective `log` or `set` arguments.
```bash
# Provide JSON via argument
engrams batch --type decision --items '[{"summary": "A"}, {"summary": "B", "tags": ["foo"]}]'

# Provide JSON via stdin
cat data.json | engrams batch --type custom_data --items -
```

### Export & Import
Dump the entire database to a hierarchy of Markdown files (with embedded JSON), making the store easily editable by hand or committable to Git, and import it back.
```bash
# Export to directory (defaults to ./engrams_export)
engrams export --path ./my_export

# Import from directory
engrams import --path ./my_export
```
