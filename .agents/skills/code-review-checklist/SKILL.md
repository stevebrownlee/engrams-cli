---
name: code-review-checklist
description: Final review of code before PR
---

You are an expert senior full-stack code reviewer. Review ONLY the code changed on the current branch (use `git diff main...HEAD` to identify changed files and hunks).

## 1. Load project rules

Read these files before reviewing — they define what constitutes a violation:

**Always read:**

- `.agents/rules/review.md` — detailed review checklist with output format
- `.agents/rules/global.md` — project-wide coding rules
- `.agents/rules/clean-expressions.md` — expression clarity rules
- `.gemini/styleguide.md` — architecture and pattern rules enforced by the automated PR reviewer

**Read when Elixir files are in the diff:**

- `.agents/rules/elixir.md`
- `.agents/rules/ecto.md`
- `.agents/rules/migrations.md`

**Read when frontend files are in the diff:**

- `.agents/rules/frontend-core.md`
- `.agents/rules/frontend-components.md`
- `.agents/rules/frontend-data.md`
- `.agents/rules/frontend-architecture.md`

## 2. Review the diff

For every changed file, check compliance against ALL loaded rules. Don't just
check the rules you remember — re-read the rule files as you review each file.

## 3. Cross-module contract tracing

When a changed file **produces** data (sets search params, emits events, returns a
shaped object) that another module **consumes**, trace the contract end-to-end:

1. Identify the producer's output shape (keys, types, defaults)
2. Find the consumer(s) that read those keys — search the codebase
3. Verify key names, types, and nullability match on both sides
4. Flag any key the consumer reads that the producer never sets

Common producer → consumer pairs to check:

- Slot/resolver functions that build `searchParams` → page hooks that destructure them
- Backend view JSON fields → frontend Zod schemas (also covered by api-schema-concordance)
- Event emitters → event handlers
- Context function return shapes → controller/view expectations
- Navigation `rightParams` / `searchParams` → target page provider hooks
- Navigation target types (e.g. `StrandedTaskNavigationTarget`) → `navigatePage` option
  types — if the producer's type is narrower than the consumer's (e.g. flat
  `Record<string, string | number>` vs `Partial<RouteSearch>` which accepts nested
  objects like `rightParams`), the type blocks valid data from flowing through. Check
  that producer types are wide enough to carry the data consumers expect

## 4. Commonly missed patterns

These are patterns that repeatedly slip past review. Pay special attention:

### Elixir

- `Repo.*` calls inside `Enum.map`/`for`/comprehension bodies → N+1 query
- `Repo.*` calls inside changeset functions → side-effect in pure function
- `Enum.each` used for operations that return `{:ok, _}`/`{:error, _}` → silent failures
- Missing `updated_by_id` on update operations
- Multi-head functions without a catch-all fallback clause — if data can vary (e.g. during migrations, local dev, or future variants), a missing fallback raises `FunctionClauseError`. Flag when the caller uses `Enum.map` + `Enum.reject(&is_nil/1)` or similar and the function has no `defp fn(_, ...), do: nil`
- Ecto `where: field in ^list` when a more precise composite filter is available (e.g. `where: {t.team_id, t.role_key} in ^pairs`) — over-broad filters do unnecessary work and can return incorrect aggregates
- Ecto `where: field not in ^list` on a nullable column — SQL `NOT IN` evaluates to
  `NULL` when the column is `NULL`, silently excluding rows. Always add an explicit
  `is_nil(field) or field not in ^list` guard when the column allows NULL values
- `case`/function clauses that accept `is_map(x)` without validating required keys —
  if downstream code depends on specific map keys (e.g. `disposition_key`, `contact_phone_id`),
  pattern-match on those keys with guards (e.g. `when is_binary(key) and key != ""`)
  instead of blindly accepting any map. This prevents DB constraint violations from
  nil/empty values
