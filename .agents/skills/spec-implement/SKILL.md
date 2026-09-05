---
name: spec-implement
description: Run the PILOT pipeline for a spec from Gate 0 through Gate 5.
---

<!-- managed by PILOT — generated from commands/spec-implement.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/spec-implement.md and re-run install -->

# /spec-implement

Run the full PILOT pipeline for a spec — from Gate 0 (orchestrator boot)
through Gate 5 (full-suite verification and PR description) — in one of
three modes.

## Usage

```
/spec-implement <spec-id>
/spec-implement <spec-id> --autonomous     (default; same as no flag)
/spec-implement <spec-id> --review         (autonomous + Q&A between phases)
/spec-implement <spec-id> --paired         (paired-mode with comprehension checks; slice c)
```

The `<spec-id>` is the 4-digit uppercase hex ID of the spec
(e.g., `0001`, `00A7`, `0F3B`). The spec must already exist at
`specs/<ID>-<name>.md`.

## What this command does

1. **Argument parsing.** Extract `<spec-id>` and the mode flag (if any).
   Default mode is `autonomous`. Refuse if `<spec-id>` is missing or
   malformed.

2. **Spec existence check.** Verify `specs/<ID>-<name>.md` exists. Glob
   for `specs/<ID>-*.md` since the kebab name isn't passed. If multiple
   match, surface — IDs should be unique. If none match, surface
   "spec not found" with the path searched.

3. **Invoke the orchestrator.** Pass three arguments to
   `implementation-orchestrator`:
   - the spec path
   - the mode (`autonomous` | `review` | `paired`)
   - a flag indicating this is a fresh start (not a resume)

4. **Hand off.** The orchestrator drives the pipeline. You return control
   to the user only when the orchestrator does — when the pipeline
   completes, halts, or pauses (in review/paired modes).

## What you do NOT do

- **You do not run the pipeline yourself.** Delegate to
  `implementation-orchestrator`.
- **You do not validate the spec.** The orchestrator's Gate 1 (which
  invokes `spec-reviewer`) does that.
- **You do not modify the spec.** Read-only entry point.

## Mode notes

- **`--autonomous`** (default): pipeline runs end-to-end without
  intermediate user interaction. The pedagogical layer is inactive —
  no rationale doc is produced, no comprehension checks fire, no skip
  logging happens. Surface only on halt or completion.
- **`--review`**: pipeline pauses between phases for developer Q&A.
  Before phase 1 starts, `spec-narrator` produces a rationale doc at
  `specs/<ID>-<name>.rationale.md` calibrated to the developer's
  profile at `~/.pilot/profile.md`. Skips logged during pauses
  contribute to calibration.
- **`--paired`**: same as `--review`, plus comprehension checks fire at
  phase boundaries with questions generated from the rationale doc's
  `deep` and `peer-level` layer sections. Skip events from comprehension
  checks are weighted more heavily (weight 3 vs. 1 for inline) per
  `protocols/skip-log.md`.

If the developer has no profile at `~/.pilot/profile.md`, the rationale
doc is still produced but uses default calibration (all granularity
layers at `moderate`). Run `/profile-init` to enable per-developer
calibration.

## Error cases

| Condition                                | Response                                              |
|------------------------------------------|-------------------------------------------------------|
| No spec-id given                         | Refuse with usage hint                                |
| spec-id format wrong (not 4 hex chars)   | Refuse with format hint                               |
| No matching spec file                    | Refuse with searched path                             |
| Multiple matching spec files             | Refuse with list of matches                           |
| Mode flag unrecognized                   | Refuse with list of valid modes                       |
| Progress file already exists for spec    | Suggest `/spec-resume` instead                        |
| Working tree has uncommitted changes     | Refuse — orchestrator needs a clean tree              |

## Examples

```
/spec-implement 0001
   → runs spec 0001 autonomously, end-to-end

/spec-implement 00A7 --review
   → runs spec 00A7 with Q&A pauses between phases

/spec-implement 0F3B --paired
   → runs spec 0F3B in paired mode (full semantics in slice c)
```
