---
name: spec-compose
description: Interactive spec authoring via the spec-composer agent.
---

<!-- managed by PILOT — generated from commands/spec-compose.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/spec-compose.md and re-run install -->

# /spec-compose

Start an interactive spec-authoring session. Walks you through producing
a well-formed spec under `specs/` that's ready for the rest of the
pipeline.

## Usage

```
/spec-compose
/spec-compose <spec-name>
```

`<spec-name>` is an optional kebab-case hint for the spec name
(e.g., `user-authentication`, `csv-export`). If given, `spec-composer`
proposes this as the filename; if omitted, `spec-composer` infers a
name from the conversation.

## What this command does

1. **Argument parsing.** Extract optional `<spec-name>` hint.

2. **Workspace check.** Verify `specs/` exists. Create it if not.

3. **ID selection.** List existing specs to determine the next free
   4-digit uppercase hex ID. This is informational; `spec-composer`
   confirms with the user.

4. **Invoke `spec-composer`.** Pass:
   - the next free ID
   - the optional name hint
   - the project root

5. **Hand off.** `spec-composer` runs interactively, asking the
   developer questions until a complete spec file is written to
   `specs/<ID>-<name>.md`.

## What you do NOT do

- **You do not write any spec content.** That's `spec-composer`'s job.
- **You do not invoke the orchestrator.** This command exits when the
  spec file is written; running the pipeline is a separate command
  (`/spec-implement`).
- **You do not validate the spec.** `spec-composer` checks against
  `protocols/spec-format.md` as it writes. Final validation happens at
  Gate 1 when `/spec-implement` is run.

## What happens after

When `spec-composer` finishes:

```
Spec written to specs/<ID>-<name>.md.
Ready to run: /spec-implement <ID>
```

The developer chooses when to run the pipeline. Specs can sit
uncommitted, get reviewed, get edited, then run later.

## Resuming a draft

If the developer stopped a previous compose session mid-way,
`spec-composer` saved progress to `specs/<ID>-<name>.md.draft`. To
resume, run `/spec-compose` again and reference the draft when
prompted. `spec-composer` reads the partial file and continues from
where it left off.

## Error cases

| Condition                       | Response                                          |
|---------------------------------|---------------------------------------------------|
| `AGENTS.md` missing             | Refuse — spec-composer needs project context      |
| `specs/` exists but unwritable  | Refuse with permission error                      |
| ID space exhausted (>FFFF)      | Refuse — surface "spec IDs exhausted, rotate"     |
| `<spec-name>` hint malformed    | Surface and let spec-composer prompt for a name   |
