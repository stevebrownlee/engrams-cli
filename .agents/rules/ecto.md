---
trigger: glob
globs: backend/apps/**/*.ex,backend/apps/**/*.exs
description: "Ecto queries, schemas, and Multi patterns"
---


<context>
This codebase uses Ecto with PostgreSQL in a multi-tenant (org-scoped) umbrella app.

Custom Repo helpers: `Repo.one_ok/1`, `Repo.get_ok/2`, `Repo.get_by_ok/2` return `{:ok, record}` or `{:error, :not_found}`. Use these instead of manual nil-checking with case statements.
</context>

<rules>

<rule name="ecto-queries">
<instructions>
1. NEVER preload associations that aren't needed — trace the immediate caller and only preload what it directly accesses. Do not cargo-cult preloads from similar functions.
2. Never preload associations while mapping over a list (N+1 query problem)
3. Prefer `select:` for specific fields over loading entire schemas
4. **Use named bindings (`as:`) when composing queries across function boundaries.** When a base query is returned by a helper function and then piped into `|> join`, `|> where`, or `|> select`, all joins must use `as:` and all downstream clauses must reference bindings by name (`[wf: wf, care_level: cp]`). Positional bindings (`[wf, cp, p]`) silently break if the base query's join order changes. Self-contained `from()` blocks with inline `where:`/`select:` are fine with positional bindings since the join order is visible and local.
5. Use `assoc()` for joins when possible instead of explicit `on:` clauses
6. Prefer `WHERE EXISTS` over `JOIN + DISTINCT` when filtering by related records
7. **NEVER put a DB query inside `Enum.map`, `Enum.each`, or `Enum.flat_map`** — this is always an N+1. Before writing a loop that processes items, extract all needed keys, bulk-fetch with `WHERE field IN ^ids`, build a lookup map with `Map.new`, then use the map in the loop.
8. Ecto auto-casts types — don't manually parse integers for query params (e.g., no `Enum.map(ids, &String.to_integer/1)` before `where: field in ^ids`)
</instructions>

<examples>

<example name="correct-named-bindings-composed">
# Base query helper names all joins with as:
defp active_workflows_query do
  from(wf in Workflow,
    as: :wf,
    join: cp in ContactPeriod,
    as: :care_level,
    on: cp.contact_id == wf.contact_id and cp.period_type_key == "care_level",
    join: p in Patient,
    as: :patient,
    on: p.id == wf.patient_id,
    where: not wf.is_deleted
  )
end

# Callers compose with named bindings — safe regardless of base query join order
active_workflows_query()
|> join(:left, [wf: wf], last_hc in subquery(last_hc_query),
  as: :last_event,
  on: last_hc.patient_id == wf.patient_id
)
|> where([care_level: cp, last_event: last_hc], cp.period_value_key in ^@levels)
|> select([wf: wf, patient: p, last_event: last_hc], %{id: wf.id, care_team_id: p.care_team_id})
</example>

<example name="forbidden-positional-on-composed-query" type="forbidden">
# Positional bindings on a composed query — breaks if base query join order changes
active_workflows_query()
|> join(:left, [wf, cp, p], last_hc in subquery(last_hc_query),
  on: last_hc.patient_id == wf.patient_id
)
|> where([wf, cp, p, last_hc], cp.period_value_key in ^@levels)
|> select([wf, cp, p, last_hc], %{id: wf.id, care_team_id: p.care_team_id})
</example>

<example name="correct-exists-subquery">
has_participant = from(lp in LogicalParticipant,
  where: lp.appointment_id == parent_as(:appt).id,
  where: lp.tag_id == ^tag_id)

from(a in Appointment, as: :appt, where: exists(has_participant)) |> Repo.all()
</example>

<example name="forbidden-join-distinct" type="forbidden">
# Use EXISTS instead of join + distinct for filtering
from(a in Appointment,
  join: lp in LogicalParticipant, on: lp.appointment_id == a.id,
  where: lp.tag_id == ^tag_id,
  distinct: true)
</example>

