---
name: generate-sql
description: >
  Apply whenever generating, writing, or suggesting SQL statements for this project's
  Postgres database. Ensures every table reference is schema-qualified with the correct
  Postgres schema prefix (core, <tenant>, or staging).
---

# SQL Schema Prefix

When generating **any** SQL — whether in a SELECT, INSERT, UPDATE, DELETE, CTE, subquery,
JOIN, or ad-hoc diagnostic query — you MUST prefix every table name with its Postgres
schema. This project does **not** use the default `public` schema.

## Schema Mapping

Determine the correct schema by checking which Ecto app owns the table's schema module:

| Postgres Schema | Ecto App | Base Schema Module | Description / Tables |
|---|---|---|---|
| `core` | `core` | `use Core.Schema` | Platform tables (`core.workflows`, `core.patients`, `core.users`, `core.teams`, `core.orgs`) |
| `<tenant>` | `<tenant>` | `use <Tenant>.Schema` | Tenant-specific domain tables (e.g. `<tenant>.wf_engagement_summaries`, `<tenant>.workflow_task_extensions`) |
| `staging` | `<tenant>` | `@schema_prefix "staging"` | Data migration / staging tables (`staging.*`) |
### Rules

1. **Always qualify** — Write `core.workflows`, never bare `workflows`.
2. **Cross-schema joins are normal** — e.g. `core.workflows w JOIN <tenant>.wf_engagement_summaries s ON s.workflow_id = w.id`.
3. **When unsure**, check the schema module's `@schema_prefix` or its `use Core.Schema` / `use <Tenant>.Schema` declaration to determine the prefix.
4. **Lookup tables** follow their parent schema — `core.workflow_lku_statuses`, `core.workflow_task_lku_types`, etc.
5. **Config data tables** also live in `core` — `core.workflow_lku_types`, `core.appointments_lku_statuses`, etc.
6. **Patient Privacy & Identification** — Never query patient names, addresses, or phone numbers. The `enterprise_patient_id` column is the ONLY value that can be used to identify a patient.

### Example — Correct

```sql
SELECT p.enterprise_patient_id, w.status_key, s.call_attempts
FROM core.workflows w
JOIN core.patients p ON p.id = w.patient_id
JOIN <tenant>.wf_engagement_summaries s ON s.workflow_id = w.id
WHERE w.type_key = 'engagement';
```

### Example — Incorrect (missing prefixes)

```sql
-- ❌ DO NOT write unqualified table names or query patient names/PHI
SELECT p.enterprise_patient_id, w.status_key, s.call_attempts
FROM workflows w
JOIN patients p ON p.id = w.patient_id
JOIN wf_engagement_summaries s ON s.workflow_id = w.id
WHERE w.type_key = 'engagement';
```
