# Protocol: skip-log

**Status:** v1
**Loaded by:** `spec-narrator`, `/profile-review`, the orchestrator in
`--paired` mode
**Defines:** the calibration loop that tracks developer skips and
suggests profile updates when patterns emerge

---

## Purpose

The skip log is the calibration loop's data backbone. It records every
time the developer skips a pedagogical artifact — a comprehension
check, an inline explanation, a rationale doc — tagged by topic. When
a topic accumulates enough weighted skip points within a recent window,
the calibration loop fires a prompt suggesting the developer's
understanding of that topic has matured.

The premise: skipping is signal. Skipping out of impatience and
skipping out of mastery look identical in the moment, but the *pattern*
of skips disambiguates over time. The follow-up prompt is the final
disambiguation step — the developer answers whether the skips meant
"I know this" or "I was in a hurry."

## File location

```
~/.pilot/skip-log.md
```

Same directory as the profile. Local-only, never transmitted,
per-developer not per-project.

## What gets logged

Three kinds of skip:

| Kind                 | Weight | When                                                          |
|----------------------|--------|---------------------------------------------------------------|
| `comprehension-check` | 3     | Developer skipped a paired-mode comprehension question        |
| `inline-explanation` | 1      | Developer asked to skip an inline explanation mid-flow        |
| `rationale-doc-unread` | 0    | Developer never opened the rationale doc before phase started |

`rationale-doc-unread` is **logged but not counted toward the threshold**.
Recording it is useful for the developer's own audit; counting it would
penalize the common pattern of trusting the autonomous pipeline without
opening every doc.

The 3:1 weight ratio means one comprehension-check skip carries as
much signal as three inline-explanation skips. Comprehension checks
are direct probes — declining one is a stronger statement than
declining further detail. The weights are tunable per-developer (see
"Configuration" below).

## Threshold

A topic crosses the threshold when its accumulated weighted points
within the sliding window reach **6**.

The threshold is **per-topic**: each topic accumulates its own
counter; the threshold value (6) is the same across every topic. There
is no global counter. When `react-hooks` hits 6, the prompt fires for
`react-hooks` — it doesn't matter what `typescript-generics` is at.

## Sliding window

Only entries within the **last 60 days** count toward a topic's
threshold. Older entries are preserved in the log file (the developer
keeps a complete history) but ignored at evaluation time.

Evaluation timing: when an agent (typically `spec-narrator` or the
orchestrator) logs a new skip, after writing the entry it re-evaluates
the topic's counter for the current 60-day window. If the threshold is
reached, the calibration prompt fires.

The window is rolling — `today - 60 days` is computed at evaluation
time, not on a schedule. The file is not pruned on a schedule either;
old entries simply stop counting.

## File format

```markdown
# Skip log

This file records skip events for PILOT's calibration loop. Entries
older than 60 days are preserved here but don't count toward
calibration thresholds.

---

## 2026-05-12T14:32:00Z — spec 0001 phase 3
- topic: react-hooks
- kind: comprehension-check
- weight: 3
- context: explanation of useEffect dependency array

## 2026-05-12T14:48:00Z — spec 0001 phase 4
- topic: typescript-generics
- kind: inline-explanation
- weight: 1
- context: rationale doc, "why we use generic constraints here"

## 2026-05-10T09:15:00Z — spec 0001 phase 1
- topic: react-hooks
- kind: comprehension-check
- weight: 3
- context: setState batching question
```

Each entry is a level-2 markdown section. Fields:

| Field     | Required | Notes                                                       |
|-----------|----------|-------------------------------------------------------------|
| heading   | yes      | `## <ISO 8601 UTC> — spec <ID> phase <N>` (or `— (no spec)`) |
| `topic`   | yes      | lowercase-kebab-case tag, matches profile vocabulary        |
| `kind`    | yes      | one of `comprehension-check`, `inline-explanation`, `rationale-doc-unread` |
| `weight`  | yes      | numeric; matches the kind's weight at write time            |
| `context` | yes      | one-sentence description of what was skipped                |

**Why weight is stored even though it's derivable:** the developer can
change weights in config later; entries keep the weight that was in
effect when they were logged. The sliding-window evaluator uses the
stored weight, not the current config — so changing weights doesn't
retroactively change history.

Order: entries are appended chronologically (newest at the bottom in
the file as written, but the heading timestamp is the source of truth
for ordering — entries can be in any visual order without affecting
correctness).

## How a skip gets logged

When an agent decides to log a skip:

1. **Determine the topic.** Use the topic recognition rules from
   `protocols/profile-schema.md`: lowercase kebab-case, fuzzy-match
   against existing profile tags, prompt-and-confirm on first encounter
   of a new tag.
2. **Determine the kind and weight.** Based on what was skipped:
   - paired-mode question skip → `comprehension-check`, weight 3
   - inline explanation skip → `inline-explanation`, weight 1
   - rationale doc never opened before phase complete →
     `rationale-doc-unread`, weight 0
3. **Append an entry to `~/.pilot/skip-log.md`.** Use the format above.
4. **Re-evaluate the topic's threshold.** Sum weights for entries with
   the same `topic` whose heading timestamp is within the last 60 days.
5. **If the threshold is reached, fire the prompt.** See "The
   calibration prompt" below.

If multiple skips happen in rapid succession (e.g., the developer
skips three inline explanations in a single phase), each is logged
separately. The agent does not batch.

## The calibration prompt

When a topic's weighted sum in the sliding window reaches 6, the
orchestrator surfaces this prompt to the developer:

