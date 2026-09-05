---
trigger: glob
globs: backend/apps/**/*.ex,backend/apps/**/*.exs
description: "Elixir and Phoenix language conventions for the backend"
---


<context>
This is an Elixir/Phoenix umbrella application with a multi-tenant architecture.
- `backend/apps/core` and `backend/apps/core_web`: Platform-level logic shared across all tenants
- `backend/apps/fh` and `backend/apps/fh_web`: Tenant-specific business logic (Firsthand)
- Business logic belongs in Context modules (e.g., `Core.Teams`, `Core.Workflows`). Controllers and Channels are thin wrappers that delegate to contexts.
</context>

<rules>

<rule name="idiomatic-elixir">
<instructions>
Write idiomatic Elixir code that follows language best practices:
1. Prefer small, composable functions over large monolithic ones
2. Follow the "let it crash" philosophy - don't defensively handle every error
3. Design function signatures for composability - put the "subject" first for pipes, match callback signatures for Multi/Enum usage
</instructions>
</rule>

<rule name="error-handling">
<instructions>
1. Functions should return `{:ok, result}` or `{:error, reason}` tuples
2. NEVER use `try`, `catch`, or `rescue` - let processes crash (OTP supervision handles recovery)
3. Use `with` statements for sequential happy-path pipelines where any step failing should bail out. Use `case` for branching/fallback logic (e.g., "try A, fall back to B"). Never use `with/else` as a substitute for `case` — if the `else` branch does real work rather than just normalizing an error, it should be `case`.
4. In controllers: let the fallback controller handle all errors - never render errors manually
5. Use bang versions (`Repo.update!`) when failure is unexpected - non-bang without error handling causes silent failures
6. **NEVER use `Enum.each` when the callback performs DB writes or returns `{:ok, _}` / `{:error, _}` tuples** — `Enum.each` discards return values, so errors pass silently. Use `Enum.map` to collect results and check them afterward.
7. Avoid nesting `with` inside another `with` or conditional — refactor into smaller Multi steps or separate function heads
</instructions>

<examples>

<example name="correct-with-pattern">
# with = sequential pipeline, bail on any error
with {:ok, resource} <- Context.create_resource(params, user.id) do
  conn |> put_status(:created) |> render("show.json", resource: resource)
end
</example>

<example name="correct-case-fallback">
# case = branching/fallback, not with/else
case find_by_tin(row.tin, org_id) do
  {:ok, contact} -> {:ok, contact}
  _ -> find_by_npi(row.npi, org_id)
end
</example>

<example name="forbidden-with-else-fallback" type="forbidden">
# Don't use with/else for fallback — use case
with {:ok, contact} <- find_by_tin(row.tin, org_id) do
  {:ok, contact}
else
  _ -> find_by_npi(row.npi, org_id)  # This is a branch, not error recovery
end
</example>

<example name="forbidden-each-with-writes" type="forbidden">
# NEVER use Enum.each for fallible operations — errors are silently discarded
Enum.each(rows, fn row ->
  create_task(row)  # returns {:ok, _} or {:error, _}, both discarded!
end)
</example>

<example name="correct-map-and-check-results">
results = Enum.map(rows, fn row -> create_task(row) end)

if Enum.any?(results, &match?({:error, _}, &1)),
  do: {:error, :partial_failure},
  else: :ok
</example>

<example name="correct-repo-helper">
Repo.get_ok(User, id)
</example>

<example name="forbidden-try-rescue" type="forbidden">
def get_user(id) do
  try do
    Repo.get!(User, id)
  rescue
    Ecto.NoResultsError -> {:error, :not_found}
  end
end
</example>

</examples>
</rule>

<rule name="map-key-types">
<instructions>
Before writing helpers that access or modify map keys, check the actual call sites to determine the key type. Do not write polymorphic helpers that handle both atom and string keys "just in case."
1. Controller/JSON params are ALWAYS string-keyed maps - use `Map.put(attrs, "field", value)` directly
2. Internal Elixir maps use atom keys - use `Map.put(attrs, :field, value)` directly
3. The existing `get_field/2` helper handles reads across key types; for writes, use the known key type
4. Never build cond/case logic to "detect" key types at runtime - this means you haven't traced the data flow
</instructions>
</rule>