<example name="forbidden-query-in-loop" type="forbidden">
# NEVER query inside a loop — even with a fallback branch
Enum.each(rows, fn row ->
  contact = Repo.one(from c in Contact, where: c.tin == ^row.tin)  # N+1!
  process(contact)
end)
</example>

<example name="correct-lookup-map">
# Bulk-fetch all needed records before the loop, build a lookup map
tins = Enum.map(rows, & &1.tin) |> Enum.reject(&is_nil/1)
contacts_by_tin =
  from(c in Contact, where: c.tin in ^tins)
  |> Repo.all()
  |> Map.new(&{&1.tin, &1})

Enum.each(rows, fn row ->
case Map.get(contacts_by_tin, row.tin) do
nil -> :skip
contact -> process(contact)
end
end)
</example>

<example name="correct-existing-lookup-map">
tag_ids = Enum.map(items, & &1.tag_id) |> Enum.uniq()
users_by_tag = from(ut in UserTag, where: ut.tag_id in ^tag_ids, select: {ut.tag_id, ut.user_id})
  |> Repo.all() |> Enum.group_by(&elem(&1, 0), &elem(&1, 1))

Enum.flat_map(items, fn item -> Map.get(users_by_tag, item.tag_id, []) end)
</example>

</examples>
</rule>

<rule name="ecto-functional-queries">
<instructions>
Write **new** Ecto queries in functional (pipe) style, not macro keyword style.

1. **Build with pipes** — start from the schema module or an existing query, then `|> where`, `|> join`, `|> select`, `|> group_by`, `|> order_by`, `|> update`. Do not use `from(Schema, where: ..., join: ...)` for multi-clause queries.
2. **One clause per pipe** — prefer several `|> where([binding], ...)` calls over one long clause.
3. **Positional bindings — list only what you use.** Ecto allows omitting unused bindings at the end. Filter on the root table only → `where([pc], ...)`, not `where([pc, _bridge, _pi], ...)`. Need a later binding → include earlier slots with `_` (e.g. `select([pc, _, pi], ...)`).
4. **Composed across functions** — still use named bindings (`as:`) per `ecto-queries` rule #4; functional pipes do not replace that.
5. **Reusable filter expressions** — extract with `dynamic/2` and interpolate via `^` in `where`/`having`. Do not call plain Elixir functions inside query macros (`where`, `select`, `having`, `update`); they are not valid query expressions.
6. **Bulk updates** — `Schema |> join(:inner, ..., subquery(rollup), ...) |> update(...) |> Repo.update_all([])`. Use `join:` (not `left_join:`) when joining a subquery in `update_all` — PostgreSQL `UPDATE ... FROM` restriction (see `RecalculateTaskWeightsWorker`).
7. **Fragments and raw SQL** — prefer Ecto structure; use `fragment/1` for PostgreSQL-specific pieces (`bool_or`, `bool_and`, `CASE` in `update` set). Reach for `Repo.query!/1` only when the query cannot be expressed cleanly otherwise.
8. **Macro `from` exceptions** — short one-offs: `exists`/`parent_as` subqueries, tiny preload subqueries, or a single-clause query. Default to pipes for everything else.
</instructions>

<examples>

<example name="correct-functional-query">
defp tbd_categories_query(last_id) do
  AcdPatientCategory
  |> where([pc], pc.acd_clinical_determination_key == "to_be_determined")
  |> where([pc], not pc.is_deleted)
  |> where([pc], pc.id > ^last_id)
  |> order_by([pc], asc: pc.id)
  |> select([pc], pc.id)
end
</example>

<example name="forbidden-macro-keyword-query" type="forbidden">
from(pc in AcdPatientCategory,
  where: pc.acd_clinical_determination_key == "to_be_determined",
  where: not pc.is_deleted,
  order_by: [asc: pc.id],
  select: pc.id)
</example>

<example name="correct-dynamic-having">
defp rollup_eligible_filter do
  dynamic(
    [_pc, _bridge, pi],
    fragment("bool_or(?)", pi.acd_clinical_determination_key == "applicable") or
      (count(pi.id) > 0 and
         fragment("bool_and(?)", pi.acd_clinical_determination_key == "not_applicable"))
  )
