# Protocol: progress-schema

**Status:** v1
**Loaded by:** `spec-json-builder` (writes), `phase-implementer`, `phase-scaffold`,
`phase-debugger`, `code-reviewer`, `implementation-orchestrator` (read/update),
`spec-narrator` (reads for rationale generation)
**Defines:** the JSON shape of `specs/<id>-<name>.progress.json`

---

## Purpose

The progress file is the pipeline's state machine. It is written by
`spec-json-builder` at Gate 2 and updated by every agent that touches the
pipeline thereafter. It records what phases exist, what their status is,
what was tried, and what failed. The orchestrator reads it to decide what
to do next; agents read it to know what's already been done.

**It is a state file, not a planning artifact.** Once a pipeline is
running, no human edits it directly. Editing it during a run produces
undefined behavior.

## File path

```
specs/<ID>-<kebab-case-name>.progress.json
```

The base name (everything before `.progress.json`) matches the spec
filename's base name exactly. `spec-json-builder` derives it from `spec_path`
at creation time.

## Top-level schema

```json
{
  "schema_version": "1",
  "spec_id": "0001",
  "spec_name": "user-authentication",
  "spec_path": "specs/0001-user-authentication.md",
  "created_at": "2026-05-11T19:32:00Z",
  "updated_at": "2026-05-11T20:15:42Z",
  "status": "in_progress",
  "mode": "autonomous",
  "current_phase": 2,
  "phases": []
}
```

### Top-level fields

| Field            | Type     | Required | Notes                                            |
|------------------|----------|----------|--------------------------------------------------|
| `schema_version` | string   | yes      | Schema version. v1 uses `"1"`.                   |
| `spec_id`        | string   | yes      | 4-digit uppercase hex, matches spec filename     |
| `spec_name`      | string   | yes      | Kebab-case name, matches spec filename           |
| `spec_path`      | string   | yes      | Repo-relative path to the spec markdown          |
| `created_at`     | string   | yes      | ISO 8601 UTC timestamp; set by spec-json-builder      |
| `updated_at`     | string   | yes      | ISO 8601 UTC timestamp; updated on every write   |
| `status`         | enum     | yes      | See **Pipeline status values** below             |
| `mode`           | enum     | yes      | `autonomous` \| `review` \| `paired`             |
| `current_phase`  | number   | yes      | 1-indexed phase number. `0` before any phase.    |
| `phases`         | array    | yes      | Phase objects in execution order                 |

### Pipeline status values

| Value         | Meaning                                                    |
|---------------|------------------------------------------------------------|
| `pending`    | Created by spec-json-builder. No phase has started yet.         |
| `in_progress` | Some phase is currently executing                          |
| `blocked`     | Pipeline halted; a phase is blocked. Human action needed.  |
| `complete`    | All phases complete. Gate 5 passed. PR-ready.              |
| `aborted`     | Human halted the pipeline (e.g. via Ctrl-C or `/abort`)    |

The pipeline starts as `pending` after spec-json-builder finishes. It moves to
`in_progress` when the first phase starts. It returns to `in_progress`
between phases. It moves to `blocked` only when phase-debugger has run and
the orchestrator has determined human input is needed. It moves to
`complete` only after Gate 5 succeeds.

---

## Phase object schema

Each entry in the `phases` array is a phase. Phases are flat (no nesting).
Ordering is handled by array position; cross-phase dependencies are
explicit via `depends_on`.

```json
{
  "id": 1,
  "title": "Add Session model and migration",
  "type": "standard",
  "status": "complete",
  "depends_on": [],
  "exemplars": ["prisma/schema.prisma"],
  "acceptance": ["AC-1", "AC-3"],
  "verification": [
    "pnpm prisma migrate dev --name add-session-model --create-only",
    "pnpm prisma generate",
    "pnpm typecheck"
  ],
  "files_touched": [
    "prisma/schema.prisma",
    "prisma/migrations/20260511_add_session_model/migration.sql"
  ],
  "started_at": "2026-05-11T19:35:00Z",
  "completed_at": "2026-05-11T19:38:14Z",
  "retry_count": 0,
  "blocked_reason": null,
  "review_findings": [],
  "commit_sha": "a1b2c3d4e5f6"
}
```

### Phase fields

