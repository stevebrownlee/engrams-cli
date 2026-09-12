---
name: staff-schedule-template-reconciliation
description: >-
  Analyze staff schedule templates, compare source-of-truth configuration against
  current appointment_series state, produce dry-run plans, optionally apply approved
  changes, and verify post-apply wall-clock behavior. Use when reconciling staff
  schedules, schedule templates, template series, off days, work blocks, or source
  schedule configuration.
---

# Staff schedule template reconciliation

Analyze current staff schedule templates, compare them with source-of-truth
configuration, produce a dry-run plan, optionally apply approved changes, and
verify post-apply state. The current implementation is FH clinical staff, but the
workflow is staff-schedule oriented and can grow to other teams.

## Non-negotiable apply gate

An initial request to reconcile a production schedule authorizes investigation and
dry-run only. It is **not** approval to apply or mutate production data.

### Absolute prohibition on preliminary database mutations

**NEVER execute ANY database mutation (`INSERT`, `UPDATE`, `DELETE`, `ALTER`, or DDL/DML scripts) against production during investigation, preflight, or dry-run phases.**

- All database commands during investigation and dry-run MUST be strictly read-only (`SELECT`).
- **Do NOT attempt to "unblock" a failing dry-run task by writing preliminary rows to production** (e.g., inserting missing `user_business_role_periods` or user records via raw `psql`).
- If a dry-run task or preflight check fails due to missing database records or prerequisites, **stop and report the missing prerequisite to the user as a blocker**. Surface what is missing and ask how they wish to proceed.
- Do NOT use raw SQL writes as a substitute or preparation for `mix clinical.apply_templates apply --confirm`.

### Gated apply execution

Never run `mix clinical.apply_templates apply --confirm` until all of these are
true:

1. You ran a prod-target dry run after the latest code/config change.
2. You summarized the dry-run output to the user: repo target, scope, email
   filter, unchanged/skipped counts, current series to cap or delete, rows to
   create, and warnings.
3. You asked for explicit apply confirmation in a separate user turn.
4. The user replied with explicit apply approval after seeing the dry-run
   summary.

Do **not** run dry run and apply in the same assistant turn. If config changes
after confirmation, confirmation is stale: run a new dry run and ask again.
Use this confirmation prompt:

```markdown
Dry run is complete and no writes have been made. Reply `APPLY` to run the prod
schedule template update for `<email>` against `<repo target>`, or tell me what
to change.
```

## Current source mappings

| Staff group | Source of truth | Prod `ext.source` | Notes |
|-------------|-----------------|-------------------|-------|
| Health Guide | `backend/apps/fh/priv/repo/seed_data/health_guide_schedule_blocks.csv` | `virtual_hg_schedule_csv` | Rows keyed by `owner_email`; OFF blocks use `time_off` |
| RN (triage nurse) | `@rn_schedules` in `backend/apps/fh/lib/fh/clinical/apply_templates.ex` | `one_time_rn_schedule_repair_2026_06` | One `open_time` series per person; `byday` = work days; `off_day` metadata |
| Psych NP | `@psych_np_users` + `@psych_np_templates` in `backend/apps/fh/lib/fh/clinical/apply_templates.ex` | `one_time_psych_np_schedule_repair_2026_06` | Block templates + full-day off-day row per NP |
| SOAR specialist | `@soar_users` in `backend/apps/fh/lib/fh/clinical/apply_templates.ex` | `clinical_soar_schedule_templates_2026_06` | Block-per-series grid; mixed `open_time` and `soar_warm_hand_off_target`; apply one user at a time |

RNs/NPs are not in the HG CSV. HGs are not in `@rn_schedules`. SOAR users are not auto-discovered from prod roster — only emails listed in `@soar_users`.

Also check `users.timezone_key`: UI displays in the user's timezone; series
`ext.timezone_key` drives encoded wall-clock times.

## Command behavior

Entrypoint:

```bash
cd backend
mix clinical.apply_templates
```

- Starts Core with Oban processing disabled so schedule reconciliation does not
  drain unrelated jobs from the target database.
- Template creation can enqueue explicit `Core.Scheduling.MaterializeSeriesWorker`
  jobs.
- HG with `--effective-date` uses phased touch-changed-only behavior.
- HG without `--effective-date`, RN, Psych NP, and SOAR use immediate source-owned
  replacement behavior.

### Flags