end

# ...
|> having(^rollup_eligible_filter())
</example>

<example name="forbidden-helper-in-macro" type="forbidden">
|> having([_pc, _bridge, pi], rollup_eligible?(pi))  # plain function — use dynamic/2 instead
</example>

<example name="correct-bindings-root-only">
# After two joins — only filter on pc:
|> where([pc], pc.id in ^category_ids)
|> group_by([pc], pc.id)
# Still need pi in select:
|> select([pc, _, pi], %{id: pc.id, flag: fragment("bool_or(?)", pi.field == "x")})
</example>

</examples>
</rule>

<rule name="schema-org-requirements">
<instructions>
## Core tables (`backend/apps/core`)

1. Every Core table MUST have a reference to Org, except for:
   - Join tables / bridge tables (many-to-many relationship tables)
   - The Org table itself
2. Core Lookup (LKU) tables MUST use a composite primary key of `{key, org_id}`
   - This ensures lookup values are scoped per organization
   - Use `@primary_key false` and define the composite key explicitly

## FH tenant tables (`backend/apps/fh`)

3. FH tables MUST NOT have `org_id` — FH is permanently single-tenant
4. FH Lookup (LKU) tables use a simple `:text` primary key (`key` only, no `org_id`)
   - The migration column is `:text`; the schema field is `:string`
   - Use a plain `references()` FK instead of `lku_constraint/4` when referencing FH LKU tables
</instructions>

<examples>

<example name="correct-core-lku-schema">
@primary_key false
schema "status_lku" do
  field :key, :string, primary_key: true
  belongs_to :org, Core.Orgs.Org, primary_key: true
  field :label, :string
  timestamps()
end
</example>

<example name="correct-fh-lku-schema">
@primary_key false
schema "toc_lku_dispositions" do
  field :key, :string, primary_key: true
  field :label, :string
  timestamps()
end
</example>

</examples>
</rule>

<rule name="lku-associations">
<instructions>
Core LKU tables have composite primary keys (key + org_id). Always match on both fields.
FH LKU tables have a simple :string primary key (`:text` DB column, `:string` Ecto field)
1. Use explicit JOINs with `on:` matching both key and org_id (Core) or key only (FH)
2. Use `preload:` with the join binding to attach the LKU association
</instructions>

<examples>

<example name="correct-core-lku-join">
from(tm in TeamMember,
  left_join: r in TeamLkuRole, on: r.key == tm.role_key and r.org_id == tm.org_id,
  preload: [role: r])
</example>

<example name="correct-fh-lku-join">
from(ti in TocIntake,
  left_join: d in TocLkuDisposition, on: d.key == ti.disposition_key,
  preload: [disposition: d])
</example>

<example name="forbidden-lku-single-key" type="forbidden">
# Core LKU joins MUST match org_id — missing it allows cross-org data leaks
left_join: r in TeamLkuRole, on: r.key == tm.role_key  # Missing org_id!
</example>

</examples>
</rule>

<rule name="changeset-conventions">
<instructions>
1. The default `changeset/2` casts ALL schema fields: `cast(params, __schema__(:fields))`. Do not list fields explicitly unless there is a specific reason.
2. For specific operations (cancel, soft-delete, status change), consider a named changeset like `cancel_changeset/2` that casts only the relevant subset. This pattern is used sparingly today but preferred for new code with distinct operation types.
3. Do not add redundant validations that Ecto already handles (e.g., `validate_inclusion(:field, [true, false])` on a boolean column).
</instructions>

<examples>

<example name="correct-default-changeset">
def changeset(data \\ %__MODULE__{}, params) do
  data
  |> cast(params, __schema__(:fields))
end
</example>

<example name="correct-named-changeset">
@cancellation_fields ~w(status_key cancelled_at cancelled_by_id cancellation_reason_key)a

def cancel_changeset(data, params) do
data
|> cast(params, @cancellation_fields)
|> validate_required(@cancellation_fields)
end
</example>

