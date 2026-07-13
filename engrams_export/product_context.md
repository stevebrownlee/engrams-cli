# Product Context

```json
{
  "content": {
    "command_structure": [
      "init",
      "product-context (get, update)",
      "active-context (get, update)",
      "history (doc, --version, --limit)",
      "decision (log, list, search, get, update)",
      "progress (log, list, update)",
      "pattern (log, list, search, get, update)",
      "custom (set, get, search)",
      "link (add, list, remove)",
      "activity",
      "batch (--type, --items)",
      "export (--path)",
      "import (--path)"
    ],
    "database_discovery": "Looks for closest workspace root (contains .engrams, engrams/context.db, .git, pyproject.toml, Cargo.toml, package.json, etc.) and stores database in <workspace>/engrams/context.db.",
    "name": "engrams-cli",
    "purpose": "A standalone, high-performance Rust CLI for managing contextual memory, architecture decisions, and system patterns without consuming LLM context windows with tool schemas.",
    "tech_stack": {
      "cli_parsing": "clap (v4, derive)",
      "database": "SQLite (rusqlite 0.32 with bundled sqlite3)",
      "datetime": "chrono v0.4",
      "language": "Rust (2021 edition)",
      "serialization": "serde (v1), serde_json",
      "uuid": "uuid v4"
    }
  },
  "updated_at": "2026-07-10T12:35:13Z",
  "version": 1
}
```