| Flag | Values | Default |
|------|--------|---------|
| `--db-url` | Prod Postgres URL from `/workspace/.env.prd` | local dev repo if missing |
| subcommand | `plan`, `apply`, `describe` | `plan` |
| `--confirm` | required for `apply` writes | dry run |
| `--scope` | `all`, `rn`, `psych_np`, `soar`, `hg` | `all` |
| `--email` | Single email (normalized) | all users in scope |
| `--replace` | Replace Psych NP/RN/SOAR shape mismatch | `false` |
| `--effective-date` | `YYYY-MM-DD` Monday cutover for phased HG | omitted |
| `--format` | `text`, `json` | `text` |

## Prod credential rules

Production credentials live in `/workspace/.env.prd` as `DB_URL`. Do not rely on
`PROD_DB_URL`, `make psql`, or an already-exported `DATABASE_URL`.

For every prod dry run or apply:

1. Source `/workspace/.env.prd`.
2. Fail if `DB_URL` is empty.
3. Pass `--db-url "$DB_URL"` to the Mix task.
4. Confirm the task prints the expected prod `Repo target` before continuing.
5. Stop if the target is local, blank, or not the expected production database.

Preflight:

```bash
cd backend
set -a; source /workspace/.env.prd; set +a
test -n "${DB_URL:-}" || { echo "DB_URL missing from /workspace/.env.prd"; exit 1; }
psql "$DB_URL" -X -v ON_ERROR_STOP=1 -q -c "SELECT current_database();"
```

## Command templates

Dry run:

```bash
cd backend
set -a; source /workspace/.env.prd; set +a
test -n "${DB_URL:-}" || { echo "DB_URL missing from /workspace/.env.prd"; exit 1; }
mix clinical.apply_templates plan \
  --db-url "$DB_URL" \
  --scope hg \
  --email anna.kutcher@firsthandcares.com \
  --effective-date 2026-06-29
```

Apply after explicit post-dry-run confirmation:

```bash
cd backend
set -a; source /workspace/.env.prd; set +a
test -n "${DB_URL:-}" || { echo "DB_URL missing from /workspace/.env.prd"; exit 1; }
mix clinical.apply_templates apply \
  --db-url "$DB_URL" \
  --scope hg \
  --email anna.kutcher@firsthandcares.com \
  --effective-date 2026-06-29 \
  --confirm
```

## Workflow

```markdown
- [ ] 1. Identify staff group and person email
- [ ] 2. Read source-of-truth config
- [ ] 3. Query prod template series and materialized template appointments
- [ ] 4. Compare expected schedule: blocks, off days, timezone
- [ ] 5. Propose config changes
- [ ] 6. Dry run and review unchanged/cap/delete/create counts
- [ ] 7. Ask for explicit `APPLY` confirmation after summarizing dry-run output
- [ ] 8. Apply only after confirmation, preferably with `--email` (SOAR rollout: one user at a time)
- [ ] 9. Verify prod with read-only SQL
- [ ] 10. Report summary
```

## Investigation

HG CSV:

```bash
rg '^owner.email@firsthandcares.com' backend/apps/fh/priv/repo/seed_data/health_guide_schedule_blocks.csv
```

RN/NP module config:

```bash
rg -A8 'email: "person@firsthandcares.com"' backend/apps/fh/lib/fh/clinical/apply_templates.ex
```

Prod template series: inspect `ext` fields: `weekday`, `byday`, `start_time`,
`end_time`, `off_day`, `timezone_key`, `source`, and `source_label`.

For reusable prod SQL snippets, see [sql-reference.md](references/sql-reference.md).

Wall-clock display:

```sql
scheduled_start_at AT TIME ZONE 'UTC' AT TIME ZONE 'America/Denver'
```

Do **not** use `AT TIME ZONE 'America/Denver'` alone on naive timestamps.

## Reconciliation rules

| Check | HG | RN/NP |
|-------|----|-------|
| Off days | Full-day `time_off` row on that weekday | `off_day` in ext; that weekday absent from recurring `byday` |
| Work hours | Per-block `start_time`/`end_time` in CSV timezone | Single block `start_time`-`end_time` in `timezone_key` |
| Timezone | CSV `timezone_key`; align with `users.timezone_key` | Set `timezone_key` to staff's real zone |
| Source filter | `virtual_hg_schedule_csv` | `one_time_rn_schedule_repair_2026_06` or psych NP source |

When the user provides a weekly grid image, map each weekday's blocks to source
config rows before proposing writes.

## Reporting template

```markdown
## [Name] schedule template reconciliation

Staff group: HG | RN | Psych NP
Email: …

| Day | Expected | Source config | Prod DB | Match? |
|-----|----------|---------------|---------|--------|
| Mon | … | … | … | yes/no |

Timezone: user `…` | series ext `…`

Action needed: none | update config | dry-run update | apply after approval
```