<example name="forbidden-explicit-field-list" type="forbidden">
def changeset(data \\ %__MODULE__{}, params) do
  data
  |> cast(params, [:name, :status, :org_id, :created_by_id, :updated_by_id])  # Use __schema__(:fields)
end
</example>

</examples>
</rule>

<rule name="trust-the-database">
<instructions>
Rely on PostgreSQL constraints as the source of truth. Do not duplicate them in application code.
1. Do not add `validate_required` for fields that are already `NOT NULL` in the migration. The database will enforce it and Ecto will return a proper error.
2. Do not add application-level uniqueness checks — use unique indexes and let `unsafe_validate_unique` or `unique_constraint` handle it if needed.
3. Do not add `on_delete: :nothing` in migrations — it is the default.
4. Do not add application-level `Enum.reject` or `Enum.filter` to enforce invariants the database already guarantees via constraints.
5. Use `insert_all` with `on_conflict: :nothing` instead of check-then-insert patterns.
6. Specify `precision` and `scale` on decimal columns for money (e.g., `precision: 10, scale: 2`).
</instructions>
</rule>

<rule name="ecto-multi">
<instructions>
Pipeline structure:
1. Keep Multi steps as single-line calls to named functions - the transaction flow should be visible at a glance
2. Use captured named functions (`&function/2`) instead of anonymous functions in Multi.run
3. Design function signatures to match Multi callback args: `_repo, %{step_name: value}`
4. Include reads in the Multi for transactional consistency - don't fetch data outside then use inside

Context values:
5. For 1-2 simple values: `Multi.put(:key, value)` calls
6. For 3+ related values: `Multi.put(:ctx, %{...})` with a map

Function signatures:
7. Simple insert/update: return changeset, use `Multi.insert/update` with 1-arity function
8. Operations needing repo: 2-arity with `_repo`, use `Multi.run`
9. Pattern match ALL required params in function head — never `Map.get` with defaults for always-present values
10. Name functions called by Multi with `_ms` suffix; helpers use normal names

Data fetching:
11. Use `Multi.one` or `Multi.all` at the start of the pipeline instead of querying outside
12. Use `Multi.merge` for dynamic operations based on fetched data (preferred for new code)

Return values:
13. Use `Repo.transact_ok()` instead of `Repo.transact()` for Multi pipelines — it normalizes errors so the fallback controller handles them automatically
14. Extract a single key with `Repo.transact_ok(multi, return: :key)` — returns `{:ok, value}` directly. Use `with` only when post-processing the result (e.g., preloading)
15. Never manually pattern-match `{:error, _step, error, _changes}` — `transact_ok` does this for you
16. Exception: When frontend only invalidates queries, returning `{:ok, :success}` is acceptable
</instructions>

<examples>

<example name="correct-multi-pipeline">
Multi.new()
|> Multi.all(:items, items_query(org_id))
|> Multi.run(:process, &process_items/2)
|> Repo.transact_ok(return: :process)

defp process_items(\_repo, %{items: items}), do: {:ok, do_process(items)}
</example>

<example name="correct-fetch-in-multi">
def remove_member(member_id, deleted_by_id) do
  Ecto.Multi.new()
  |> Ecto.Multi.one(:member, member_query(member_id))
  |> Ecto.Multi.put(:user_id, deleted_by_id)
  |> Ecto.Multi.run(:delete, &soft_delete_member_ms/2)
  |> Repo.transact_ok()
end
</example>

<example name="forbidden-manual-multi-error-handling" type="forbidden">
# Never manually strip Multi error tuples — use transact_ok
|> Repo.transact()
|> case do
  {:ok, %{foo: result}} -> {:ok, result}
  {:error, _step, changeset, _} -> {:error, changeset}
end
</example>

<example name="correct-changeset-function">
Ecto.Multi.insert(:period, &create_member_period/1)

defp create_member_period(%{member: member, user_id: user_id}) do
TeamMemberPeriod.changeset(%{
team_id: member.team_id,
user_id: member.user_id,
start_at: DateTime.utc_now()
})
end
</example>

