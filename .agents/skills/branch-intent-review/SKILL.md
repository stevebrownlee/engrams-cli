---
name: branch-intent-review
description: Qualitative review of the changes on the current branch. Determines the intent and goal of the code, explains how it fits into the larger project and what capabilities it creates or changes, and aggressively reviews whether every added test earns its place. Use whenever the user wants to understand what a branch actually accomplishes, asks for a summary of branch changes before a PR, wants a plain-language explanation of recent work, or wants the added tests scrutinized for necessity — even if they don't explicitly ask for an "intent review."
---

You are reviewing the code changes on the current branch to explain *why they exist* and *what they accomplish* — not whether they follow style rules (that is code-review-checklist's job). Your deliverable is a readable report a busy engineer can skim in two minutes and walk away knowing what this branch does and whether its tests are pulling their weight.

## 1. Gather the changes

Use `git diff main...HEAD` to identify changed files and hunks. If the branch has no upstream base configured, fall back to the merge-base with the repo's default branch. If the user names a different base, use that.

Read each changed file — the full file, not just the hunk. Intent rarely lives in the diff alone; it lives in how the changed lines interact with the code around them. A three-line change to a resolver means nothing until you know who consumes its output.

## 2. Determine intent

Answer these questions for yourself before writing anything:

- **What problem does this solve?** Look for the friction: a missing capability, a broken behavior, a workflow that forced users around the system. Commit messages and PR descriptions are hints, not answers — verify against the code.
- **How does it fit the larger project?** Identify which subsystem it touches and what role that subsystem plays. Read neighboring modules, adjacent docs, or the callers/callees of changed functions until you can state the fit in one sentence.
- **What capabilities does it create or modify?** Frame these as things the system (or its users) can now do, or do differently — not as functions that were added.

If the diff is genuinely ambiguous about intent, say so plainly rather than inventing a narrative.

## 3. Review the tests — aggressively

Scope boundary with code-review-checklist: the checklist owns *coverage* — does a
new public function, controller action, or auth boundary have a test at all. This
skill owns *necessity* — of the tests that exist, which earn their place. The two
verdicts answer different questions about the same tests, so they can coexist on one
PR: the checklist may flag a missing authz test while this skill recommends cutting
a redundant header test. Never recommend cutting a test that is the only coverage of
a contract the checklist requires — flag the tension instead.

Be brutal — this is an explicit instruction, not a mood. Every test is a liability until proven an asset: tests cost maintenance time, slow suites, and break on refactors that change no real behavior, so the ideal suite for this branch is the smallest one that still guards critical behavior. The default answer to "is this test needed?" is **no**; the test must argue its way in, and a borderline verdict is a cut. Do not soften recommendations to seem fair, do not keep a test because someone took the time to write it, and do not accept "more coverage can't hurt" — it does, on every future refactor. The litmus test: a test must name, in one sentence, the critical contract it guards, whose silent breakage would produce a wrong result a user or operator would actually notice. Can't write that sentence → cut. The sentence describes something a compiler, type system, shared-module test, or sibling test already guards → cut.

For each new or modified test, decide **keep / merge / cut** and be prepared to justify it in one sentence — the sentence is the critical contract it guards (a business invariant, state transition, authorization boundary, calculation, or harmful error path):

**Cut candidates (recommend removal):**

- Tests asserting static text, labels, or hardcoded strings
- Tests asserting seed data or static fixtures exist
- Tests of framework plumbing: component mounts, pass-through wrappers, re-exports, trivial getters
- Tests that re-verify what a type system or compiler already guarantees
- Tests that duplicate coverage of the same contract another test already defends (keep the stronger one, cut the rest)
- Tests tightly coupled to implementation details (internal state shape, private function names, exact call sequences) — these have the largest blast radius and break on every innocent refactor

**Merge candidates:** several narrow tests asserting different facets of one behavior collapse into a single test that exercises the behavior once.

**Keep candidates:** the only tests protecting a critical contract from silent regression.

For each recommendation, name the blast radius: what innocent change would break this test and cost someone time.

## 4. Write the report

Plain language throughout. Write for a smart reader who doesn't live in this codebase. Explain jargon or omit it. Short sentences. No filler, no throat-clearing, no marketing tone. If a sentence doesn't add information, delete it.

Use this structure:

```markdown
# Branch Review: <branch name>

## What this branch does
<2-4 sentences. The problem, the fix, in plain terms.>

## How it fits
<1-3 sentences. Where this lives in the project and why that's the right place
—or a flag if it isn't.>

## Capabilities
<Short bullet list of what the system can now do / do differently. Phrased as
capabilities, not function names.>

## Test review
<For each test file or group: keep / merge / cut + one-sentence reason. State
the total reduction you're recommending (e.g. "12 tests → 5"). If the tests are
all justified, say so briefly — don't manufacture cuts.>
```

Length discipline: if the report needs more than a page, the branch is doing too much or the writing is doing too little. Compress the writing first.

## 5. Follow-up

End the report with a direct question to the human: whether they'd like to follow up on anything in the review — dig deeper into a section, challenge a test-cut recommendation, or apply the recommended test removals.
