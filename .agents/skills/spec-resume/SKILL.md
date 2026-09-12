---
name: spec-resume
description: Resume an in-progress pipeline from current state.
---

<!-- managed by PILOT — generated from commands/spec-resume.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/spec-resume.md and re-run install -->

# /spec-resume

Resume a pipeline that was paused or interrupted. Reads
`specs/<ID>-<name>.progress.json`, determines the current state, and
hands off to `implementation-orchestrator` at the appropriate gate.

## Usage

```
/spec-resume <spec-id>
/spec-resume <spec-id> --autonomous
/spec-resume <spec-id> --review
/spec-resume <spec-id> --paired
```

If a mode flag is given, it **overrides** the mode recorded in the
existing progress file. This is how a developer mid-spec switches from
autonomous to review (e.g., after hitting unfamiliar code) or vice versa.

If no mode flag is given, the mode in the progress file is preserved.

## What this command does

1. **Argument parsing.** Extract `<spec-id>` and the optional mode flag.

2. **Progress file check.** Verify
   `specs/<ID>-<name>.progress.json` exists. If it doesn't, refuse and
   suggest `/spec-implement` instead (this command is for resumption).

3. **State inspection.** Read the progress file. Refuse if:
   - `status: complete` — nothing to resume
   - `status: blocked` — surface the `blocked_reason` and instruct the
     developer to address it (edit the spec, fix the environment, etc.)
     before re-running
   - `schema_version` doesn't match (`"1"`)

4. **Mode reconciliation.** If a mode flag was given, update the
   progress file's `mode` field. Otherwise, use the existing mode.

5. **Invoke the orchestrator.** Pass:
   - the spec path
   - the (possibly updated) mode
   - a flag indicating this is a resume (not a fresh start)

6. **Hand off.** The orchestrator picks up from the current pipeline
   state per `protocols/gate-checks.md` resume routing.

## When to use this vs. /spec-implement

- **`/spec-implement`** is for fresh runs. No progress file should exist.
- **`/spec-resume`** is for picking up an interrupted or paused run.

If you run `/spec-implement` on a spec that already has a progress
file, it refuses and points you here. If you run `/spec-resume` on a
spec that has no progress file, it refuses and points you to
`/spec-implement`.

## Mid-spec mode switching

The most common use of `--<mode>` on resume is the
"started-autonomous-now-want-review" case. Example:

```
/spec-implement 0001                  → starts autonomous
   (pipeline runs through phase 3, hits unfamiliar code in phase 4)
   (developer hits Ctrl-C between phases)

/spec-resume 0001 --review            → continues from phase 4
                                        with rationale doc and Q&A pauses
```

The reverse also works — start in review, switch to autonomous when
you've seen enough.

## Error cases

| Condition                                | Response                                          |
|------------------------------------------|---------------------------------------------------|
| No spec-id given                         | Refuse with usage hint                            |
| No matching progress file                | Suggest `/spec-implement` instead                 |
| Pipeline is `complete`                   | Surface and refuse                                |
| Pipeline is `blocked`                    | Surface `blocked_reason` and refuse               |
| Schema version mismatch                  | Surface migration message                         |
| Working tree has uncommitted changes     | Refuse — orchestrator needs a clean tree          |
