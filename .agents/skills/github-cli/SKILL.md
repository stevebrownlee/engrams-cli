---
name: github-cli
description: >
  Apply whenever having any interaction with GitHub (e.g., managing PRs, issues, repos, workflows, comments, etc.).
  Ensures that the agent always uses the terminal-based `gh` tool instead of web searches, curl, or browser subagents.
---

# GitHub CLI Only for GitHub Interactions

When instructed to interact with GitHub for any operation, you MUST use the terminal-based `gh` command-line tool executed via `bash(command="gh ...")`.

**CRITICAL:** `github-cli` is a skill, NOT an agent tool/function. Do NOT attempt to call a function named `github-cli(...)`. Always execute `gh` commands in the persistent shell via `bash`.

Do NOT use browser subagents to navigate the GitHub website, and do NOT use `curl` or `wget` with manual authentication headers unless specifically required and `gh` is unavailable.
## Principles

1. **Terminal-First**: Always leverage `gh` commands via terminal execution rather than browser interaction.
2. **Authenticated Context**: The `gh` CLI on the system is already configured with appropriate credentials. Do not ask for or hardcode access tokens.
3. **Terse & Automated**: Prefer using `--json` and `--jq` flags with `gh` to extract exact structured info instead of parsing verbose terminal logs or using interactive prompts.

## Common Operations

### Pull Requests
- **Create a PR**: `gh pr create --title "..." --body "..."`
- **View PR Details**: `gh pr view [<pr-number> | <branch>] --json <fields>`
- **List PRs**: `gh pr list --limit 10`
- **Check PR Status/Checks**: `gh pr checks`

### GitHub Actions & Workflow Runs
- **List Runs**: `gh run list --workflow="<Workflow Name>" --limit 5`
- **View Run Status/Jobs**: `gh run view <run-id> --json jobs`
- **Trigger/Rerun Run**: `gh run rerun <run-id> [--failed]`

### Issues
- **List Issues**: `gh issue list --limit 10`
- **View Issue**: `gh issue view <issue-number>`
- **Create Issue**: `gh issue create --title "..." --body "..."`

### General Repository Info & API
- **View Repo Info**: `gh repo view`
- **Arbitrary API Access**: Use `gh api` to query endpoints directly (e.g., `gh api repos/{owner}/{repo}/issues/{number}/comments`).
