---
name: diagnose
description: >
  Use when investigating or diagnosing an issue raised by the system, a user, or a
  developer — production incidents, unexpected data, error reports, alert anomalies,
  or UI defects. Gathers read-only evidence (psql database inspection, agent-browser UI
  inspection, logs) and produces a diagnosis report. Strictly read-only: no data
  mutation, no code edits, no reading of credential files.
---

# Diagnose (Read-Only Investigation)

Investigate issues and find root cause from evidence. **You diagnose. You never fix.**

## Hard Prohibitions (inviolable)

This skill is strictly read-only. You are PROHIBITED from:

1. **Mutating data** — no `INSERT`, `UPDATE`, `DELETE`, `DROP`, `TRUNCATE`, `ALTER`,
   `CREATE`, `GRANT`, or any DML/DDL. `SELECT` and `EXPLAIN` are the only permitted
   SQL verbs.
2. **Writing or editing code** — no source changes, migrations, config edits, `git`
   writes, or running formatters/builds. Diagnosis yields a report, not a patch.
3. **Reading credentials** — never open, print, or echo the contents of `.env.prd` or any
   other file holding a database connection string or secret (`.env*`, `*.envrc`,
   `config/*.secret.exs`, deploy manifests, CI secret files). `source` the file so the
   shell holds the values, then reference them only through variables like
   `"$DB_URL"`. The secret must never enter your context, the transcript, the
   evidence directory, or the report.

### Rationalizations to reject

| Excuse | Reality |
|---|---|
| "A read-only `UPDATE` to test a theory" | Any write is a mutation. Prove it with `SELECT`. |
| "The fix is obvious, I'll just make it" | Diagnosis ≠ fix. Report it; an implementer makes the change. |
| "The migration is safe / reversible" | Migrations are code edits AND data mutations. Forbidden. |
| "I'll wrap it in a transaction and roll back" | Still a mutation while it runs. `SELECT` only. |
| "Adding an index won't change data" | DDL is a mutation. Forbidden. |
| "I need to edit code to reproduce it" | Reproduce read-only: logs, `SELECT`, browser inspection. |
| "I'll just `cat .env.prd` to get the host and port" | Sourcing gives you a working connection without seeing the value. Reading it is forbidden. |
| "I have to see `DB_URL` to check its format" | The shell can test the value without printing it — use the `case` check below. |
| "I'll `echo $DB_URL` to confirm it loaded" | That prints the password. Test the connection instead: `psql "$DB" -c 'SELECT 1;'`. |
| "The connection failed, I need to inspect the file to debug" | Debug with the non-revealing checks below. Never print the value. |
| "I'll only read the non-secret lines / grep one key" | You cannot know which lines are secret without reading them. The whole file is off limits. |

### Red flags — STOP

- Reaching for `UPDATE`/`DELETE`/`INSERT` "just to check"
- Opening an editor / running `edit` or `write` on a source file
- Drafting a migration; running `git add`/`git commit` or formatters
- Running `cat`/`head`/`tail`/`less`/`grep`/`view`/`read` against `.env.prd` or any secret file
- `echo`, `env`, `printenv`, or `set` output that would include `DB_URL`
- Pasting a connection string, password, or host into a query, screenshot, log excerpt, or report
- "While I'm in here, I'll also fix…"

**Any of these → stop, switch to evidence-gathering, and report.**

## Investigation Process

1. **Confirm the symptom** — logs, `SELECT` queries, or browser state.
2. **Form hypotheses first** — list likely explanations by probability *before* digging.
   Don't lock onto the first one.
3. **Test each read-only** — one targeted query or browser step per hypothesis.
   Falsify, don't just confirm.
4. **Report** — root cause (confirmed vs. `[INFERENCE]`), evidence with exact
   values/IDs/timestamps, and a recommended next step. A human or implementer acts.

## Production Database Issues (psql)

The connection string lives in `.env.prd` as `DB_URL` (`postgresql://USER:PASS@HOST:PORT/DATABASE`) — already in the scheme `psql` expects, so it needs no transformation. **Do not open that file.** Source it and pass the variable straight through:

Examples of usage:

```bash
bash -c 'set -a; source .env.prd; set +a; psql "$DB_URL" -c "SELECT id, worker, state, args, attempted_at, completed_at FROM core.oban_jobs WHERE args->> '\''appointment_id'\'' = '\''382985'\'' ORDER BY attempted_at DESC LIMIT 30;"' 
```

```bash
set -a; source .env.prd; set +a    # loads DB_URL; prints nothing

# SELECT / EXPLAIN only
psql "$DB_URL" -c "SELECT id, status_key, inserted_at FROM core.workflows WHERE id = 123;"
psql "$DB_URL" -c "EXPLAIN SELECT * FROM core.patients WHERE org_id = 7;"
```

Always quote `"$DB_URL"`. Never interpolate it into a string you print, a heredoc you display, or a file you write.

### Debugging the connection without exposing it

If `psql` fails, diagnose with checks that reveal nothing sensitive:

```bash
[ -n "$DB_URL" ] && echo "DB_URL is set" || echo "not set"      # loaded?
[ -f .env.prd ] && echo "env file present"                                 # file there?
case "$DB_URL" in postgresql://*) echo "scheme ok";; *) echo "unexpected scheme";; esac
psql "$DB_URL" -c "SELECT 1;"                                        # connectivity
psql "$DB_URL" -c "SELECT current_database(), current_user;"         # right target?
```

The `case` check confirms the scheme without printing the value. If it reports an unexpected
scheme, say so in the report and stop — do not open the file to look.

If an error message from `psql` or a stack trace echoes back part of the connection string,
redact it before it goes anywhere: report `connection refused to <redacted host>`, not the
literal value.

Rules:

- **Schema-qualify every table** — `core.workflows`, never bare `workflows`. Follow the
  sql-schema-prefix skill: schema is `core`, `<tenant>`, or `staging`.
- **Never query PHI** — no patient names, addresses, phone numbers. `enterprise_patient_id`
  is the only permitted patient identifier.
- **Never surface credentials** — no connection strings, passwords, hosts, or tokens in
  queries, evidence files, screenshots, or the final report.
- **Bound results** — always `LIMIT` ad-hoc queries; never dump a full table.
- **Scope by `org_id`** — every query filters by org; unscoped queries are invalid.
- **Read logs** — `backend/log/dev.log` holds recent backend output; production logs live
  in the observability tool. If a log line contains a connection string, do not copy it
  into your report verbatim.

## UI Issues (agent-browser)

Invoke `skill://agent-browser` to diagnose all UI issues.

## Report Format

A diagnosis is a report, not a fix. Deliver:

- **Symptom** — what was reported.
- **Root cause** — confirmed, or `[INFERENCE]` if not proven.
- **Evidence** — exact query results, log lines, browser state, IDs/timestamps. No PHI,
  no credentials.
- **Recommended action** — what the implementer should do (you don't do it).