```
Topic "<topic>" has accumulated 6 weighted skip points across
the last <N> days. Has your understanding of this topic strengthened?

If yes, PILOT can update your profile:

   Move "<topic>" from <current section> → <suggested section>
   Wipe the <count> skip entries for this topic.

   [A]ccept   [D]ecline   [M]odify

[A]ccept    Apply the suggested change.
[D]ecline   Keep profile unchanged. Skip entries wipe (the developer
            affirmed expediency, not mastery).
[M]odify    Open ~/.pilot/profile.md in $EDITOR; you decide the
            change manually. Skip entries wipe.
```

### Suggested section transitions

The "→" in the prompt is computed by promoting the topic one section
toward Strong skills:

| Current section      | Suggested move                          |
|----------------------|-----------------------------------------|
| Currently learning   | Deepening                               |
| Deepening            | Strong skills                           |
| Strong skills        | (the topic is already there; see below) |
| Not in profile       | Add to Deepening                        |

If the topic is already in Strong skills and the developer is *still*
skipping comprehension checks on it, that's expected — the prompt
should not fire for Strong skills topics in the first place
(`spec-narrator` doesn't surface those topics, so there's nothing to
skip). If somehow the prompt does fire (edge case: developer manually
moved a topic and then a stale rationale doc surfaced it), the prompt
should ask if the developer wants to *remove* the topic from the
profile entirely.

### Behavior on each option

**`[A]ccept`:**
1. Update `~/.pilot/profile.md` per the suggested transition
2. Update the profile's `Updated` section to the current timestamp
3. Delete all skip-log entries for this topic (any age, not just
   within the window)
4. Surface: "Profile updated. Skip log cleared for <topic>."

**`[D]ecline`:**
1. Do not modify the profile
2. Delete all skip-log entries for this topic
3. Surface: "Profile unchanged. Skip log cleared for <topic>. The
   calibration loop will start watching this topic again from zero."

**`[M]odify`:**
1. Open `~/.pilot/profile.md` in `$EDITOR` (or the user's configured
   editor; default to `vi` if unset)
2. Wait for the developer to save and close
3. Update the profile's `Updated` section to the current timestamp
4. Delete all skip-log entries for this topic
5. Surface: "Profile saved. Skip log cleared for <topic>."

In all three cases, skip entries for the affected topic wipe. The
prompt is the disambiguation event; once the developer has answered,
the counter resets and accumulation begins fresh.

Skip entries for **other** topics are not affected. The wipe is
topic-scoped.

## When the prompt does NOT fire

- The threshold was reached but the developer has already been prompted
  about this topic in the current session (don't ask twice per session)
- The topic is in Strong skills (it shouldn't have generated skips in
  the first place; if it did, something else is wrong)
- The pipeline is in `--autonomous` mode (no pedagogical surface, no
  prompts; calibration only fires in `--review` and `--paired` modes)
- The developer has set `calibration: off` in their config (see
  "Configuration")

## Configuration

The skip log defaults can be overridden by `~/.pilot/config.md`. This
file is optional; if absent, defaults apply.

```markdown
# PILOT config

## Skip-log weights

- comprehension-check: 3
- inline-explanation: 1
- rationale-doc-unread: 0

## Skip-log threshold

6

## Skip-log window

60 days

## Calibration

on
```

Changing weights only affects entries logged *after* the change.
Historic entries retain their original weight.

## What the skip log does NOT do

- **Track time spent.** This is a skip log, not a time tracker. Whether
  the developer spent 30 seconds or 30 minutes on a rationale doc is
  irrelevant; whether they skipped a comprehension check is what's
  logged.
- **Score the developer.** There's no "level" or "score". The profile
  tracks topic state; the skip log tracks evidence for changing topic
  state. Neither is a performance metric.
- **Get transmitted.** Anywhere. Ever. This is the most sensitive file
  PILOT writes — it records what the developer is uncertain about. It
  stays local.
- **Get committed to source control by default.** The installer
  creates `~/.pilot/` outside the project directory specifically so the
  skip log can't be accidentally committed.

## Privacy commitment

The skip log records "this developer didn't engage with the explanation
of X" multiple times. That's sensitive — it's a record of where the
developer is still uncertain. PILOT's commitment:

- The file stays at `~/.pilot/skip-log.md`. Local only.
- No agent ever reads the skip log without the developer's session
  having invoked a command that needs it (`--paired` mode,
  `/profile-review`).
- No content from the skip log is ever included in tool calls to
  external services, web fetches, or anything that leaves the
  developer's machine.
- The developer can delete the file at any time:
  `rm ~/.pilot/skip-log.md`. PILOT will keep working; calibration
  starts fresh.

## Inspecting the log

The file is plain markdown. The developer can:

- Read it: `cat ~/.pilot/skip-log.md`
- Search it: `grep "topic: react-hooks" ~/.pilot/skip-log.md`
- Count entries in the window:
  `grep -c "^## 2026-0[4-5]" ~/.pilot/skip-log.md`
- Edit it: `$EDITOR ~/.pilot/skip-log.md` (the developer may delete
  entries they no longer want counted; calibration re-reads from the
  current file on every evaluation)

Editing the log is allowed but unusual. The intended workflow is
that the calibration loop manages writes and the developer manages
the profile.

## Versioning

This protocol is at version 1. Future versions may add kinds of skip
(e.g., "rationale section closed without scrolling to end"), change
weights, or extend the prompt. The log file format is forward-compatible:
new fields can be added to entries without breaking older entries.