| Field             | Type      | Required at create | Notes                                       |
|-------------------|-----------|--------------------|---------------------------------------------|
| `id`              | number    | yes                | 1-indexed phase number                      |
| `title`           | string    | yes                | Short imperative summary                    |
| `type`            | enum      | yes                | `standard` \| `scaffold`                    |
| `status`          | enum      | yes                | See **Phase status values** below           |
| `depends_on`      | array     | yes                | Phase IDs this phase requires complete      |
| `exemplars`       | array     | yes                | Repo-relative paths to similar code         |
| `acceptance`      | array     | yes                | AC IDs from the spec this phase satisfies   |
| `verification`    | array     | yes                | Commands to run at Gate 3                   |
| `files_touched`   | array     | populated at run   | Repo-relative paths modified by this phase  |
| `started_at`      | string    | populated at run   | ISO 8601 UTC                                |
| `completed_at`    | string    | populated at run   | ISO 8601 UTC; null until status=complete    |
| `retry_count`     | number    | yes (init 0)       | Times verification has failed in this phase |
| `blocked_reason`  | string    | populated on block | Free text; written by phase-debugger        |
| `review_findings` | array     | populated at G3    | Findings from code-reviewer                 |
| `commit_sha`      | string    | populated at G4    | Sha of the phase's commit                   |

### Phase status values

| Value          | Meaning                                                            |
|----------------|--------------------------------------------------------------------|
| `pending`      | Defined by spec-json-builder, not yet started                           |
| `in_progress`  | phase-implementer or phase-scaffold is currently executing it      |
| `verifying`    | Verification commands are running (Gate 3)                         |
| `reviewing`    | code-reviewer is running (Gate 4)                                  |
| `complete`     | All gates passed for this phase                                    |
| `blocked`      | phase-debugger ran, produced a diagnosis, human input needed       |
| `skipped`      | Phase explicitly skipped (rare; recorded for audit trail)          |

### Phase types

- **`standard`** — full work: design, code, tests, edge cases. Run by
  `phase-implementer`.
- **`scaffold`** — mechanical work: boilerplate, file moves, type updates,
  imports. Run by `phase-scaffold` at lower model tier for speed and cost.

`spec-json-builder` decides which type each phase is when it produces the
progress file.

### Acceptance references

The `acceptance` array holds **AC IDs**, not the criterion text itself.
Each entry is a string of the form `AC-N` (e.g., `"AC-1"`, `"AC-12"`)
that references a criterion defined in the spec's Acceptance criteria
section.

This indirection keeps the criterion text canonical in the spec. If the
spec author edits the wording of `AC-3`, every phase that satisfies
`AC-3` automatically reflects the new wording — no progress.json
rewriting needed. It also keeps `progress.json` compact and machine-
readable: a phase's acceptance is a small list of IDs, not embedded
prose.

`spec-json-builder` populates this array at creation time by mapping each
phase's contribution to the AC IDs it advances. A phase may reference
zero, one, or many AC IDs. An AC ID may be referenced by multiple
phases (when a criterion requires work spread across phases).

The PR description generated at Gate 5 resolves each AC ID back to its
G/W/T block by reading the spec.

### Review findings

Each entry in `review_findings` follows this shape (see also
`protocols/self-review.md` for the full contract):

```json
{
  "severity": "minor",
  "category": "convention-drift",
  "message": "Direct prisma import in route handler; AGENTS.md invariant requires going through lib/db/",
  "file": "app/api/login/route.ts",
  "line": 12,
  "resolution": "fixed_in_phase"
}
```

| Field        | Notes                                                  |
|--------------|--------------------------------------------------------|
| `severity`   | `severe` \| `moderate` \| `minor` (see gate-checks)    |
| `category`   | Short tag (`convention-drift`, `missing-test`, etc.)   |
| `message`    | One-sentence description                               |
| `file`       | Repo-relative path (optional)                          |
| `line`       | Line number (optional)                                 |
| `resolution` | `fixed_in_phase` \| `deferred` \| `noted`              |

---

## Update conventions

The progress file is written by agents during their work. To keep it
trustworthy:

1. **Always update `updated_at`** when writing. Use ISO 8601 UTC.
2. **Append, don't reorder.** Never reorder the `phases` array. Never
   reorder entries in `review_findings`.
3. **Status transitions are forward-only**, except for retry:
   - `pending → in_progress → verifying → reviewing → complete`
   - `verifying → in_progress` on a retry (resets to in_progress, not back
     to pending; retry_count is incremented)
   - `* → blocked` is allowed from any state
4. **Don't delete phases.** If a phase is no longer needed, mark it
   `skipped` and add a `review_findings` entry explaining why.
5. **Files are written atomically.** Read the entire file, mutate in
   memory, write the entire file back. No partial writes.

## Read pattern

When an agent loads the progress file, it should:

1. Read the file once into memory
2. Check `schema_version` — if not `"1"`, refuse to operate
3. Check `status` — if `complete`, `aborted`, or `blocked`, abort (unless
   the agent is explicitly handling that state)
4. Identify its phase via `current_phase` or by scanning for the first
   `pending` phase whose `depends_on` are all `complete`
5. Update `started_at` and `status` before doing any real work
6. Write the file back atomically

## Schema migrations

When the schema changes in a future version:
- Increment `schema_version`
- Provide a migration step in the installer
- Old `schema_version` files are read-only until migrated

v1 does not include migration tooling; if/when v2 ships, a `bin/migrate.sh`
will handle the conversion.
