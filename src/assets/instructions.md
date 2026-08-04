## Memory & Project Context (engrams)
This project uses the `engrams` CLI (a local SQLite knowledge base) to persist architectural decisions, recurring patterns, task progress, named active-context tracks, a linked knowledge graph, and any custom data between sessions. All output is **JSON**.

Run in every session (local SQLite — cheap):

| Goal | Command |
|---|---|
| Get oriented (session start) | `engrams prime [--budget <n>] [--paths p1,p2] [--tags a,b]` |
| Context anchored to files you'll edit | `engrams relevant <paths>` (`--staged` for `git add`ed files) |
| Search prior art | `engrams query "<topic>"` · `engrams decision search "<term>" --snippets` |
| Log a design choice | `engrams decision log --summary "..." --rationale "..." --tags a,b --anchor <path> [--pr <n>]` |
| Decision replaces another | `engrams decision supersede <old-id> --by <new-id>` |
| Relate items in the graph | `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical> [--description "..."]` |
| Log task progress | `engrams progress log --status <Status> --description "..."` |
| Release / hand-off | `engrams active-context update --patch '<json>'` (merge) · `--content` (replace) |
| Health check | `engrams doctor` |
| Schema migration | `engrams migrate` |

Notes: `--status` ∈ `Todo, InProgress, InReview, Blocked, Done, Dropped`. Valid `--rel`: `relates_to`, `depends_on`, `part_of`, `implements`, `refines`, `supersedes`, `conflicts_with` (custom labels allowed; full ontology at `https://engrams.sh`); pass `--force` on `link add` to record an intentional constraint override (disjointness, cardinality, domain/range). Add `--compact` to any command; `--fields <list>` to project specific keys.

### Core Rules for Agent Memory

- **CLI-First:** query all history/context through the `engrams` CLI (`decision search`, `pattern list`, `prime`, `relevant`, `query`, `graph ...`). Never improvise.
- **Don't read `engrams_export/`:** for human Git-tracking only — token-inefficient and misses database-only state (links, graph edges, history). The CLI is the source of truth.
- **Accuracy discipline:** verify any command/flag/output you describe against the live CLI (`--help`). Decision logs describe intent, not implementation.
- **Entity types use hyphens:** `decision`, `progress-entry`, `system-pattern`, `custom-data`.
- **Session End Protocol:** before declaring done, run: (1) `decision log` each design choice → (2) `link add` new items to related ones (`implements`/`depends_on`/`supersedes`) → (3) `progress log --status Done` → (4) `active-context update --patch '<json>'` → (5) `engrams export` → (6) commit & push `engrams_export/`.

For the full command reference (Store/Link/Retrieve tables, `graph` queries, the relationship ontology, and status vocabularies) see `https://engrams.sh`.