<example name="forbidden-fetch-outside" type="forbidden">
def remove_member(member_id, deleted_by_id) do
  with {:ok, member} <- Repo.get_ok(TeamMember, member_id) do
    Ecto.Multi.new()
    |> Ecto.Multi.put(:member, member)  # Already fetched outside transaction!
  end
end
</example>

</examples>
</rule>

<rule name="org-id-scoping">
<instructions>
## Core context functions (`backend/apps/core`)

Every public Core context function that queries data MUST accept and filter by org_id. Controllers pass `conn.assigns.org_id`. Private helpers called only from org-scoped functions are exempt.

## FH context functions (`backend/apps/fh`)

FH context functions do NOT need an org_id parameter — FH tables have no org_id column and FH is permanently single-tenant.
</instructions>

<examples>

<example name="correct-core-org-scoping">
def list_items(user_id, org_id) do
  from(i in Item, where: i.user_id == ^user_id, where: i.org_id == ^org_id)
  |> Repo.all()
end
</example>

<example name="correct-fh-no-org-scoping">
def list_expenses(patient_id) do
  from(e in Expense, where: e.patient_id == ^patient_id)
  |> Repo.all()
end
</example>

<example name="forbidden-missing-org-filter-in-core" type="forbidden">
# Core context functions MUST filter by org_id
def list_items(user_id) do
  from(i in Item, where: i.user_id == ^user_id)
  |> Repo.all()
end
</example>

</examples>
</rule>


<rule name="use-generated-columns">
<instructions>
When a table has a `GENERATED ALWAYS AS (...) STORED` column (e.g., `users.full_name`, `contacts.full_name`), reference the column directly in queries. Do not recreate the derivation with `fragment("concat_ws(...)")` or string interpolation — the generated column is the canonical representation and may differ slightly from an ad-hoc reconstruction (e.g., `coalesce` vs bare concatenation, whitespace handling).
</instructions>

<examples>

<example name="correct-use-generated-column">
# full_name is GENERATED ALWAYS AS (coalesce(first_name,'') || ' ' || coalesce(last_name,'')) STORED
where(query, [user: u], ilike(u.full_name, ^search_pattern))
</example>

<example name="forbidden-recreate-generated-column" type="forbidden">
# Don't reconstruct what the DB already stores
where(query, [user: u],
  ilike(fragment("concat_ws(' ', ?, ?)", u.first_name, u.last_name), ^search_pattern))
</example>

</examples>
</rule>

<rule name="ddl-conventions">
<instructions>
1. `:bigint` is the default type for references — omit `type: :bigint`
2. Use `generated: "ALWAYS AS (...) STORED"` columns for derived booleans (e.g., `is_deleted` from `deleted_at`)
3. Use PostgreSQL exclusion constraints with `tsrange` for temporal non-overlap requirements
4. Don't create redundant indexes — a composite index `[:a, :b, :c]` covers leftmost-prefix queries on `[:a]` and `[:a, :b]`
5. Don't use `~s` sigils for SQL fragments when a regular string would do
</instructions>
</rule>

<rule name="seed-data">
<instructions>
1. Use `Repo.insert_all` for bulk seed data creation, not `Enum.each` with individual inserts
2. Split seed data into separate files by domain to avoid merge conflicts
</instructions>
</rule>


<rule name="user-preload-select">
<instructions>
When preloading `created_by`, `updated_by`, or `deleted_by` associations in list or detail queries,
use a named preload query with `select: struct(u, [...])` to keep payload small while still returning
a struct that `render_user_ref/1` can pattern-match on.

IMPORTANT: Use `select: struct(u, [...])`, NOT `select: map(u, [...])`.
`CoreWeb.Helpers.UserJSON.render_user_ref/1` guards on `is_struct` and returns `nil` for plain maps.

```elixir
defp user_preload do
  from(u in Core.Accounts.User,
    select: struct(u, [:id, :email, :first_name, :last_name, :full_name])
  )
end

# Used in context queries:
preload: [created_by: ^user_preload(), updated_by: ^user_preload()]
```

</instructions>
</rule>

</rules>
