---
name: profile-review
description: Review and apply suggested profile updates from the calibration loop.
---

<!-- managed by PILOT — generated from commands/profile-review.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/profile-review.md and re-run install -->

# /profile-review

Process any pending suggested updates to `~/.pilot/profile.md` produced
by PILOT's calibration loop. When the skip log shows a topic has
crossed the threshold (6 weighted points within the sliding 60-day
window), this command surfaces the suggestion and lets the developer
accept, decline, or modify.

This command is normally invoked automatically by the orchestrator
when a threshold is crossed mid-pipeline. It can also be invoked
manually if the developer wants to check for pending suggestions.

## Usage

```
/profile-review
```

No arguments. Operates on `~/.pilot/profile.md` and
`~/.pilot/skip-log.md`.

## What this command does

1. **Read context.**
   - `protocols/profile-schema.md` — profile format
   - `protocols/skip-log.md` — skip-log format and threshold rules
   - `~/.pilot/profile.md` — the current profile
   - `~/.pilot/skip-log.md` — the skip log

2. **Refuse if prerequisites are missing.**
   - No profile → suggest `/profile-init` first
   - No skip log → surface "no calibration data yet"

3. **Identify topics that have crossed the threshold.**
   - For each unique topic in the skip log, sum weights for entries
     within the last 60 days
   - Topics with a sum ≥ 6 are candidates for review

4. **If no candidates, exit cleanly.**

   > "No topics have crossed the calibration threshold. The skip log
   > is healthy."

5. **For each candidate topic, present the prompt** and act on the
   developer's choice. Loop through all candidates in order of
   highest-weighted-sum first (i.e., the most overdue prompt fires
   first).

## The calibration prompt

For each candidate topic:

```
Topic "<topic>" has accumulated <N> weighted skip points across
the last <days> days. Has your understanding of this topic
strengthened?

Current section: <Strong skills | Deepening | Currently learning | (not in profile)>
Suggested move:  <Strong skills | Deepening | (none — already at top)>

If yes, PILOT can update your profile:
   Move "<topic>": <current> → <suggested>
   Wipe the <count> skip entries for this topic.

[A]ccept   [D]ecline   [M]odify

[A]ccept    Apply the suggested change.
[D]ecline   Keep profile unchanged. Skip entries wipe (the developer
            affirmed expediency, not mastery).
[M]odify    Open ~/.pilot/profile.md in $EDITOR; you decide the
            change manually. Skip entries wipe.
```

Wait for the developer's response. Don't auto-time-out; the developer
should answer when they're ready.

## Behavior on each option

### `[A]ccept`

1. Read `~/.pilot/profile.md`
2. Move the topic from its current section to the suggested section
3. Update the `Updated` timestamp to the current ISO 8601 UTC
4. Write the file atomically
5. Read `~/.pilot/skip-log.md`
6. Remove all entries where `topic: <topic>` matches (any age)
7. Write the skip log atomically
8. Surface:

   > "Profile updated: <topic> moved <current> → <suggested>.
   > Skip log cleared for this topic."

### `[D]ecline`

1. Do NOT modify the profile
2. Read `~/.pilot/skip-log.md`
3. Remove all entries where `topic: <topic>` matches (any age)
4. Write the skip log atomically
5. Surface:

   > "Profile unchanged. Skip log cleared for <topic>. The calibration
   > loop will start watching this topic again from zero."

### `[M]odify`

1. Open `~/.pilot/profile.md` in `$EDITOR` (or `vi` if unset)
2. Wait for the developer to save and close the editor
3. Update the `Updated` timestamp to the current ISO 8601 UTC
4. Read `~/.pilot/skip-log.md`
5. Remove all entries where `topic: <topic>` matches (any age)
6. Write the skip log atomically
7. Surface:

   > "Profile saved (developer-edited). Skip log cleared for <topic>."

## Special cases

### Topic already in Strong skills

If a topic has crossed the threshold but is already in Strong skills,
this is unusual — Strong skills topics shouldn't generate skips
(spec-narrator omits them entirely, so there's nothing to skip).

When this happens, the prompt is different:

```
Topic "<topic>" is in your Strong skills section but has accumulated
<N> weighted skip points. This usually means spec-narrator surfaced
content about this topic despite the Strong skills classification —
likely because the Notes section overrides Strong skills for this
context, or because the topic tag was used differently than expected.

Options:
[R]emove   Remove "<topic>" from the profile entirely
[K]eep     Keep the topic in Strong skills (the skip log clears)
[M]odify   Edit ~/.pilot/profile.md manually
```

### Topic not in profile

If a topic has crossed the threshold but isn't anywhere in the profile,
the suggested move is "Add to Deepening" (since the developer has
shown enough engagement with this topic to warrant calibration but the
profile hasn't acknowledged it yet).

### Multiple candidate topics

If three topics have crossed the threshold, the prompt fires three
times in sequence. The developer can `[A]ccept`, `[D]ecline`, or
`[M]odify` each independently.

After all candidates are processed, surface a summary:

```
Calibration review complete.
- 2 topics accepted
- 1 topic declined
- Skip log entries cleared for all 3 topics
```

### Developer aborts mid-review

If the developer abandons the review (closes the terminal, hits
Ctrl-C), the topics not yet reviewed remain in the skip log
unprocessed. Next invocation of `/profile-review` (manual or
orchestrator-triggered) will surface them again.

Topics that *were* processed retain their effect: an accepted update
stays; a declined topic stays cleared in the skip log.

## When the orchestrator invokes this

In `--review` and `--paired` modes, the orchestrator checks the skip
log after each skip event. When a topic crosses the threshold mid-spec,
the orchestrator pauses the pipeline and invokes `/profile-review`.

In this auto-invocation case, after `/profile-review` exits, the
orchestrator resumes the paused pipeline. The developer sees:

> "Calibration check: topic <topic> crossed threshold."
> [profile-review runs]
> "Resuming spec <ID> at phase <N>..."

## What you do NOT do

- **You do not auto-apply profile updates.** Always prompt; never
  modify the profile without explicit `[A]ccept`.
- **You do not modify the profile on `[D]ecline`.** The whole point
  is that decline means no profile change.
- **You do not modify the skip log entries' content.** You delete
  matching entries entirely; you don't edit them.
- **You do not modify other topics' skip entries.** The wipe is
  topic-scoped.
- **You do not transmit any data.** Profile and skip log are
  local-only.
- **You do not show the skip log entries in the prompt.** The
  developer can `cat ~/.pilot/skip-log.md` if they want to inspect.
  The prompt shows the summary (count and weighted sum), not the
  entries.

## Error cases

| Condition                                | Response                                          |
|------------------------------------------|---------------------------------------------------|
| `~/.pilot/profile.md` missing            | Suggest `/profile-init` first                     |
| `~/.pilot/skip-log.md` missing           | Surface "no calibration data yet"                 |
| Profile is malformed                     | Refuse; suggest `/profile-init --upgrade`         |
| Skip log is malformed                    | Refuse; let the developer inspect manually        |
| `$EDITOR` unset for `[M]odify`           | Use `vi`; surface a note that `$EDITOR` was unset |
| Editor exits non-zero (e.g., :q!)        | Treat as `[D]ecline` for this topic               |
| Developer enters unrecognized response   | Ask again; show the three valid options           |

## After the review

The command exits cleanly. If invoked by the orchestrator, control
returns to the orchestrator. If invoked manually, the developer
returns to their shell.
