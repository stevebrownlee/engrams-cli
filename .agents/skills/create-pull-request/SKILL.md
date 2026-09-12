---
name: create-pull-request
description: Push the current branch to origin and open a GitHub pull request with a concise title, a structured description, and a steps-to-test section when the change has UI or manual-verification surfaces. Used when the developer wants to open a PR for work that wasn't produced by the pipeline.
---

# Create Pull Request

Push the current branch and open a GitHub pull request following the
project's PR conventions. Used when the developer wants to open a PR for
work that wasn't produced by the PILOT pipeline (manual edits, exploratory
work, hot fixes) but should still follow project conventions.

No arguments. Operates on the current branch and its unpushed commits.

## Procedure

1. **Read context.**
   - `AGENTS.md` for project conventions
   - `skill://github-cli` for the `gh`-only rule
   - `protocols/commit-message.md` for the PR description format
     (the protocol's "Per-spec final commit PR description" section applies
     to PILOT specs; for non-spec PRs follow its spirit — traceability and
     a verification summary)

2. **Inspect the branch state.**
   - Confirm the current branch: `git branch --show-current`
   - List unpushed commits: `git log origin/<branch>..HEAD --oneline`
     (fall back to `git log <base>..HEAD --oneline` if the branch has no
     upstream yet)
   - Review the full diff against the base: `git diff <base>...HEAD`
   - Confirm the base branch (`main` unless the repo workflow says
     otherwise) before opening the PR against it

3. **Push the branch.**
   - Push to origin with upstream tracking: `git push -u origin <branch>`
   - Do NOT force-push unless the developer explicitly asks

4. **Determine the title.**
   - If the branch's commits share a single conventional-commit type and
     scope, mirror it in the PR title (e.g. `fix(gcal-sync): ...`)
   - If the branch spans multiple concerns, write a title that summarizes
     the overall theme; do not invent a scope that misrepresents the diff
   - Keep the title terse but descriptive — it should stand alone in a PR
     list

5. **Draft the PR body.**
   - Write standard, well-structured Markdown (never raw `\n` string literals or escaped newlines).
   - One-paragraph summary of what changed and why
   - Bulleted list of notable changes
   - **Steps to test** section when the change has UI components or
     workflows that need manual verification. Omit it for pure backend /
     non-UI changes
   - For spec- or issue-scoped work, link it (`Closes #<issue>` or
     `Refs: specs/<ID>-<name>.md`) so the PR traces back to its source

6. **Create the PR safely with `--body-file`.**
   - **CRITICAL:** NEVER pass multi-line Markdown or code blocks containing backticks (` `) directly via `--body "..."` in shell commands. Bash interprets backticks as subshell command execution (e.g. `` `ob install` `` will execute the command on the host) and mangles line breaks into literal `\n` characters.
   - **Always write the PR body to a temporary markdown file first**, then pass it via `--body-file`:
     ```bash
     # 1. Write the body cleanly to a temp file
     # 2. Run gh pr create using --body-file
     gh pr create --base <base> --head <branch> --title "..." --body-file /tmp/pr_body.md
     # 3. Remove the temp file
     rm -f /tmp/pr_body.md
     ```
   - Use the GitHub CLI only — never the browser or `curl`, per
     `skill://github-cli`
   - Report the PR URL
## When this skill does NOT apply

- **`/spec-implement`** opens its own PR at Gate 5 via the PILOT pipeline.
- **`/issue-pr`** opens a PR for a groomed issue implementation.
- This skill is for the in-between: manual branches, exploratory work, and
  hot fixes that need a PR.

## Boundaries

- **Do not merge.** Open the PR only; merging is the developer's call.
- **Do not request reviewers** unless the developer asks.
- **Do not force-push** unless the developer asks.
- **Do not run tests.** Use `/validate` if validation is wanted; this skill
  ships what is already committed.

## Error cases

| Condition                          | Response                                          |
| ---------------------------------- | ------------------------------------------------- |
| Branch already has an open PR      | Surface the existing PR URL and exit              |
| No unpushed commits                | Surface and exit — nothing to push or open        |
| Push rejected (non-fast-forward)   | Surface the rejection; do not force-push          |
| `gh` not authenticated             | Surface the error and let the developer fix       |
