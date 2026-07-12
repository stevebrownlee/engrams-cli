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
