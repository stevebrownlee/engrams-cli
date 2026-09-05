# Protocol: profile-schema

**Status:** v1
**Loaded by:** `spec-narrator`, `/profile-init`, `/profile-review`,
the calibration loop in `protocols/skip-log.md`
**Defines:** the format of `~/.pilot/profile.md`, the per-developer
profile that drives PILOT's pedagogical layer

---

## Purpose

The developer profile is a living model of where the developer is on
their learning curve. It exists because PILOT's pedagogical layer
(rationale doc, comprehension checks, `--review` mode pauses) needs to
know how much explanation the developer wants — a junior on their first
auth system wants foundational explanations of closures and middleware;
a staff engineer on the same spec wants peer-level discussion of
session storage trade-offs.

The profile is:

- **Local-only.** Lives in `~/.pilot/profile.md`. Never transmitted.
- **Per-developer, not per-project.** The same developer working on
  three different projects uses the same profile. (Project-specific
  overrides live in `AGENTS.md`, not here.)
- **Markdown.** Human-edited, human-readable. No YAML, no JSON, no
  databases.
- **Inspectable.** The developer owns it. They can `cat` it, edit it
  in `$EDITOR`, version-control it elsewhere if they want.
- **Optional.** If the profile doesn't exist, `spec-narrator` operates
  in a default-everyone-is-mid-level mode and the calibration loop is
  inactive. PILOT works without a profile; it works *better* with one.

## File location

```
~/.pilot/profile.md
```

If `~/.pilot/` doesn't exist, the installer creates it. The profile
itself is created by `/profile-init`.

## Format

The profile has six required sections in this exact order:

```markdown
# Developer profile

## Strong skills

## Deepening

## Currently learning

## Granularity vector

## Notes

## Updated
```

Each section is described below. The file is plain markdown; no
frontmatter, no YAML, no special syntax beyond standard markdown.

---

### Strong skills

Topics where the developer wants **no explanation**. They've used these
enough to need no commentary; explaining them is condescending noise.

**Format:** bullet list of lowercase-kebab-case topic tags. One per
line. Optional trailing prose for grouping commentary.

```markdown
## Strong skills

- typescript
- react
- postgresql
- git
- python
- rest-apis

Anything in this category gets zero pedagogical content. Spec-narrator
treats these as assumed knowledge.
```

**Behavior:** `spec-narrator` omits explanations of topics in this
section entirely. If a phase's rationale would normally discuss
`postgresql`, and the developer has `postgresql` here, the rationale
doc says nothing about it.

### Deepening

Topics the developer knows well but is still refining. Explanations
are **brief, focused on the why**, never on the how.

**Format:** same as Strong skills — bulleted lowercase-kebab-case tags.

```markdown
## Deepening

- server-components
- react-19-actions
- prisma
- inngest-jobs

Brief explanations in rationale; comprehension checks rarely surface.
```

**Behavior:** `spec-narrator` produces 1–2 sentence rationale for these
topics. Comprehension checks in `--paired` mode are sparse and aimed at
edge cases, not basics.

### Currently learning

Topics the developer is actively learning. Explanations are **full**,
with examples and trade-offs.

**Format:** same as the other learning sections.

```markdown
## Currently learning

- server-side-streaming
- edge-runtime-constraints
- iron-session
- rate-limiting-strategies

Full explanations in rationale; comprehension checks engaged.
```

**Behavior:** `spec-narrator` produces full explanations (3+ sentences,
examples, trade-offs) for these topics. In `--paired` mode,
comprehension checks fire on every encounter.

### Granularity vector

Per-architectural-layer depth setting. Independent of the topic
sections above — topics in Strong skills get omitted regardless, but
*how* explanation lands for topics outside Strong skills is governed by
this vector.

**Format:** one line per layer, exactly five layers, in this order.
Each line is `layer: level`.

```markdown
## Granularity vector

- idiom: skim
- function: moderate
- data-flow: deep
- system: deep
- architecture: peer-level
```

**The five layers:**

| Layer          | What it covers                                          |
|----------------|---------------------------------------------------------|
| `idiom`        | Language-level constructs (closures, async, generics)   |
| `function`     | How individual functions/modules are decomposed         |
| `data-flow`    | How data moves between functions/modules                |
| `system`       | How components/services interact at the system boundary |
| `architecture` | Why the overall shape was chosen; alternatives          |

**The five levels:**

| Level        | At this layer, content is...                                          |
|--------------|-----------------------------------------------------------------------|
| `skip`       | omitted entirely                                                      |
| `skim`       | one line; no explanation                                              |
| `moderate`   | brief paragraph; the why, not the how                                 |
| `deep`       | full explanation with examples and trade-offs                         |
| `peer-level` | discussion as between senior engineers — alternatives, edge cases     |

