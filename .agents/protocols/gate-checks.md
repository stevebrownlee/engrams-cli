# Protocol: gate-checks

**Status:** v1
**Loaded by:** `phase-implementer`, `phase-scaffold`, `phase-debugger`,
`code-reviewer`, `implementation-orchestrator`
**Defines:** the verification strategy, retry ladder, and severity grading
that govern phase execution

---

## Purpose

Every phase passes through a series of gates. Gates are the checkpoints
that let the pipeline run unattended: they're how we know a phase actually
worked instead of just appearing to work. This protocol defines what
happens at each gate, what to do when a gate fails, and how findings are
graded.

## The gate sequence

```
Gate 0    Orchestrator boot — read AGENTS.md, validate environment
Gate 1    Spec review — spec-reviewer grades the spec
Gate 2    Spec build — spec-json-builder produces progress.json
Gate 3    Phase implementation + verification (per phase)
Gate 4    Self-review (per phase)
Gate 5    Full-suite verification (after all phases complete)
```

Six integer gates, numbered 0 through 5. Gates 0, 1, and 2 run once per
pipeline. Gates 3 and 4 run once per phase. Gate 5 runs once at the end.

This protocol covers Gates 3, 4, and 5 in detail. Gates 0, 1, and 2 are
covered by their respective agents' bodies and by `spec-format.md`.

---

## Gate 3: phase implementation + verification

Gate 3 is the inner loop. It runs once per phase. The contract:

1. The implementing agent (`phase-implementer` or `phase-scaffold`) does the
   work — writes code, updates types, adds tests.
2. After the work, the agent runs the phase's `verification` commands (from
   the progress file) in order.
3. If all commands exit zero, Gate 3 passes; proceed to Gate 4.
4. If any command fails, enter the **retry ladder** (below).

### The retry ladder

When a Gate 3 verification command fails, the agent does not give up
immediately. The ladder gives the agent three progressively more informed
attempts before escalating.

| Retry | Trigger             | Context provided                              | Agent          |
|-------|---------------------|-----------------------------------------------|----------------|
| 0     | First attempt       | Phase spec + exemplars                        | implementer    |
| 1     | First failure       | Above + the failing command's stderr/stdout   | implementer    |
| 2     | Second failure      | Above + diff of the agent's recent changes    | implementer    |
| 3     | Third failure       | Hand off to phase-debugger                    | debugger       |

On each retry, `retry_count` in the progress file is incremented. The
agent may modify code on each retry; the retry is not just a re-run.

