# Protocol: rationale-format

**Status:** v1
**Loaded by:** `spec-narrator` (writes), `implementation-orchestrator`
(reads in `--review`/`--paired` modes)
**Defines:** the format of `specs/<ID>-<name>.rationale.md`, the
companion document that explains *why* each phase exists and what each
piece of work teaches

---

## Purpose

The rationale doc is the artifact that distinguishes PILOT from a pure
spec-to-PR pipeline. It is **for the developer**, not for the agents.
It runs alongside `progress.json` — same input (the spec, the
decomposed phases), different audience.

Where `progress.json` says "phase 1 has these exemplars and these
verification commands," the rationale doc says "phase 1 exists because
the data layer must be in place before any route can read or write
sessions; the migration must run before generated types reflect the
new shape; and the ordering of these two steps is a common source of
confusion that this phase exists to make explicit."

The rationale doc is calibrated to the developer's profile. Same spec,
same phases — radically different rationale depth depending on the
profile's granularity vector and learning sections.

## When the rationale doc is produced

- **In `--review` mode** — produced after Gate 2 (spec-json-builder),
  before Gate 3 (first phase implementation). The developer reads it
  before any code is written.
- **In `--paired` mode** — same timing as review mode; additionally
  used to source the comprehension checks that fire mid-phase.
- **In `--autonomous` mode** — not produced. The narrator agent doesn't
  run, and the rationale file doesn't exist.

This is the only artifact the narrator produces. The narrator doesn't
modify code, the spec, or the progress file.

## File location

```
specs/<ID>-<name>.rationale.md
```

Sits next to the spec and the progress file. Same base name.

## File structure

```markdown
# Rationale: <Spec ID> — <Spec Title>

> Produced for spec implementation in <mode> mode.
> Calibrated to developer profile at <ISO timestamp of profile.md>.

## Overview

<2-4 paragraphs explaining the spec's overall shape: why these phases,
in this order, and how the decomposition serves the goal>

## Phase 1: <phase title>

<rationale content for this phase, layered per granularity vector>

## Phase 2: <phase title>

<rationale content for this phase>

...

## Across phases

<optional section. Topics or trade-offs that span multiple phases and
make more sense to discuss once, here, rather than repeated per phase>
```

The exact section count matches the phase count in `progress.json`,
plus the Overview and optional Across-phases sections.

## Per-phase rationale content

For each phase, the rationale is organized by the five granularity
layers from `protocols/profile-schema.md`:

```markdown
## Phase N: <phase title>

### What this phase does

<one paragraph; always present, regardless of profile>

### Why this ordering

<one paragraph explaining why this phase comes where it does in the
sequence; always present>

### At the architecture layer

<content at the granularity level the profile sets for `architecture`>

### At the system layer

<content at the granularity level the profile sets for `system`>

### At the data-flow layer

<content at the granularity level the profile sets for `data-flow`>

### At the function layer

<content at the granularity level the profile sets for `function`>

### At the idiom layer

<content at the granularity level the profile sets for `idiom`>

### What this phase satisfies

<list of AC IDs from progress.json's acceptance array, resolved to
their G/W/T blocks, with one-sentence linkage>
```

The "What this phase does" and "Why this ordering" sections are
**always present** regardless of profile — these are the minimum
content every developer needs. The five layer sections are governed by
the granularity vector.

### Granularity rendering

For each layer section, the rendering depends on the profile's level
for that layer:

| Level        | Section behavior                                                       |
|--------------|------------------------------------------------------------------------|
| `skip`       | Section is omitted entirely — no heading, no content                   |
| `skim`       | Section appears with a single line of content                          |
| `moderate`   | Section appears with a paragraph (2-5 sentences); the why, not the how |
| `deep`       | Section appears with full explanation including examples and trade-offs |
| `peer-level` | Section appears with discussion of alternatives, edge cases, second-order effects |

**Example rendering for the architecture layer:**

A profile with `architecture: skip`:
```
(no section at all — heading and body omitted)
```

A profile with `architecture: skim`:
```markdown
### At the architecture layer

Standard server-action pattern with session in cookie.
```

A profile with `architecture: moderate`:
```markdown
### At the architecture layer

The auth flow uses server actions because the form submission needs
server-side validation before the redirect happens; client-side
validation would let an attacker bypass it. Sessions live in a cookie
rather than the database because the team already operates iron-session
and there's no requirement for server-side revocation in this spec.
```

A profile with `architecture: peer-level`:
```markdown
### At the architecture layer

The choice between server actions and a traditional API route is mostly
about progressive enhancement: server actions degrade to standard form
posts when JS is off, which matters for the login flow because that's
the first page a new visitor sees. The cost is that server actions
can't be called from non-form contexts as cleanly, so if we later add a
mobile app that hits this auth flow, we'll need to surface an API
endpoint anyway — at that point it may be worth refactoring to the
endpoint and having the server action wrap it.

The session-in-cookie choice is more controversial. iron-session
encrypts the payload, so the cookie itself isn't a leak, but the trade-
off is that revoking a session before its expiration requires either a
revocation table (defeating the stateless property) or accepting the
expiration as the revocation window. For this spec the latter is fine,
but it locks us into a maximum session length of whatever we set the
expiration to — there's no "log out everywhere" affordance.
```

