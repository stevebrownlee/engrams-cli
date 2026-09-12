---
name: debug-github-action
description: Debug GitHub Actions workflow runs using gh CLI — inspect runs, job status, step details, pull logs, check PR comments, view artifacts, and rerun failed jobs.
---

# Debug a GitHub Actions Workflow Run

Use `gh` CLI to inspect workflow runs, pull logs, view job details, and check PR comments — all without leaving the terminal.

## 1. List recent runs for a workflow

```bash
# By workflow name
gh run list --workflow="<Workflow Name>" --limit 5 --json databaseId,status,conclusion,headBranch,displayTitle

# By branch
gh run list --branch <branch-name> --limit 5 --json databaseId,status,conclusion,workflowName
```

## 2. Get job-level status for a run

```bash
# Quick overview: job names + conclusions
gh run view <RUN_ID> --json jobs --jq '.jobs[] | "\(.id) \(.name) \(.conclusion)"'
```

## 3. Get step-level details for a specific job

```bash
# List all steps and their conclusions for a specific job
gh api repos/<OWNER>/<REPO>/actions/jobs/<JOB_ID> --jq '.steps[] | "\(.number) \(.name) \(.conclusion)"'
```

## 4. Pull full logs for a job

```bash
# Download and search logs for a specific job
gh api repos/<OWNER>/<REPO>/actions/jobs/<JOB_ID>/logs 2>&1 | head -200

# Search for specific patterns (errors, test results, etc.)
gh api repos/<OWNER>/<REPO>/actions/jobs/<JOB_ID>/logs 2>&1 | grep -iE "error|failed|passed|skipped" | head -40

# Get the last N lines (often contains the summary)
gh api repos/<OWNER>/<REPO>/actions/jobs/<JOB_ID>/logs 2>&1 | tail -60
```

> **Note:** `gh run view <RUN_ID> --log` only shows logs for the main job.
> For reusable workflows (`workflow_call`), use the `gh api` approach with the specific JOB_ID.

## 5. View PR comments

```bash
# List all bot comments on a PR
gh pr view <PR_NUMBER> --json comments --jq '.comments[] | select(.author.login == "github-actions") | .body' | head -50

# Or use the API for more control
gh api repos/<OWNER>/<REPO>/issues/<PR_NUMBER>/comments --jq '.[] | select(.user.type == "Bot") | {id: .id, body: .body[:200]}'
```

## 6. Check workflow artifacts

```bash
# List artifacts from a run
gh api repos/<OWNER>/<REPO>/actions/runs/<RUN_ID>/artifacts --jq '.artifacts[] | "\(.name) \(.size_in_bytes) bytes"'
```

## 7. Re-run a failed job

```bash
# Re-run only failed jobs
gh run rerun <RUN_ID> --failed

# Re-run entire workflow
gh run rerun <RUN_ID>
```

## 8. Common debugging patterns

### Find the OWNER/REPO from the current git remote
```bash
gh repo view --json nameWithOwner --jq '.nameWithOwner'
```

### Find the run ID for the latest run on the current branch
```bash
gh run list --branch $(git branch --show-current) --limit 1 --json databaseId --jq '.[0].databaseId'
```

### Full debug pipeline: find run → get jobs → pull failing job logs
```bash
RUN_ID=$(gh run list --branch $(git branch --show-current) --limit 1 --json databaseId --jq '.[0].databaseId')
echo "Run ID: $RUN_ID"

gh run view $RUN_ID --json jobs --jq '.jobs[] | "\(.id) \(.name) \(.conclusion)"'

# Then use the JOB_ID from above for the failing job:
# gh api repos/<OWNER>/<REPO>/actions/jobs/<JOB_ID>/logs 2>&1 | grep -i "error\|failed" | head -30
```
