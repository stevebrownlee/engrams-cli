# Protocol: spec-format

**Status:** v1
**Loaded by:** `spec-composer`, `spec-reviewer`, `spec-json-builder`, `spec-narrator`
**Defines:** the structure every spec under `specs/` must follow

---

## Purpose

A PILOT spec is the source of truth for a feature. It is read by humans and
agents alike. Every downstream artifact — the progress file, the rationale
doc, the implementation, the commit messages, the PR description — traces
back to it. A spec that's vague produces a pipeline that's vague; a spec
that's specific produces work that's specific.

The format below is **layered**: a product layer (the "what and why") sits
on top of a technical layer (the "how"). The product layer is what a
non-engineer stakeholder would read to understand whether the feature meets
the need. The technical layer is what `spec-json-builder` reads to decompose the
work into phases.

Both layers are required. The product layer without the technical layer
leads to under-specified work; the technical layer without the product
layer leads to work that ships the wrong thing.

## File naming

```
specs/<ID>-<kebab-case-name>.md
```

- `<ID>` is 4-digit uppercase hexadecimal: `0001` through `FFFF`.
- `<kebab-case-name>` is lowercase, hyphen-separated, descriptive but brief.
- Examples: `0001-user-authentication.md`, `001A-password-reset-flow.md`,
  `00B7-export-to-csv.md`.

The `implementation-orchestrator` (orchestrator) assigns the next free ID when invoked via
`/spec-compose`. If creating a spec manually, use the next free ID — never
reuse an ID, even for a spec that was abandoned.

## Required sections

A valid spec has **exactly these top-level headings, in this order** (10 sections):

```markdown
# <Spec ID> — <Title>

## Summary

## Acceptance criteria

## Out of scope

## Open questions

## Architecture

## Implementation risks

## Data model

## API surface

## Dependencies

## Verification strategy
```

The first four sections are the **product layer**. The last six are the
**technical layer**. Both layers are required.

Each section is described below: what it must contain, what it must not
contain, and how `spec-reviewer` checks it at Gate 1.

---

## Product layer

### Summary

**Contains:** one to three paragraphs describing what this feature is and
why it exists. Written for a reader who knows the product but not this
spec's context. Names the user-facing behavior, not the implementation.

**Does not contain:** code, file paths, library names, or any
implementation detail.

**Spec-reviewer checks:**
- Length between 50 and 600 words
- No code blocks
- Mentions at least one user-facing outcome

### Acceptance criteria

**Contains:** a series of testable conditions in Given/When/Then format
that, when all true, mean the spec is fulfilled. Each criterion has a
unique ID and a short title, followed by a G/W/T block that describes
observable behavior — never internal state.

**Format:**

```markdown
### AC-1: Unauthenticated dashboard access redirects to login

**Given** a user with no active session
**When** they visit `/dashboard`
**Then** they are redirected to `/login?next=/dashboard`
**And** no dashboard content is rendered before the redirect

### AC-2: Successful login redirects to original destination

**Given** a user is on `/login?next=/billing/invoices`
**When** they submit valid credentials
**Then** they land on `/billing/invoices`, not on `/dashboard`

### AC-3: Invalid password preserves the email field

**Given** a user is on `/login`
**When** they submit a valid email with an invalid password
**Then** an error message is shown
**And** the email field retains its value
**And** the password field is cleared
```

**Naming rules:**
- IDs are sequential within the spec: `AC-1`, `AC-2`, ... up to `AC-99`
  (no zero-padding; no spec should have 100+ criteria)
- Titles are imperative, ≤ 80 characters, and unique within the spec
- IDs are never reused, even if a criterion is removed; new criteria use
  the next free integer (so historic references in commits and the
  rationale doc remain valid)

**Keywords:**
- `Given` — sets the precondition (one or more, joined with `And`)
- `When` — names the user action or trigger (typically exactly one)
- `Then` — names the observable outcome (one or more, joined with `And`)
- `But` is permitted as a negation alternative to `And` in `Then`

