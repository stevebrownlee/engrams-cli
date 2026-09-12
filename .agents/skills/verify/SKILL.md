---
name: verify
description: Run the end-to-end finalization pipeline — derive and verify acceptance criteria from branch context, simplify code, run validation checks, stage/commit changes, and open a pull request.
---

# Verify (End-to-End Finalization Pipeline)

End-to-end finalization pipeline. Derives acceptance criteria from the branch context, verifies them, simplifies the code, runs all validation checks, commits, and opens a pull request.

**Halt rule:** If any phase produces a hard failure (defined per phase below), stop and report. Do not skip ahead.

---

## Phase 1 — Simplify

Invoke **`skill://code-simplification`** to reduce and consolidate code without changing observable behavior:
1. Map blast radius: `git diff main...HEAD --name-only`.
2. Identify & apply refactors (deletion, consolidation, reduce/reuse).
3. Report lines removed/added and refactors applied.

**Gate:** This phase does not fail. If no opportunities are found, proceed.

---

## Phase 2 — Initial Review

Invoke **`skill://code-review-checklist`** to perform a comprehensive , or `skill://agent-browser`.

**Gate:** If any critical issues are found, and do not have an obvious solution, halt and prompt the human for a decision. Implement a solution for all non-critical issues found.

---

## Phase 3 — Verification of UI

Only run this phase if changes were made to the frontend code that impacts the the UI in any way.

Invoke **`skill://agent-browser`** to self-verify that the UI works as described in the requirements/specification. Save the resulting video to the `/tmp` directory with a unique name. Remember the file name for use in future phases.

**Gate:** UI works as intended in both mobile and full-screen modes. All required data is displayed. All interactivity works as described.

---

## Phase 4 — Validate

Invoke **`skill://validate`** to run all backend and frontend validation checks (compile, reset test DB, Credo, format, lint:fix, type-check, unit tests, audit, check-cycles).

**Gate:** All checks must pass with zero errors. Substantial failures halt the pipeline.

---

## Phase 5 — Commit

Invoke **`skill://commit`** to stage and commit changes following project conventions:
1. Exclude files in Commit Exclusions (`AGENTS.md`).
2. Determine conventional commit type and scope.
3. Produce commit message and commit staged files.

**Gate:** A commit SHA must be produced unless working tree is already clean.

---

## Phase 6 — Create Pull Request

Invoke **`skill://create-pull-request`** to push the branch and open a GitHub PR:
1. Push branch to remote: `git push -u origin HEAD`.
2. Generate PR title and structured, beginner-friendly PR description (with "Steps to Test").
3. Create PR via `gh pr create`.

Add the video file generated in phase 3 to the PR as an asset.

**Gate:** A PR URL must be produced.

---

## Phase 7 — Watch CI gates

Invoke **`skill://watch-ci-gates`** to monitor Github CI checks and resolve issues that arise

**Gate:** All CI gates are green

---

## Phase 8 — Review and Resolve PR Comments

Invoke **`skill://review-pr-comments`** to examine comments on the PR, fix if needed, and respond to each one individually

**Gate:** All comments have been addressed and have a human sounding response.

---

## Final Report

After all phases complete, produce a summary:

```
## ✅ Pipeline Complete

**ACs:** X/Y passed, Z blocked (warnings)
**Simplify:** N refactors applied (±M lines)
**Validate:** All checks passed
**Commit:** <SHA> — <first line of message>
**PR:** <URL>
```

If the pipeline halted early, report which phase failed and why.
