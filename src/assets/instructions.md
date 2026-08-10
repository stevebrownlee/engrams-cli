## Engrams — Memory & Active Code Advisor

This project uses the `engrams` CLI (local SQLite) to persist decisions, patterns, progress, knowledge-graph links, and custom data between sessions. All output is **JSON**. Treat engrams as an **active advisor**: consult it before acting, log decisions as you make them, and check code against registered patterns before committing.

### When to Consult Engrams

- **Session start:** `engrams prime` — load context (mandatory before any other step).
- **Before editing files:** `engrams advise <paths>` — get only actionable constraints and violations for those files (compact; `--staged` for `git add`ed). For full context with scores, use `engrams relevant <paths>`.
- **Before designing or fixing:** `engrams query "<topic>"` · `engrams decision search "<term>" --snippets` — find prior decisions and patterns so you don't re-litigate settled choices.
- **When you make a design choice:** `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> --importance 8` — log immediately. Set importance (0–10) to influence retrieval ranking. To supersede: `engrams decision supersede <old-id> --by <new-id>`, then `engrams link add --rel supersedes` to connect them in the graph.
- **When you spot a recurring convention:** `engrams pattern log --name "..." --check-kind regex --check '<expr>' --severity error --anchor <path>` — make it machine-enforceable, not just prose.
- **Before committing:** `engrams check --staged` — scan staged files for violations against registered patterns (exits 1 on violations).
- **Install enforceable rules for omp sessions:** `engrams install --harness omp` (writes `.omp/rules/`). Add `--hooks` to also install a git pre-commit hook running `engrams check --staged`.

### Other Commands

| Goal | Command |
|---|---|
| Relate items (graph) | `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical>` |
| Log task progress | `engrams progress log --status <Status> --description "..."` |
| Hand off context | `engrams active-context update --patch '<json>'` (merge) · `--content` (replace) |
| List patterns | `engrams pattern list [--tags a,b]` |
| Attach file anchors | `engrams anchor add --type <decision|system-pattern> --id <n> --path <path>` |
| Attach PR reference | `engrams pr add --type <decision|system-pattern> --id <n> --pr <n_or_url>` |
| Bulk operations | `engrams batch --type <decision|progress|pattern|custom-data> --items <json_or_->` |
| Export to Markdown | `engrams export [--path <dir>]` |
| Export rules (no install) | `engrams rules export --harness omp [--out <DIR>]` |
| Health check | `engrams doctor` |
| Prune decayed records | `engrams prune [--dry-run] [--threshold 0.1]` |
| Schema migration | `engrams migrate` |

Notes: `--status` ∈ `Todo, InProgress, InReview, Blocked, Done, Dropped`. Valid `--rel`: `relates_to`, `depends_on`, `part_of`, `implements`, `refines`, `supersedes`, `conflicts_with` (custom labels allowed; ontology at `https://engrams.sh`). Add `--compact` to any command; `--fields <list>` to project specific keys.

### Core Rules

- **CLI-First:** query all history/context through the `engrams` CLI. Never read `engrams_export/` (Git-tracked human export — token-inefficient, misses database-only state). Never improvise.
- **Accuracy:** verify any command/flag against `--help`. Decision logs describe intent, not implementation.
- **Entity types use hyphens:** `decision`, `progress-entry`, `system-pattern`, `custom-data`.

### Session End Protocol

Before declaring done: (1) `decision log` each design choice → (2) `link add` new items (`implements`/`depends_on`/`supersedes`) → (3) `progress log --status Done` → (4) `active-context update --patch '<json>'` → (5) `engrams export` → (6) commit & push `engrams_export/`.

For the full reference (Store/Link/Retrieve tables, `graph` queries, relationship ontology, status vocabularies) see `https://engrams.sh`.