**Does not contain:** implementation hints ("uses bcrypt", "stores token in
Redis"). Those belong in the Architecture section.

**Why AC IDs:** other artifacts reference these IDs.
- `progress.json` phases reference AC IDs in their `acceptance` array
- The PR description's checklist references AC IDs
- The rationale doc (`--review`/`--paired` modes) explains which phase
  satisfies which AC by ID
- Commit messages may reference an AC ID when a phase exists primarily
  to satisfy one criterion

**Spec-reviewer checks:**
- At least 3 criteria
- Each criterion has the `### AC-N: <title>` heading format
- IDs are sequential starting at 1 with no gaps and no duplicates
- Each criterion contains at least one `Given`, exactly one `When`, and
  at least one `Then`
- No criterion contains code-like syntax that suggests implementation
- Each criterion is testable in principle (the reviewer asks: "could a
  test be written that asserts this?")

### Out of scope

**Contains:** a bulleted list of things explicitly *not* being built in
this spec. Each item is something a reasonable person might assume is part
of the feature but isn't. This section prevents scope creep during
implementation.

**Example:**

```markdown
- Password reset flow (separate spec, see `0003-password-reset-flow.md`)
- Social login providers (deferred to Q3)
- Multi-factor authentication
- Account recovery via email verification
```

**Spec-reviewer checks:**
- At least one item (an empty Out of scope is a smell — almost every
  feature has neighbors that aren't in scope; if none are listed, the
  spec author hasn't thought about boundaries)

### Open questions

**Contains:** a bulleted list of things the spec author is unsure about,
phrased as questions. Empty is acceptable only if every decision has
been made. If anything is genuinely uncertain, name it.

**Example:**

```markdown
- Should the rate limiter on `/login` use IP-based throttling, user-based,
  or both?
- What is the canonical session length? (Currently inconsistent across
  the auth-related specs.)
```

**Spec-reviewer checks:**
- If non-empty, each item is phrased as a question (ends with `?`)
- `spec-reviewer` flags this section in its report; the orchestrator may
  pause the pipeline if open questions exist, depending on severity

---

## Technical layer

### Architecture

**Contains:** the technical approach. Names the major components, the
patterns being used, and the trade-offs being made. Identifies which
existing parts of the codebase will be touched and which new parts will be
added. Explains why this approach was chosen over alternatives, briefly.

**Length:** typically 200-800 words. Long enough to settle the design;
short enough that nobody skips it.

**Example structure:**

```markdown
Authentication is handled by a new `lib/auth/` module that wraps an
existing session library (iron-session). New login and logout server
actions live in `app/(auth)/actions.ts`. The `/dashboard` route gets a
new `middleware.ts` that redirects unauthenticated requests to `/login`,
preserving the original path in a `?next=` query param.

We chose iron-session over JWT-in-cookie because the team already has
operational experience with it and the spec doesn't require stateless
sessions. We chose middleware-based redirect over a server-component
check because middleware applies before the page renders, avoiding a
flash of unauthenticated content.
```

**Spec-reviewer checks:**
- Names at least one specific file path or module
- Names at least one library or framework feature being used
- Explains at least one design choice ("we chose X over Y because...")

### Implementation risks

**Contains:** a bulleted list of known pitfalls, contract boundaries, and
commonly-violated patterns that apply to this spec's implementation. Each
risk item has a short **label**, a one-sentence description, and a
mitigation directive ("mitigate by..." or "verify by..."). Risks are drawn
from the project's review checklist and the spec author's knowledge of
the codebase.

**Categories to probe:**

- **Cross-module contracts** — producer/consumer mismatches: search params
  vs page hooks, backend view JSON fields vs frontend Zod schemas, event
  emitter shapes vs handler expectations, navigation `rightParams` vs
  target page provider types
- **Elixir pitfalls** — N+1 queries (`Repo.*` inside `Enum.map`), `Repo.*`
  calls in changeset functions, missing `updated_by_id` on updates, Ecto
  `NOT IN` on nullable columns, multi-head functions without catch-all,
  `Enum.each` swallowing `{:ok, _}`/`{:error, _}`, LKU key strings not
  present in `*Values.ex`
- **React/TS pitfalls** — `mutateAsync` without catch handler, bare strings
  missing `t()`, sentinel string matching instead of boolean flags, unstable
  factory functions called in hook bodies, `switch` on discriminated unions
  without `default`, unsafe `as` casts on mutation results, `useRef(false)`
  guards that block re-firing, page/view components containing state or
  query logic instead of delegating to a provider hook
- **i18n** — every user-facing string (titles, empty states, column headers,
  badges, toasts, filter labels) must use `t()`; pluralization via i18next
  `_one`/`_other` suffixes, not ternaries
- **Test coverage** — new public context functions need ExUnit tests, new
  controller actions need happy-path + auth boundary + multi-tenant
  isolation tests, new provider hooks with URL params need param→behavior
  verification

If the spec author genuinely cannot identify any risks, write
`_No known implementation risks._` — but this should be rare for any
non-trivial feature.

**Example:**

```markdown
- **N+1 on engagement loading**: The new query fetches engagements then
  maps over them to load dispositions. Mitigate by preloading dispositions
  in the initial query.
- **Zod schema drift**: Adding `priority` to the API response requires a
  matching field in the frontend Zod schema. Verify both sides update in
  the same phase.
- **i18n for filter labels**: The new filter dropdown labels must use
  `t()`, not bare strings. Verify by grepping the new component for bare
  English string literals.
- **Missing catch on mutateAsync**: The save handler will call
  `mutateAsync`. Mitigate by wrapping in try/catch or chaining `.catch()`.
```

**Spec-reviewer checks:**
- Section is present → `severe` if missing, category `risks-missing`
- Spec touches both backend and frontend (as indicated by the Architecture
  section) but risks list is empty or `_No known implementation risks._` →
  `moderate`, category `risks-empty-cross-stack`
- Risk item lacks a mitigation clause ("mitigate by" or "verify by") →
  `minor`, category `risks-no-mitigation`

### Data model

**Contains:** schemas, types, or table definitions for any new or modified
data. For typed languages, the actual type signatures. For databases, the
table/collection shape. Includes indexes, constraints, and relations
relevant to the feature.

**Example:**

```markdown
New Prisma model:

\`\`\`prisma
model Session {
  id          String   @id @default(cuid())
  userId      String
  expiresAt   DateTime
  createdAt   DateTime @default(now())
  user        User     @relation(fields: [userId], references: [id])

  @@index([userId])
  @@index([expiresAt])
}
\`\`\`

Modified User model: add `passwordHash String` (nullable for backward
compat with SSO-only users; constraint enforced at application layer).
```

**Spec-reviewer checks:**
- If the spec touches data, this section is non-empty
- If the spec doesn't touch data, this section says so explicitly:
  `_No data model changes._`

### API surface

**Contains:** new or modified API endpoints, server actions, RPC methods,
exported functions, or any other contract that crosses a boundary. For
each, specifies the input, the output, the errors, and the auth
requirements.

**Example:**

```markdown
Server actions in `app/(auth)/actions.ts`:

- `login(email: string, password: string): Promise<LoginResult>`
  - Success: `{ ok: true, redirectTo: string }`
  - Failure: `{ ok: false, error: "INVALID_CREDENTIALS" | "RATE_LIMITED" }`
  - Errors never expose whether the email exists (to prevent enumeration)

- `logout(): Promise<void>`
  - Clears the session cookie. Idempotent.
```

**Spec-reviewer checks:**
- If the spec touches APIs, this section is non-empty
- If the spec doesn't touch APIs, this section says so explicitly:
  `_No API surface changes._`

### Dependencies

**Contains:** what this spec depends on, both internal (other specs, other
modules) and external (libraries to be added, services to be configured).

**Example:**

```markdown
Internal:
- Existing User model and database connection
- `lib/env.ts` for environment-variable loading

External:
- New dependency: `iron-session@^8.0.0` (npm package)
- Required env var: `SESSION_SECRET` (32-byte random string)
- Required env var: `SESSION_COOKIE_NAME` (default: `__pilot_session`)
```

**Spec-reviewer checks:**
- At least one of (internal, external) lists is non-empty for any
  non-trivial spec
- Any new external dependencies are pinned (have a version)
- Any new env vars are named explicitly

### Verification strategy

**Contains:** how we'll know the work is done. The agents that implement
the spec read this section to know what commands to run at gate checks.
Lists the commands phase-implementer should run during Gate 3 (phase-scoped)
and Gate 5 (full suite).

**Example:**

```markdown
Per-phase verification commands (Gate 3):
- Typecheck: `pnpm typecheck`
- Lint touched files: `pnpm lint -- <files-touched>`
- Phase-scoped tests: `pnpm test -- <files-touched>`

Full-suite verification (Gate 5):
- Full test suite: `pnpm test`
- Build verification: `pnpm build`
- Integration tests: `pnpm test:integration`
- Manual smoke test: log in, log out, visit `/dashboard` while logged out,
  confirm redirect with `?next=` param preserved.
```

When the spec includes acceptance criteria with user-visible `Then` clauses
(rendered text, form state, navigation, visibility), the Verification
strategy should include `agent-browser` commands for browser verification.
These can appear in Gate 3 (per-phase) or Gate 5 (full-suite), depending
on whether the UI behavior is phase-scoped or cross-phase.

Example browser verification commands:

```
agent-browser open http://localhost:5173
agent-browser pushstate /patients
agent-browser wait --load networkidle
agent-browser snapshot -i
agent-browser get text @e1
agent-browser close
```

See `.pilot/rules/agent-browser.md` for the full command reference.

**Spec-reviewer checks:**
- Both Gate 3 and Gate 5 command lists are non-empty
- Commands look syntactically plausible (no `TODO`s, no placeholder text)
- At least one Gate 5 step references manual verification if the feature
  has user-facing behavior
- Feature has user-facing ACs (any `Then` clause describing rendered
  content, navigation, form state, or visibility) but Verification
  strategy contains no `agent-browser` commands → `moderate`, category
  `verify-no-browser`

---

## Examples of well-formed specs

A complete example spec is provided at `templates/spec.md`. It is the
starting point used by `/spec-compose` and is the format `spec-reviewer`
expects.

## Spec quality, not just format

`spec-reviewer` checks **format** mechanically. It also performs **quality
checks** that rely on judgment:

- Acceptance criteria that are too vague to test
- Out-of-scope items that should actually be in scope (or vice versa)
- Open questions that are actually blocking
- Architecture choices that conflict with project invariants in `AGENTS.md`
- Data model changes that would break existing migrations

Quality issues are reported as **findings**; the orchestrator decides
whether they block the pipeline or proceed with the spec author notified.
See `protocols/gate-checks.md` for the severity ladder.