**Why three retries:** the first retry is often enough to fix a transient
issue (test relied on ordering, a file wasn't saved). The second retry is
where the agent sees its own diff and notices what it changed. The third
retry is the boundary at which the agent's local context is unlikely to
solve the problem, and a fresh agent (the debugger) with a different
prompt is more likely to make progress.

**Why not more retries:** unbounded retries lead to oscillation — the
agent makes the same wrong fix multiple times. The debugger handoff is a
genuine context reset, not just another attempt.

### When the debugger runs

`phase-debugger` is invoked at retry count 3. It:

1. Reads the spec, the progress file, the phase definition, and all of
   `phase-implementer`'s prior attempts (captured in transcript).
2. Performs root-cause analysis: is the spec wrong? Is an invariant being
   violated? Is the test wrong? Is the dependency broken? Is the
   environment misconfigured?
3. Writes a diagnosis to `blocked_reason` in the progress file.
4. Returns to the orchestrator with one of three recommendations:
   - **`recoverable`** — the debugger can describe the fix; orchestrator
     re-invokes phase-implementer with the diagnosis included in context.
     One more attempt is allowed. If it fails, the phase is blocked.
   - **`spec-revision-needed`** — the spec has a flaw that no amount of
     implementation work will fix. Pipeline blocks; human must edit the
     spec.
   - **`environment-blocker`** — something outside the codebase is broken
     (missing env var, service down, dependency unavailable). Pipeline
     blocks; human must resolve the environment.

In all three cases, the phase status moves to `blocked` and the pipeline
halts. The orchestrator surfaces the diagnosis to the developer.

### Gate 3 cost considerations

Every retry consumes tokens. The retry ladder is deliberately capped at
3 to bound the worst-case cost of a single phase. A spec that produces
phases prone to retry-ladder exhaustion is a spec quality problem; flag
it in code-reviewer findings so future spec authoring improves.

### Browser verification at Gate 3

When a phase's `verification` array includes `agent-browser` commands,
the implementing agent runs them after the standard verification commands.
Browser commands follow the same pass/fail/retry contract as any other
verification command:

- Exit zero → pass
- Non-zero exit → enter the retry ladder

Before running browser commands, the agent must ensure:
1. The frontend dev server is running on `localhost:5173`
2. The backend dev server is running on `localhost:4000`
3. No prior `agent-browser` session is dangling (`agent-browser close --all`)

After browser verification, always close the session:
`agent-browser close`

See `.pilot/rules/agent-browser.md` for command reference and
`.pilot/rules/ui-verification.md` for the verification SOP.

---

## Gate 4: self-review

After Gate 3 passes for a phase, `code-reviewer` runs. The contract:

1. Reviewer reads the phase's diff (only the files in `files_touched`).
2. Reviewer reads `AGENTS.md`, relevant entries from `rules/`, and the
   phase's `acceptance` criteria (resolved from AC IDs in the spec).
3. Reviewer produces a list of **findings**, each graded by severity.
4. Findings are written to the phase's `review_findings` array in the
   progress file.

For full details of *what* code-reviewer checks for, see
`protocols/self-review.md`. This protocol defines what the orchestrator
*does* with the findings.

### Severity ladder

| Severity   | Definition                                                     | Orchestrator action                                |
|------------|----------------------------------------------------------------|----------------------------------------------------|
| `severe`   | Violates an `AGENTS.md` invariant or a rule with `MUST` wording | Send phase back to phase-implementer with finding |
| `moderate` | Violates project convention or introduces a clear regression   | Note in progress file; continue. Surface in final report. |
| `minor`    | Style nit, missed opportunity, suggestion for improvement      | Note in progress file; continue                    |

**Severe** findings re-enter the retry ladder at retry 1 (the agent is
given the finding as context, not asked to start over). If the agent can't
resolve a severe finding within the remaining retries, the phase is
blocked.

**Moderate** and **minor** findings do not block the pipeline. They are
collected and surfaced in the final PR description so the developer can
review them in context.

### Why not block on every finding

If every code-reviewer finding blocked the pipeline, the autonomous
pipeline becomes interactive — every minor convention drift becomes a
human conversation. Severity grading is what lets autonomy be real: only
the things that genuinely require a redo trigger one.

The trade-off: some imperfections ship to the PR. They're surfaced
clearly, but they ship. This is intentional. PILOT's contract is "the
pipeline finishes and surfaces what it noticed," not "the pipeline only
finishes when the result is perfect."

---

## Gate 5: full-suite verification

After all phases are complete, `phase-implementer` runs Gate 5: the
full-suite verification commands from the spec's `Verification strategy`
section.

1. Run every command in the spec's Gate 5 list, in order.
2. If all pass, mark pipeline `status: complete` in the progress file.
3. If any fail, enter a **restricted retry ladder**:
   - Retry 1: re-run with stderr/stdout context.
   - Retry 2: invoke phase-debugger directly (skip retry 2 of the
     standard ladder, because at Gate 5 the diff is large and incremental
     debugging is more valuable).
4. On debugger handoff, treat the same as Gate 3 — orchestrator routes
   based on the debugger's recommendation.

Gate 5 also drives the final commit message and PR description; see
`protocols/commit-message.md`.

---

## What gates do NOT do

- **Gates do not modify the spec.** If a spec needs to change, the
  pipeline must block. Agents never edit the spec mid-run.
- **Gates do not modify `AGENTS.md`.** Same reasoning. Architectural
  invariants are out of scope for any single feature spec.
- **Gates do not modify another phase's `files_touched`** unless that
  phase is explicitly listed in `depends_on` and is `complete`. Cross-phase
  edits should be rare; when they happen, they're recorded in
  `review_findings` so the audit trail is preserved.

---

## Summary diagram

```
                 ┌──────────────┐
   start ───────▶│   Gate 3     │── pass ──┐
                 │ verification │           │
                 └──────┬───────┘           ▼
                        │ fail        ┌──────────┐
                        ▼             │  Gate 4  │── severe ──┐
                 ┌──────────────┐     │  review  │            │
                 │  retry 1-2   │     └─────┬────┘            │
                 │  implementer │           │ ok              │
                 └──────┬───────┘           ▼            ┌────┴─────┐
                        │ fail        ┌──────────┐      │ back to  │
                        ▼             │  Gate 5  │      │  retry 1 │
                 ┌──────────────┐     │ (final)  │      └──────────┘
                 │  debugger    │     └─────┬────┘
                 │  diagnosis   │           │
                 └──────┬───────┘           ▼
                        │              complete
                        ▼
                 ┌──────────────┐
                 │  blocked     │
                 │  (human)     │
                 └──────────────┘
```
