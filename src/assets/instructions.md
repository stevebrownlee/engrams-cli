## Engrams — Memory & Active Code Advisor

This project uses the `engrams` CLI (local SQLite) to persist decisions, patterns, progress, knowledge-graph links, and custom data between sessions. All output is **JSON**. Treat engrams as an **active advisor**: consult it before acting, log decisions as you make them, and check code against registered patterns before committing.

### Strategy-First Retrieval — before any file read

A KB query costs tens of tokens; a source-file read costs 5–15k. Climb this ladder in order and only fall through when the current rung fails:

1. `engrams prime` — session brief (top decisions, patterns, active context).
2. `engrams brief <node|query>` — one-call composite read: summary, contract, rationale, PRs, anchors, drift, and enriched 1..3-hop neighbors. This replaces "get the decision, then read its anchored file".
3. `engrams query <q> --full` / `engrams relevant <paths>` — FTS hits with embedded summaries and implementation details; no follow-up `get` calls. Empty result sets include `miss_guidance` (nearest tags, per-token hit counts, recent decisions, graph hubs) — re-target from it instead of dumping tables or guessing file names.
4. Only now open files — and open only the symbols the graph said matter.

Retrieve with convention-aware terms: `snake_case`, `kebab-case`, and `camelCase` variants of the same words all match (`decision list --filter` supports the same). Every retrieval is logged (`engrams usage`); zero-hit queries (`engrams usage --misses`) name vocabulary gaps worth closing with better anchors or tags rather than another file read.

### When to Consult Engrams

- **Session start:** `engrams prime [--budget <n>] [--paths p1,p2] [--tags a,b]` — load context.
- **Before editing files:** `engrams advise <paths>` — get only actionable constraints and violations for those files (compact; `--staged` for `git add`ed). For full context with scores, use `engrams relevant <paths>`.
- **Before designing or fixing:** `engrams brief "<topic>"` · `engrams query "<topic>" --full` · `engrams decision list --filter "<term>"` — composite context in one call; avoid litigating settled choices.
- **When you make a design choice:** `engrams decision log --summary "..." [--contract "..."] --rationale "..." --tags a,b --anchor <path> [--pr <n>]` — log immediately, not at session end. Always declare `--contract` (signatures, struct shapes) when introducing an abstraction. A contradiction gate blocks near-duplicate active decisions and suggests `supersedes`/`conflicts_with`; resolve inline with `--supersedes <id>` / `--conflicts-with <id>` (or `--force` to bypass). To supersede after the fact: `engrams decision supersede <old-id> --by <new-id>`.
- **When you spot a recurring convention:** `engrams pattern log --name "..." --check-kind regex --check '<expr>' --severity error --anchor <path>` — make it machine-enforceable, not just prose.
- **Before committing:** `engrams check --staged` — scan staged files for violations against registered patterns (exits 1 on violations).
- **Install enforceable rules for omp sessions:** `engrams install --harness omp` (writes `.omp/rules/`). Add `--hooks` to also install a git pre-commit hook running `engrams check --staged`.

### Other Commands

| Goal | Command |
|---|---|
| Composite read | `engrams brief <node\|query> [--depth 1..3]` — summary, contract, anchors, symbols, 1-hop neighbors |
| Usage telemetry | `engrams usage [--since <ts\|2w>] [--daily] [--misses]` — retrieval counts and zero-hit query ranking |
| Knowledge coverage | `engrams coverage <paths> [--diff <base>...HEAD]` — fraction anchored, dead anchors, median hops |
| Session ROI & PR gate | `engrams session close [--reads-skipped <n>] [--reads-required <n>] [--tokens-saved <n>] [--pr <n>]` |
| Session history | `engrams session history` — cumulative reads saved and recent close rollups |
| Decision curation | `engrams decision stats [--most-accessed] [--never-accessed] [--limit <n>]` |
| PR reverse lookup | `engrams pr find <number\|url>` — which decisions/patterns reference a PR |
| Relate items (graph) | `engrams link add --source-type <t> --source-id <n> --target-type <t> --target-id <n> --rel <canonical>` |
| Log task progress | `engrams progress log --status <Status> --description "..."` |
| Hand off context | `engrams active-context update --patch '<json>'` (merge) · `--content` (replace) |
| List patterns | `engrams pattern list [--tags a,b]` |
| Attach file anchors | `engrams anchor add --type <decision\|progress-entry\|system-pattern> --id <n> --path <path>` |
| Attach PR reference | `engrams pr add --type <decision\|system-pattern> --id <n> --pr <n_or_url>` |
| Promote repeated progress into patterns | `engrams consolidate [--apply]` — propose by default; `--apply` inserts with evidence links + confidence |
| Causal chain ("why"/impact) | `engrams graph why --node decision:7 [--down]` |
| Bulk operations | `engrams batch --type <decision\|progress\|pattern\|custom-data> --items <json_or_->` |
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

### Session End Protocol (PR close-back)

Before declaring done:
1. `decision log` each design choice (declare `--contract` for any interface introduced)
2. `link add` new items (`implements`/`depends_on`/`supersedes`)
3. `progress log --status Done`
4. `active-context update --patch '<json>'`
5. `engrams session close --reads-skipped <n> --reads-required <n> --tokens-saved <est> --pr <n>` — validates the PR has linked decisions and anchored files
6. `engrams export`
7. commit & push `engrams_export/`

For the full reference (Store/Link/Retrieve tables, `graph` queries, relationship ontology, status vocabularies) see `https://engrams.sh`.
