# Protocol: self-review

**Status:** v1
**Loaded by:** `code-reviewer`, `implementation-orchestrator` (for finding routing)
**Defines:** what `code-reviewer` checks at Gate 4, how findings are
written, and how they're graded

---

## Purpose

Gate 4 is the self-review pass. After a phase's implementation passes its
Gate 3 verification, `code-reviewer` looks at the diff with fresh eyes —
its context isn't polluted by the back-and-forth of implementation. The
goal is to catch the things that Gate 3 can't catch: convention drift,
invariant violations, missing tests, unstated assumptions, and quality
issues that compile cleanly and pass tests but shouldn't ship.

This protocol describes **what** the reviewer checks. The severity ladder
and orchestrator routing are in `protocols/gate-checks.md`.

## What code-reviewer reads

For each phase under review:

1. **`AGENTS.md`** — the project's invariants and conventions
2. **`rules/`** — any rule files whose `applies-to` glob matches the
   phase's `files_touched`
3. **The phase definition** in `progress.json` — `acceptance` (a list of
   AC IDs like `["AC-1", "AC-3"]`), `exemplars`, `files_touched`
4. **The spec** — to resolve the AC IDs in the phase's `acceptance` array
   back to their G/W/T blocks
5. **The diff** — only the lines changed by this phase (not the full
   files; the reviewer trusts unchanged code)
6. **`agent-memory/code-reviewer/findings.md`** — past findings, to
   recognize recurring patterns

The reviewer does **not** re-run tests or verification commands. Those
were already run at Gate 3. The reviewer's job is to look at what was
written, not to re-verify that it works.

## The check categories

The reviewer organizes findings under categories. Each category has a
clear question the reviewer asks of the diff.

### `invariant-violation`

> Does this change violate any invariant declared in `AGENTS.md`?

This is the highest-priority category. Invariants are non-negotiables;
violations are always `severe`. Examples:

- `AGENTS.md` says "all database access goes through `lib/db/`"; the diff
  imports `@prisma/client` directly in a route handler.
- `AGENTS.md` says "no secrets in code"; the diff includes a literal
  string that looks like an API key.
- `AGENTS.md` says "server actions never called from useEffect"; the diff
  does exactly that.

**Default severity:** `severe`. Never downgrade. If the diff legitimately
needs to violate an invariant (rare), the invariant itself should be
updated — and that update belongs in a separate spec.

### `convention-drift`

> Does this change follow the conventions declared in `rules/`?

Less strict than invariants but still important. Conventions are how the
team has agreed to write things; drift is how codebases become
inconsistent over time. Examples:

- `rules/typescript.md` says "no `any`"; the diff uses `any`.
- `rules/react.md` says "components in PascalCase files"; the diff adds
  `userCard.tsx` instead of `UserCard.tsx`.
- `rules/commits.md` says "imperative present tense"; a commit message
  is past tense.

**Default severity:** `moderate`. May be downgraded to `minor` if the
convention is genuinely ambiguous and the existing codebase already drifts.

### `missing-test`

> Does this change add behavior that should have a test, but doesn't?

Examples:

- A new public function in `lib/auth/` with no corresponding test file.
- A new error branch in an existing function (the function had a test,
  but only for the happy path; the new branch is uncovered).
- A `Then` clause in an AC referenced by this phase that has no
  corresponding test assertion.

**Default severity:** `moderate`. Upgraded to `severe` if the AC the
phase references explicitly contains a testable `Then` clause that this
diff fails to cover. Downgraded to `minor` for cases where the
behavior is genuinely difficult to test (UI animations, third-party
integration boundaries).

### `unstated-assumption`

> Does this change assume something that isn't stated in the spec, code,
> or types?

The most subtle category. Examples:

- The diff assumes `process.env.DATABASE_URL` is set, but `lib/env.ts`
  doesn't validate it.
- The diff assumes a Prisma migration has already run, but the migration
  file isn't in this phase's `files_touched`.
- The diff calls `await fetch(...)` without a timeout or error handler;
  the assumption is that the call always succeeds.