<rule name="server-and-testing">
<instructions>
1. Never start the server manually - it runs in the `Elixir Backend` terminal
2. Never run `iex` - use `cd backend && mix compile` to check compilation
3. The server auto-recompiles on the next web request
4. Run `cd backend && mix compile --warnings-as-errors --force`
5. Run `cd backend && mix test` and fix any failing tests
6. Run `cd backend && mix credo --strict`
</instructions>
</rule>

<rule name="audit-fields">
<instructions>
For schemas with `created_by_id` and `updated_by_id` fields:
1. On CREATE: Set both `created_by_id` and `updated_by_id` to the current user
2. On UPDATE: Always set `updated_by_id` to the current user
3. In controllers, use a single `user_id` variable when setting both fields
4. Ending a period (setting `end_at`) is NOT deletion - do not set `deleted_by_id`
</instructions>

<examples>

<example name="correct-audit-fields">
user_id = conn.assigns.user.id
params |> Map.put("created_by_id", user_id) |> Map.put("updated_by_id", user_id)
</example>

<example name="forbidden-missing-audit" type="forbidden">
def update(conn, %{"id" => id, "resource" => params}) do
  Context.update_resource(id, params)  # Missing updated_by_id!
end
</example>

</examples>
</rule>

<rule name="authz-at-boundary">
<instructions>
Authorization invariants must be enforced at the API boundary (controller or plug), not only in the client. Hiding options in the frontend UI (filtering a list, disabling a button) is a UX convenience, not a security control.

1. If a `@doc` or business rule says "only X can do Y" or "cannot target Z", there must be a server-side check in the controller's `with` chain or a dedicated plug that returns `{:error, :unauthorized}`.
2. Preload the data needed for the check (roles, permissions, ownership) before the authorization step — do not assume the frontend filtered it correctly.
3. Cover authorization boundaries with controller tests proving the forbidden case returns `403`.
</instructions>

<examples>

<example name="correct-server-side-guard">
# Controller enforces "cannot impersonate an admin" server-side
with :ok <- require_impersonator_permission(impersonator.id),
     {:ok, target_user} <- Accounts.get_user_by_email(email),
     :ok <- require_non_admin_target(target_user),
     ...
</example>

<example name="forbidden-client-only-guard" type="forbidden">
# BAD — only the frontend filters admin users from the list;
# the API still accepts any email, so a direct POST bypasses the restriction
with :ok <- require_impersonator_permission(impersonator.id),
     {:ok, target_user} <- Accounts.get_user_by_email(email),
     # no server-side role check on target_user!
     ...
</example>

</examples>
</rule>

<rule name="no-typespecs">
<instructions>
Do not add @spec annotations to functions. The Elixir type system is under active development and specs are noise in this codebase. If elixirLS or autocomplete suggests a spec, remove it.
</instructions>
</rule>

<rule name="pattern-matching">
<instructions>
Use pattern matching to reduce nesting depth and improve readability.
1. Prefer function-head pattern matching when one branch is trivial (`nil`, `:ok`, `[]`, etc.) and another branch contains heavier logic; when branches are similarly complex, prefer `if`/`cond`/`case` for readability
2. Simple nil-check function heads (one-liners like `defp render_user(nil), do: nil`) are preferred when each clause is trivial
3. Never nest a `with` chain inside `cond`/`if` — split into multiple function heads or extract the inner logic into its own function
</instructions>
</rule>

<rule name="module-organization">
<instructions>
1. When a context module grows large, split by subdomain (e.g., `Workflows` → `WorkflowEvents`, `WorkflowQueues`)
2. Core vs FH boundary: `Core` contains generic infrastructure (workflows, scheduling, teams). Business-specific lookup types, values, and domain logic belong in `FH`
3. Keep list endpoints lightweight — only preload what the list view needs. Detail endpoints can have the kitchen sink
</instructions>
</rule>

<rule name="no-defensive-coding">
<instructions>
Do not add defensive guards, redundant function clauses, or impossible fallbacks. Verify how the callee behaves, then call it unconditionally when it is safe.

**Safe to call without list-length guards**
- `Repo.insert_all`, `Enum.map`, `Enum.each`, and similar are no-ops on `[]` — never wrap with `if list != []` / `unless Enum.empty?`
- Do not use a `[]` function head (e.g. `def run([]), do: :ok`) when the main clause already handles `[]` (e.g. `Enum.each` on `[]`). Reserve a `[]` clause only when empty truly needs a different code path.

