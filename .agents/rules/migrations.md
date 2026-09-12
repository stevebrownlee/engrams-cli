---
trigger: glob
globs: backend/**/*migrations/*.exs
description: "Ecto migration patterns and conventions for database schema changes"
---


<rules>

<rule name="use-core-migration">
<instructions>
ALWAYS use `Core.Migration` instead of `Ecto.Migration` in all migration files.

Core.Migration provides:

- Automatic schema search path setup via `after_begin`
- Helper functions for common field patterns
- Consistent migration structure across the codebase
  </instructions>

<examples>

<example name="correct-core-migration">
defmodule Core.Repo.Migrations.CreateUsers do
  use Core.Migration

def change do
create table(:users) do
add :email, :text, null: false
standard_fields()
end
end
end
</example>

<example name="forbidden-ecto-migration" type="forbidden">
defmodule Core.Repo.Migrations.CreateUsers do
  use Ecto.Migration  # WRONG - use Core.Migration
  
  def change do
    # ...
  end
end
</example>

</examples>
</rule>

<rule name="core-migration-helpers">
<instructions>
Use Core.Migration helper functions for common field patterns to ensure consistency:

1. **standard_fields(opts \\ [])** - Adds common fields to most tables:
   - `org_id`: references(:orgs), null: false, default: 1
   - `created_by_id`: references(:users)
   - `updated_by_id`: references(:users)
   - `timestamps(default: fragment("now()"))`
   - Use `skip_org: true` for join/bridge tables AND for all FH tenant tables (FH is single-tenant)

2. **soft_deletes()** - Adds soft delete tracking:
   - `deleted_at`: utc_datetime_usec
   - `is_deleted`: boolean (generated column based on deleted_at IS NOT NULL)
   - `deleted_by_id`: references(:users)

3. **created_updated_by_ids(opts \\ [])** - Just the audit fields without org_id/timestamps:
   - `created_by_id`: references(:users)
   - `updated_by_id`: references(:users)
   - Options: `created_by_id_null`, `updated_by_id_null`

4. **lku_constraint(source_table, source_column, ref_table, ref_column \\ "key")** - Foreign key constraint for Core LKU tables, which use a composite `{key, org_id}` primary key. Use only for Core tables. For FH LKU tables (`:text` key only, no org_id), use a plain `references()` FK instead.

   </instructions>

<examples>

<example name="correct-core-standard-fields">
create table(:team_members) do
  add :user_id, references(:users, prefix: "core"), null: false
  add :team_id, references(:teams, prefix: "core"), null: false
  add :role_key, :text, null: false
  
  standard_fields()  # Core table: Adds org_id, created_by_id, updated_by_id, timestamps
end
</example>

<example name="correct-fh-standard-fields">
create table(:toc_intakes, prefix: "fh") do
  add :patient_id, references(:patients, prefix: "core"), null: false

standard_fields(skip_org: true) # FH table: never include org_id
end
</example>

<example name="correct-soft-deletes">
alter table(:appointments) do
  soft_deletes()  # Adds deleted_at, is_deleted, deleted_by_id
end
</example>

<example name="correct-core-lku-fk">
# Core: use lku_constraint for composite {key, org_id} FK
lku_constraint(:appointments, :status_key, :appointment_lku_statuses)
</example>

<example name="correct-fh-lku-fk">
# FH: plain references() — no org_id in LKU tables
add :disposition_key,
    references(:toc_lku_dispositions, column: :key, type: :text, prefix: "fh")
</example>

</examples>
</rule>

<rule name="migration-conventions">
<instructions>
1. Group all related DB changes into a single migration file
2. Create migrations with: `cd backend && mix ecto.gen.migration descriptive_name`
3. Use :text for unconstrained string columns in migrations; use :string with an explicit size: only when a bounded VARCHAR(n) is intentional
4. Use :jsonb for JSON columns (maps to :map in schemas)
5. Always set default timestamps: timestamps(default: fragment("now()"))
6. Prefer `cd backend && mix release.deploy` to run `Core.Release.deploy/0`: full migration pipeline (Core + FH + `extra_migrations` such as `staging_migrations`) plus `sync_config_data` and the `sync_prompts` hook, matching production deploy database writes. Plain `mix ecto.migrate` only follows the default Core migration path and does not run config sync.
7. Use `cd backend && mix reset_db` to reset and migrate the dev database
8. Use `cd backend && mix reset_db_test` to reset and migrate the test database
</instructions>
</rule>

<rule name="migration-workflow">
<instructions>
Always generate migrations with the CLI. Never create migration files manually.

1. **Core** (`apps/core/priv/repo/migrations`): `cd backend && mix ecto.gen.migration descriptive_name --repo Core.Repo`

   Use for **platform-wide** work: DDL for shared `core` schema objects, and **data migrations that apply the same way for every tenant/org** (or that are not tenant-specific).

2. **FH / Firsthand** (`apps/fh/priv/repo/migrations`): `cd backend && mix ecto.gen.migration descriptive_name --repo Core.Repo --migrations-path apps/fh/priv/repo/migrations`

   Still **`Core.Repo`** and **`Core.Migration`** (same DB and `core` search path for SQL that targets `core.*`). Use for:
   - DDL for tables in the **`fh`** schema, and
   - **Tenant-specific data migrations** — including **`INSERT`/`UPDATE`/`DELETE` of tenant-owned rows in `core` tables** (e.g. org-scoped reference data such as `payer_markets` for Firsthand only).

   **Do not** place tenant-only reference or seed data in Core migrations **only** because the physical table lives under the `core` schema. Core owns the table definition; Firsthand owns the row data for that tenant.

3. Replace `use Ecto.Migration` with `use Core.Migration`
4. Fill in `change/0` (or `up`/`down` when needed)

Manual file creation bypasses timestamp and collision safeguards.
</instructions>

<examples>

<example name="correct-core-migration-workflow">
$ cd backend && mix ecto.gen.migration contact_form_registry --repo Core.Repo
# * creating backend/apps/core/priv/repo/migrations/20260228070054_contact_form_registry.exs

# Generated stub (edit this):

defmodule Core.Repo.Migrations.ContactFormRegistry do
use Ecto.Migration # change to Core.Migration

def change do
end
end

# Corrected:

defmodule Core.Repo.Migrations.ContactFormRegistry do
use Core.Migration

def change do
create table(:contact_form_registry) do # ...
soft_deletes()
standard_fields()
end
end
end

</example>

<example name="correct-fh-migration-workflow">
$ cd backend && mix ecto.gen.migration iprs_upd --repo Core.Repo --migrations-path apps/fh/priv/repo/migrations
# * creating backend/apps/fh/priv/repo/migrations/20260306172314_iprs_upd.exs
</example>

<example name="tenant-data-in-core-table-fh-path">
# Firsthand-only rows in a `core` table (still Core.Repo + Core.Migration):
$ cd backend && mix ecto.gen.migration add_acme_payer_markets --repo Core.Repo --migrations-path apps/fh/priv/repo/migrations
# Implement `up`/`down` with raw SQL or `execute` against `payer_markets`, `payers`, etc.
</example>

</examples>
</rule>

</rules>