- `Map.get(map, "key")` for simple optional key access — prefer `map["key"]` (Access
  syntax) which is more concise and idiomatic. Reserve `Map.get/3` for when a non-nil
  default is needed
- `{:ok, value} = SomeModule.from_iso8601(str)` in test helpers — prefer bang variants
  (`from_iso8601!`) which raise on invalid input and eliminate the intermediate tuple match
- **Non-idempotent workflow status transitions (`move_to_new_status`)** — calling status transition helpers like `Workflows.move_to_new_status` without guarding on the workflow's current `status_key` causes:
  1. Duplicate status periods and closed period timestamps violating `start_at < end_at` check constraints when events occur in quick succession
  2. State regression (e.g. pulling a workflow back to an earlier stage when it has already advanced or terminated)
  3. Spurious duplicate status change sentinel events.
  Always guard transitions with pattern-matches on the expected prior status (e.g. `%Workflow{status_key: "expected_status"}`) and return a clean `{:ok, :already_in_target_status}` (or `{:ok, :noop}`) for any other status.
- **Event/webhook handler over-triggering across multi-event flows** — when handling sentinel events or webhooks (e.g. `consent_signed`, `emr_visit_note_signed`, task completions), do NOT execute one-time lifecycle transitions merely because a global precondition is met. Check whether the specific event type currently being processed is the intended trigger for that transition, or check if the target entity has already progressed, preventing subsequent related events from re-triggering transitions.
- **Routing specialized form completions through generic task closure pipelines** — functions like `AdHoc.close_task/3` enforce validation rules (e.g. non-blank `note_human`). If a specialized form (like `EngagementConsentSignedForm`) completes without user notes or via backend event handlers, routing it through generic task-close functions will fail validation and leave tasks open. Check that task-close helpers match the payload contracts of all invoking forms.
- **Seed data vs migration schema conflicts** — when seed scripts use `Repo.insert_all(..., on_conflict: :nothing)`, migrations inserting legacy/stale rows (or running in separate app prefixes on fresh DBs) will cause stale rows to survive and block new seed rows from being inserted or used. Always ensure seeds deactivate or supersede legacy rows.

### React/TypeScript
- Page/view components containing state, queries, or navigation logic instead of delegating to a `useXxxProvider` hook
- `mutateAsync` calls without a `catch` handler → unhandled promise rejection
- Optional/nullable values passed to translation `t()` or string interpolation without `?? ''` fallback
- Sentinel string matching (`status === 'completed'`) instead of boolean flags (`status.isCompleted`)
- Frontend Zod schema fields not matching backend view fields changed in the same PR
- Boolean `useRef` guards in effects that should track the _value_ instead —
  `useRef(false)` set once blocks re-firing when the dependency changes;
  use `useRef<T | null>(null)` and compare against the current dependency value
- Pure factory functions called inside hook/component bodies (e.g. `createColumns()`)
  that return a stable result — hoist to module scope to avoid unstable references that reset TanStack Table state or trigger unnecessary re-renders
- `switch` on discriminated unions without a `default` clause — TypeScript allows `undefined` to leak as a return value when the declared return type is `T | null`; always add `default: return null` (or use exhaustive checking with `never`)
- Calling string methods (`.split()`, `.trim()`, `.toLowerCase()`) on URL search param
  values obtained via `useSearch({ strict: false })` or `as Record<string, string>`
  casts — the runtime value may be a number, array, or undefined despite the cast.
  Always wrap with `String(val)` before calling string methods, or validate with Zod first
- Unsafe `as` casts on mutation results (e.g. `result as { id?: number }`) — if the
  mutation hook is generically typed (e.g. `useRequestMutation<TData, TInput>`), the
  result is already typed as `TData`. Access properties directly instead of casting