**`Repo.update_all`**
- Empty `where` (no rows) is safe — returns `{0, nil}`
- Empty `set:` raises `ArgumentError` — guard with `if updates != []` or a function head before calling `Repo.update_all`

**Schema and boundaries**
- Do not guard nil for `NOT NULL` columns or add `|| fallback` where the schema guarantees a value
- Do not invent defaults for required inputs — require them at the boundary (pattern match, controller params)
- Before any guard or fallback, trace the data flow and confirm the case can occur
</instructions>

<examples>

<example name="forbidden-empty-guard" type="forbidden">
if records != [] do
  create_records(records)
end
</example>

<example name="forbidden-empty-function-head" type="forbidden">
# Same violation in function-head form — Enum.each on [] is already a no-op
def run([]), do: :ok

def run(items) do
  Enum.each(items, &process/1)
end
</example>

<example name="correct-unconditional-call">
create_records(records)
</example>

</examples>
</rule>

<rule name="trace-before-pattern-match">
<instructions>
Before writing any function clause that pattern-matches on externally-shaped data (Oban job args, controller params, pubsub/broadcast messages, webhook payloads), trace the data structure to its origin and verify the exact shape.

**For Oban workers specifically:**
- Job args are built by the code that calls `Worker.new(%{...})` or `Oban.insert_all`
- The `payload` key and top-level keys (like `org_id`, `event_id`) are separate — find the dispatch site and read the exact map structure before writing `perform/1` pattern matches
- A pattern match that silently misses (falls through to another clause or raises FunctionClauseError) will cause test failures that look like "wrong clause matched" — always check dispatch site first

**For controller params:**
- Params are always string-keyed maps — use `"key"` not `:key` in pattern matches
- Nested params come from the request body shape — check the frontend API call or controller test to confirm nesting before pattern matching
</instructions>

<examples>

<example name="correct-oban-trace">
# Before writing perform/1, find where Worker.new is called:
# Core.SentinelEvents dispatches: module.new(%{event_id: id, payload: payload, org_id: org_id})
# So the args shape is: %{"event_id" => _, "org_id" => _, "payload" => %{...}}

def perform(%Oban.Job{args: %{"org_id" => org_id, "payload" => %{"patient_id" => patient_id}}}) do
  # correct — org_id is top-level, patient_id is inside payload
end
</example>

<example name="forbidden-untraced-pattern" type="forbidden">
# WRONG — wrote pattern from assumption, not from tracing the dispatch site
def perform(%Oban.Job{args: %{"payload" => %{"patient_id" => patient_id, "org_id" => org_id}}}) do
  # org_id is NOT inside payload — this clause will never match
end
</example>

</examples>
</rule>

<rule name="test-factory-discipline">
<instructions>
1. **Never hardcode numeric IDs in test data.** Always use factory-generated values. Before introducing a custom ID (e.g. `enterprise_id = 100`), check whether the factory already generates a unique value via `sequence` — if it does, use the factory directly and read the value back from the inserted struct.
2. When a test needs a relationship between two records (e.g., patient and staging row both keyed on `enterprise_patient_id`), insert the parent first with the factory, then derive the child's key from the inserted struct: `insert(:child, fh_id: patient.enterprise_patient_id)`.
3. Do not create wrapper helpers that accept hardcoded IDs just to avoid reading from the factory — this introduces fragility. Prefer `insert(:factory_name)` and access fields on the result.
</instructions>

<examples>

<example name="forbidden-hardcoded-id" type="forbidden">
enterprise_id = 100  # fragile — could collide with factory sequence
patient = insert(:patient, enterprise_patient_id: enterprise_id)
insert(:staging_row, fh_id: enterprise_id)
</example>

<example name="correct-factory-derived">
patient = insert(:patient)  # factory generates unique enterprise_patient_id via sequence
insert(:staging_row, fh_id: patient.enterprise_patient_id)  # derived from actual struct
</example>

</examples>
</rule>

<rule name="llm-code-quality">
<instructions>
Code patterns are training data — LLMs reproduce what they see. Prefer the better pattern even when a less-ideal one "works fine."
1. Remove AI-generated artifacts: overly verbose comments on obvious code, unnecessary `@doc` on trivial functions, and defensive handling of impossible cases
2. Prefer consistency with established project conventions instead of introducing alternate patterns in generated code
</instructions>
</rule>

</rules>