The level shifts the entire register. A `moderate` paragraph is for
a developer who needs the why; a `peer-level` paragraph is for a
developer who wants to discuss the choice as a peer.

### Topic filtering

Independently of the granularity vector, **topics in the profile's
Strong skills section are omitted** from rationale content even if the
granularity vector would otherwise include them.

Example: if `architecture: deep` but the topic is `server-actions` and
that's in Strong skills, the architecture-layer section for that phase
either:
- Omits the discussion of server-actions entirely (if it was the only
  topic the section would discuss), or
- Discusses other architectural topics relevant to the phase without
  mentioning server-actions

The narrator decides based on context — if removing the Strong-skills
topic leaves the section empty, omit the section. If other content
remains, render at the granularity level minus the omitted topic.

## The Overview section

The Overview is 2-4 paragraphs at the top of the rationale doc that
frames the entire spec. It always covers:

1. **What the spec is about** — one paragraph paraphrasing the spec's
   Summary section in the narrator's voice
2. **How it's decomposed** — one paragraph explaining the shape of the
   phase sequence (linear, parallel-eligible, sequential-with-fan-in,
   etc.) and why
3. **What to watch for** — one paragraph naming any unusual aspects:
   tricky ordering, deferred decisions, areas where the developer's
   profile suggests they'll want to read more carefully
4. **Optional: what's interesting** — one paragraph for paired or
   peer-level developers; called out only when the spec has a genuinely
   interesting architectural call

The Overview is calibrated by the profile's overall depth. A developer
whose granularity vector is dominated by `skim`/`moderate` gets a
shorter Overview; one whose vector is dominated by `deep`/`peer-level`
gets a longer one.

## The Across-phases section (optional)

Some specs have considerations that span phases — a security
constraint that affects every phase, a performance bar that influences
multiple choices, a backward-compat concern that shapes how
data-model and API phases interact. These are awkward to discuss
per-phase because they'd repeat.

The narrator includes an "Across phases" section at the bottom only
when:

- The cross-cutting concern is non-obvious from the per-phase
  rationale
- The developer's profile suggests they'd benefit (typically `system`
  or `architecture` at `deep` or `peer-level`)

If neither condition holds, the section is omitted.

## AC ID resolution in the rationale

Where the rationale references an AC, it cites by ID and resolves the
title from the spec:

> "Phase 3 satisfies AC-2 (Successful login redirects to original
> destination) by..."

Full G/W/T blocks are not embedded in the rationale — the spec is the
canonical home for that content. The rationale references; the spec
holds.

## Tone and voice

The rationale is written in the narrator's voice, addressing the
developer directly:

- **Second-person where natural** — "You'll notice that phase 2
  doesn't touch the API layer..." rather than "The developer will
  notice..."
- **Plain explanation, not commentary** — the rationale explains; it
  doesn't editorialize ("this is a clever design") or hedge ("this
  might be confusing")
- **Specific, not generic** — name the actual file, the actual
  function, the actual library decision. Generic explanations of
  "best practices" are a smell; the rationale exists to explain *this
  spec's* decisions, not the field broadly
- **One register per developer** — the narrator picks the register
  based on the profile and stays there. A peer-level developer doesn't
  get one paragraph in technical-mentor mode followed by another in
  Wikipedia-summary mode

## What the rationale does NOT contain

- **Code.** The rationale explains code; it doesn't reproduce it. If
  a code example is genuinely necessary for clarity, the narrator may
  include a short snippet, but the default is prose.
- **Verification commands.** Those live in the progress file. The
  rationale may mention *why* the verification is shaped a certain
  way, but it doesn't list the commands.
- **Findings.** Code-reviewer's findings live in the progress file's
  `review_findings` and surface in the PR description. The rationale
  is forward-looking (before phases run); findings are backward-looking
  (after phases run).
- **Implementation details the spec didn't specify.** The rationale
  explains the spec's choices; it doesn't speculate about choices the
  spec left to the implementer.
- **The skip log.** Even when the calibration loop is active, the
  rationale doesn't reference the skip log. The two artifacts have
  different audiences (rationale for the developer's understanding,
  skip log for the agents' calibration).

## Comprehension check sourcing (paired mode only)

In `--paired` mode, the orchestrator pulls comprehension check
questions from the rationale doc. Each `deep` or `peer-level` layer
section can produce 1-3 questions; the orchestrator fires them at
phase boundaries (or, in slice-d, at sub-step boundaries).

The narrator does not write the questions explicitly — the
orchestrator generates them from the rationale content at runtime. The
narrator's job is to write rationale that *can be questioned*; the
orchestrator's job is to generate the questions.

This means the rationale doc is the same in `--review` and `--paired`
modes. The difference is what the orchestrator does with it.

## Versioning

This protocol is at version 1. The rationale doc format is
forward-compatible: new sections can be added without invalidating
older docs. The narrator notes its version in the doc's preamble so
the orchestrator can adapt if reading an older rationale.
