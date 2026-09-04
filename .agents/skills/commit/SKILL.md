---
name: commit
description: Stage and commit current changes using Conventional Commits format. Inspects the working tree, determines commit type and scope, stages appropriate files, and produces a well-formatted commit message.
---

# Commit

Stage current changes and produce a commit following the project's
commit-message protocol. Used when the developer wants to commit work
that wasn't produced by the pipeline (manual edits, exploratory work,
spec-revision commits) but should still follow project conventions.

No arguments. Operates on whatever is currently staged or unstaged in
the working tree.

## Model

Perform the entire commit task using the **`COMMIT`** model. When you
delegate any part of this skill (diff analysis, type/scope determination,
message drafting) to the `llm` tool, instruct it to use the `COMMIT`
model. Do not use the session default for this work.

## Procedure

1. **Read context.**
   - `AGENTS.md` for project conventions and the **Commit exclusions** section
   - `protocols/commit-message.md` or `.pilot/protocols/commit-message.md` if present; otherwise follow standard Conventional Commits format
   - `AGENTS.md` rules or `.pilot/rules/commits.md` if present

2. **Inspect the working tree.**
   - Run `git status --porcelain` to see what's modified
   - Run `git diff --staged` and `git diff` to see what's actually changed
   - Identify files that match commit exclusions in `AGENTS.md` and flag
     them (do not stage)

3. **Determine the commit type and scope.**
   - Look at the diff and choose the conventional commit type
     (`feat`, `fix`, `refactor`, `chore`, `test`, `docs`, `perf`, `build`)
   - If the changes touch a spec, the scope is the spec ID (e.g., `feat(0001)`)
   - If the changes touch `AGENTS.md` or PILOT framework files, the scope
     is `pilot` (e.g., `chore(pilot)`)
   - If the changes are unrelated to a spec or PILOT, omit the scope
     (e.g., `chore: bump dependencies`)

4. **Stage the appropriate files.**
   - Stage files the developer modified
   - Do NOT stage files in commit exclusions
   - Show the developer what will be staged before committing

5. **Produce the commit message.**
   - First line: `<type>(<scope>): <imperative summary>` ≤ 72 chars
   - Body: 1–3 paragraphs describing what changed and why
   - **No hard line breaks within paragraphs:** Each paragraph must be a single, unwrapped string of text with no mid-sentence `\n` characters (separate paragraphs with a blank line only). Do not hard-wrap paragraphs at 72 or 80 characters.
   - For spec-scoped commits, include `Refs: specs/<ID>-<name>.md`
   - For PILOT-framework commits, include `Refs: pilot/`
6. **Confirm and commit.**
   - Show the developer the staged file list and the proposed message
   - On confirmation, run `git commit`
   - Report the commit SHA

## When this skill does NOT apply

- **`/spec-implement`** produces commits automatically per phase.
- This skill is for the in-between: hot fixes, dependency bumps, spec
  edits, `AGENTS.md` updates, exploratory work, manual cleanups.

This skill does NOT update `progress.json`. If the developer commits
changes that should have been part of a phase, the progress file is
already stale (and that's expected — Gate 5 already passed, or the
phase already shipped its commit).

## Boundaries

- **Do not push.** Local commit only.
- **Do not modify the spec.** If the developer wants to edit a spec,
  that's a manual edit; this skill just commits the result.
- **Do not run tests.** Commit what's there; the developer is
  responsible for confirming it works before committing. (Use
  `/validate` if they want to run the project's validation commands.)
- **Do not stage everything.** Commit exclusions are honored. If the
  developer has uncommitted secrets, refuse to stage them and surface
  the exclusion.

## Error cases

| Condition                          | Response                                          |
| ---------------------------------- | ------------------------------------------------- |
| Nothing to commit                  | Surface and exit                                  |
| All modified files are excluded    | Surface excluded files and refuse                 |
| Conventional commit type ambiguous | Ask the developer which type fits                 |
| Pre-commit hook fails              | Surface hook output and let developer fix         |