- Navigation targets that call `navigatePage` to a page with right-panel views but
  omit `rightPanel` and `rightParams` — the destination page won't auto-open its
  drawer. When a slot resolver or navigation function targets a page that has
  `views: [{ slot: 'right', ... }]`, verify it sets `rightPanel: VIEW_ID` and
  `rightParams: { ... }` in `searchParams` so `RightPanelContentDetector` activates
- Redundant `!isLoading` guards in derived booleans (e.g.
  `const hasData = !isLoading && !!data?.length`) when the component is already
  wrapped in `OperationStateDisplay` or a `Suspense` boundary that gates the loading
  state. The loading check belongs in exactly one place — the wrapper. Duplicating it
  in the provider creates dead branches and obscures intent. Prefer `!!data?.length`
  and `data != null && data.length === 0`
- Multi-concern provider hooks that inline 15+ lines of self-contained logic (URL
  param parsing, deferred selection, complex derived state) instead of extracting a
  focused custom hook. If a block reads its own refs, has its own `useEffect`, and
  could be tested independently, it should be a separate `useXxx` hook in the same
  directory
- **Silent error swallowing in mutation/form submit handlers** — `mutateAsync(...).catch(() => undefined)` or empty `catch {}` blocks in task form footers and modal action buttons swallow backend validation errors (e.g. missing required fields, concurrency conflicts) without displaying a toast or user feedback, leaving forms in a broken or stuck state. Always surface errors via `toast.error` or form-level error banners.

## 5. i18n enforcement

**Every** user-facing string in a new or modified React component, provider hook, or
column definition must use `t()` from `useTranslation`. This is the single most
frequently missed issue in review.

**What counts as user-facing:**
- Page/view titles, subtitles, and header copy
- Empty-state messages (e.g. "No results found", "All clear")
- Column headers (`title` prop on `DataTableColumnHeader`)
- Badge/label text rendered in JSX
- Filter option labels and filter group labels
- Toast messages, confirmation dialogs, and error messages
- Pluralized text (use i18next `_one`/`_other` suffixes, not ternaries)
- `emptyMessage`, `filterLabel`, `placeholder` props passed to shared components

**How to check:**

1. For every changed `.tsx` / `.ts` file that renders JSX or returns strings consumed
   by JSX, scan for **bare English string literals** in JSX expressions, prop values,
   and template literals
2. Object literals whose values are English labels (e.g.
   `{ queue: 'Queue: No Workers' }`) must use `t()` — either call it inline or build
   the map inside a function that receives `t` as a parameter
3. Verify that new `t()` keys have corresponding entries in the locale JSON file
   under `frontend/src/i18n/locales/en/`. If a new key is used but not added to the
   locale file, flag it
4. Ternary pluralization (`count === 1 ? 'task' : 'tasks'`) must be replaced with
   i18next plural keys (`t('key', { count })` + `key_one` / `key_other` in locale)

**Exceptions** (do NOT flag these):

- Import paths and module IDs
- CSS class names
- `console.log` / `console.error` messages
- Test file assertions
- `data-testid` attributes
- Enum/constant keys used for logic, not display

## 6. Test coverage checks

**Backend:**

- Every new public context function (`def` in a context module) must have at least one test case in the corresponding `*_test.exs`. If the PR adds a new public function with no matching `describe` block in tests, flag it.
- Every new controller action must have:
  - A happy-path test
  - An authorization boundary test (verify 403 for unauthorized access)
  - A multi-tenant isolation test for list endpoints — insert data for a second org and
    assert the response only contains data for the requesting org's `org_id`

**Frontend:**

- New provider hooks that read URL search params should have the param → behavior
  contract verified (either via tests or explicit manual-test documentation in the PR).
- If any frontend tests were written, evaluate whether they test critical functionality that has corresponding backend tests. Recommend which tests are critical and which can be eliminated.

## 7. Output

Use the format specified in `.agents/rules/review.md`. For each finding, cite the specific rule file and rule name that was violated.

If the code is high-quality and idiomatic, keep the review brief and note what was done well.