**Default severity:** `moderate`. Upgraded to `severe` if the unstated
assumption is about security or data integrity (e.g., assuming user input
is sanitized when it isn't).

### `quality-smell`

> Is there something here that compiles, passes tests, but a reviewer
> would push back on in a PR?

Catch-all for the things experienced reviewers would flag. Examples:

- A 200-line function that could be three 60-line functions.
- A variable named `data` or `info` where a specific name is obvious.
- A magic number with no comment.
- A `// TODO` left in the diff.
- A copy-pasted block of code that should have been extracted.

**Default severity:** `minor`. Almost never `severe`. Quality smells are
the things PILOT's contract is honest about: the pipeline may ship them,
but it surfaces them.

### `acceptance-gap`

> Does this phase actually satisfy the AC IDs in its `acceptance` array?

For each AC ID in the phase's `acceptance` array, the reviewer resolves
the ID back to its G/W/T block in the spec and asks: does the diff
contain code (and tests) that would make the `Then` clauses observably
true given the `When` clause occurs under the `Given` conditions? If a
G/W/T block has no corresponding code, it's an acceptance gap.

Reviewer flags the gap with the AC ID:

> "Phase claims to satisfy AC-3 ('Invalid password preserves the email
> field'), but no code in the diff handles the `Then the email field
> retains its value` clause."

**Default severity:** `severe`. The phase claims it's complete; if it
doesn't actually meet the AC IDs it claimed, it isn't complete. Sending
the phase back to the implementer is correct.

### `missing-browser-verification`

> Does this phase touch frontend files and satisfy user-visible ACs,
> but have no browser verification in its `verification` array?

When a phase's `files_touched` includes `.tsx` or `.ts` files under
`frontend/src/` and its `acceptance` references ACs with user-visible
`Then` clauses (rendered text, navigation, form state, visibility),
the phase should have `agent-browser` commands in its `verification`
array.

**Default severity:** `moderate`. The code-reviewer flags the gap; it
doesn't block the pipeline. This is a quality signal, not a hard gate.
Downgrade to `minor` if the UI behavior is fully covered by
Vitest/Testing Library tests in the diff.

---

## The finding format

Each finding is a JSON object written to the phase's `review_findings`
array in `progress.json`:

```json
{
  "severity": "moderate",
  "category": "convention-drift",
  "message": "Direct prisma import in route handler; AGENTS.md invariant requires going through lib/db/",
  "file": "app/api/login/route.ts",
  "line": 12,
  "resolution": "noted"
}
```

| Field        | Required | Notes                                          |
|--------------|----------|------------------------------------------------|
| `severity`   | yes      | `severe` \| `moderate` \| `minor`              |
| `category`   | yes      | One of the categories above                    |
| `message`    | yes      | One sentence. Specific. Names the actual issue. |
| `file`       | no       | Repo-relative path                             |
| `line`       | no       | Line number                                    |
| `resolution` | yes      | `fixed_in_phase` \| `deferred` \| `noted`      |

`resolution` is filled in by the orchestrator after the reviewer returns:

- `fixed_in_phase` — finding triggered a re-implementation; the rewritten
  code addresses it
- `deferred` — finding was significant but not blocking; surfaces in the
  PR description
- `noted` — finding was recorded for the audit trail; no action taken

## Writing good messages

A finding's `message` field is read by humans. Write it the way a
respected senior would: specific, low-temperature, names the issue, names
the relevant invariant or rule.

**Good messages:**

> Direct prisma import in route handler; AGENTS.md invariant requires
> going through lib/db/.

> The new `verifySession` function has no test. The phase's third
> acceptance criterion requires that invalid sessions return null,
> which is currently only asserted in commentary.

> `parseDateRange` is called from two places with different timezone
> assumptions; the function silently uses UTC.

**Bad messages:**

> This code is bad. (Too vague. What's bad?)

> Consider refactoring. (Vague *and* a recommendation rather than an
> observation.)

> Convention violation here. (Names a category but no specifics — which
> convention, which line?)

## Memory: accumulating findings across runs

`code-reviewer` has `memory: true` in its metadata. After each Gate 4 run,
it appends to `agent-memory/code-reviewer/findings.md`:

```markdown
## 2026-05-11 — phase 3 of spec 0001-user-authentication

- moderate / convention-drift — direct prisma import in route handler
  (app/api/login/route.ts:12)
- minor / quality-smell — `data` variable name in extractEmail (lib/auth/utils.ts:48)

## 2026-05-10 — phase 2 of spec 0001-user-authentication
...
```

This serves two purposes:

1. **Pattern recognition.** If the same convention drift appears in three
   phases, that's a signal the rule isn't being followed — the reviewer
   can note it explicitly in subsequent reviews and the team can decide
   whether the convention is too strict or whether enforcement needs to
   move to lint.
2. **Audit trail.** When a developer reviews a PR PILOT produced, they
   can see what was found and how it was resolved across the spec's
   history.

The memory file is committed to source control by default. If your team
prefers it local, add `agent-memory/` to `.gitignore`.

## Things the reviewer does NOT check

Worth being explicit about, since these are common assumptions:

- **Performance.** Unless an `AGENTS.md` invariant or `rules/performance.md`
  explicitly names a performance bar, the reviewer doesn't flag
  performance. Premature optimization is a real cost.
- **Security beyond named invariants.** The reviewer flags things that
  violate stated rules; it does not do open-ended threat modeling. That's
  what `security-auditor` would be (not in v1).
- **The spec's correctness.** The reviewer trusts the spec. If the spec
  is wrong, that's a `spec-revision-needed` finding from the debugger at
  Gate 3, not a Gate 4 finding.
- **Whether the feature is a good idea.** Out of scope. The reviewer
  reviews what's in front of it.