**How spec-narrator uses this:** for each phase, for each layer
relevant to that phase, it produces content at the granularity level
the developer set. A junior with `idiom: deep` gets full explanations
of closures when a phase uses them; a senior with `idiom: skip` gets
nothing at the idiom layer at all.

**Note:** topic-based filtering (Strong/Deepening/Learning) and
granularity-based filtering compose. A topic in Strong skills is
omitted entirely regardless of the granularity vector. A topic
outside Strong skills inherits the granularity vector's level for
each layer.

### Notes

Free-form prose the developer wants the agents to know about. Examples:

```markdown
## Notes

I'm working through a backend rotation; UI architecture explanations
are useful even though I've been writing React for years.

When using Prisma, prefer explaining migration ordering — that's where
I keep tripping.

Don't suggest I add comments to my code. I prefer dense code with
descriptive names.
```

**Behavior:** `spec-narrator` reads this section and incorporates its
spirit into the rationale doc. It is the only section that is read
holistically rather than parsed into structured fields.

### Updated

Timestamp of the last change to the profile. ISO 8601 UTC.

```markdown
## Updated

2026-05-12T18:34:00Z
```

**Behavior:** `/profile-review` updates this whenever it modifies the
profile. `/profile-init` sets it when the profile is first created.
`spec-narrator` reads it to know whether the profile is fresh enough
to trust (if it hasn't been updated in 12+ months, surface a gentle
prompt to revisit).

## Topic tags: the canonical vocabulary

All topic tags (in Strong skills, Deepening, Currently learning, and
the skip log) use **lowercase kebab-case**:

- `react-hooks` (not "React Hooks" or "react_hooks")
- `typescript-generics`
- `postgresql-indexes`
- `iron-session`
- `server-components`

The set of tags is open — the developer (or the agent, with
confirmation) coins new tags as needed. The same tag is used everywhere
it appears: the profile, the skip log, suggested diffs.

When an agent encounters a topic it might want to log, it normalizes:

1. Strip case
2. Replace spaces and underscores with hyphens
3. Strip trailing/leading punctuation
4. Fuzzy-match against the profile's existing tags

On a fuzzy match (e.g., agent has `react hook`, profile has
`react-hooks`), use the profile's existing tag. On a true miss, prompt
the developer once:

> "I'm tagging this concept as `react-hooks`. Is that right? [Y/n]"

On `Y` or no response within a short timeout, the tag becomes
canonical. The agent records the new tag in the profile's relevant
section (typically Currently learning, but the developer can move it).

## What you do NOT put in the profile

- **Personal information.** No name, role, employer, team. The profile
  is about technical knowledge state, not identity.
- **Project context.** That belongs in `AGENTS.md` per-project. The
  profile is what the developer brings to *any* project.
- **Goals or career plans.** Not what this is for.
- **Credentials or API keys.** Ever. This is plaintext on disk.
- **Anything the developer wouldn't want a future co-worker to see.**
  The profile may end up in a backup, screenshot, or screen share.

## Profile evolution

The profile changes over time as the developer learns. Three forces
move topics around:

1. **Manual edits.** The developer opens `~/.pilot/profile.md` and
   moves a topic from Currently learning to Deepening. Direct and
   honest.
2. **Calibration loop.** `protocols/skip-log.md` defines how skip
   patterns trigger suggested updates. The developer accepts, declines,
   or modifies via `/profile-review`.
3. **Periodic check-in.** Every N spec completions (heuristic, not a
   hard rule), the orchestrator can surface a gentle prompt:
   "Your profile was last updated <X> months ago. Quick review?"
   In v1 this is a slice-(d) feature; mentioned here for context.

The profile is **the developer's**, not the agents'. The agents
*suggest* updates via the calibration loop, but every actual write is
either the developer's manual edit or the developer's explicit `[A]ccept`
in the prompt. No agent silently mutates the profile.

## Default profile (no profile present)

If `~/.pilot/profile.md` doesn't exist, `spec-narrator` operates in a
default mode:

- Strong skills: empty
- Deepening: empty
- Currently learning: empty
- Granularity vector: all layers at `moderate`
- Notes: empty
- Behavior: produce moderate-depth rationale for every concept

This default is the "neutral" rendering — it's what a generic
explainer would produce. It works, but it's not calibrated. The
developer gets value by writing a real profile.

## Versioning

This protocol is at version 1. Future versions may add sections or
change the layer/level vocabularies. Profiles are versioned implicitly
by the structure they conform to; if a future spec-narrator finds a
profile missing a required section, it prompts the developer to run
`/profile-init --upgrade` rather than crashing.
