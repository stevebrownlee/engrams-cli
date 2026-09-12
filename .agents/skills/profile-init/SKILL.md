---
name: profile-init
description: Initialize a developer profile at ~/.pilot/profile.md.
---

<!-- managed by PILOT — generated from commands/profile-init.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/profile-init.md and re-run install -->

# /profile-init

Walk the developer through producing a developer profile at
`~/.pilot/profile.md`. The profile drives PILOT's pedagogical layer:
which topics get explained, how deeply, and at which architectural
layers.

## Usage

```
/profile-init
/profile-init --upgrade
```

`--upgrade` migrates an existing profile from a previous schema
version to the current one. Without the flag, this command refuses if
a profile already exists (to prevent accidentally overwriting); the
developer can use `--upgrade` to migrate or use `/profile-review` to
edit an existing profile.

## What this command does

1. **Read context.**
   - `protocols/profile-schema.md` — the canonical profile format
   - `~/.pilot/` — verify the directory exists; create if not
   - `~/.pilot/profile.md` — check whether it already exists

2. **Decide the path.**
   - No existing profile → produce a fresh profile interactively
   - Existing profile, no `--upgrade` flag → refuse; point to
     `/profile-review` or manual edit
   - Existing profile, `--upgrade` flag → migrate to current schema
     while preserving the developer's content where possible

3. **Walk the developer through the sections.** For a fresh profile,
   ask one section at a time. Don't dump the full template and ask
   the developer to fill it in; that's a wall of work. Walk it.

   For each section, present what it's for, ask for input, and write
   a draft back for confirmation.

4. **Write the file atomically.** When the developer confirms, write
   `~/.pilot/profile.md` with all sections populated and `Updated`
   set to the current ISO 8601 UTC timestamp.

5. **Confirm.** Surface the path and a one-line summary:

   > "Profile written to ~/.pilot/profile.md.
   > Strong: <N> topics. Deepening: <M>. Learning: <K>.
   > Granularity: <vector summary>."

## The interactive walk

### Step 1: Strong skills

> "What topics do you want PILOT to skip explaining entirely? These
> are things you've used enough that any explanation is condescending
> noise. Use lowercase-kebab-case for each tag.
>
> Common examples: `typescript`, `react`, `postgresql`, `git`,
> `python`, `rest-apis`."

Collect bullet items. Push back if the developer is being too modest
(empty list is rare — almost every developer has at least one Strong
skill). Don't push back on a long list — long Strong skills sections
just mean a more experienced developer.

### Step 2: Deepening

> "What topics do you know well but are still refining? PILOT will
> include brief explanations (1-2 sentences) when these come up. The
> 'why' but not the 'how'."

Collect tags. Same format.

### Step 3: Currently learning

> "What topics are you actively learning? PILOT will give you full
> explanations with examples and trade-offs, and in --paired mode
> will surface comprehension checks."

Collect tags. Same format.

### Step 4: Granularity vector

This is the most consequential step. Explain the five layers and the
five levels, then ask the developer to set each.

> "PILOT's explanations are organized by five architectural layers,
> and for each layer you can pick how deep the explanation goes.
>
> The layers, from low-level to high-level:
> - `idiom` — language constructs (closures, async, generics)
> - `function` — how functions and modules are decomposed
> - `data-flow` — how data moves between functions/modules
> - `system` — how components interact at the system boundary
> - `architecture` — why the overall shape was chosen
>
> For each, pick a level:
> - `skip` — nothing on this layer
> - `skim` — one line
> - `moderate` — a paragraph, the why not the how
> - `deep` — full explanation with examples and trade-offs
> - `peer-level` — discussion as between senior engineers
>
> A common pattern for a mid-level developer might be:
>   idiom: skim, function: moderate, data-flow: deep,
>   system: deep, architecture: peer-level
>
> A staff engineer might use:
>   idiom: skip, function: skim, data-flow: moderate,
>   system: deep, architecture: peer-level
>
> Set yours:"

Collect the five levels. Validate that each is one of the five
recognized values. If the developer gives an out-of-range value, ask
again with the valid set.

### Step 5: Notes

> "Anything PILOT should know about you that doesn't fit the sections
> above? Free-form prose. Examples:
> - 'I'm in a backend rotation; UI explanations are useful even
>   though I've used React for years.'
> - 'When using Prisma, prefer explaining migration ordering — that's
>   where I keep tripping.'
> - 'Don't suggest I add comments. I prefer dense code with
>   descriptive names.'
>
> Skip if nothing comes to mind."

Empty is fine.

### Step 6: Confirm and write

Show the developer the assembled profile. Ask for final confirmation.
On `yes`, write `~/.pilot/profile.md` with all sections plus the
`Updated` timestamp.

## --upgrade behavior

When the existing profile is on a previous schema version:

1. Read the existing profile and parse what's there
2. Identify missing sections that the current schema requires
3. Walk the developer through only those missing sections
4. Preserve all content from the old profile that maps cleanly to the
   new schema
5. Write the migrated profile and update the `Updated` timestamp

If the old profile has sections that the new schema removed, ask the
developer whether to preserve them as Notes content or drop them.

## What you do NOT do

- **You do not auto-infer the profile from past work.** Profile
  authoring is explicit; you ask, the developer answers.
- **You do not transmit the profile anywhere.** Local-only.
- **You do not modify any project file.** The profile is
  per-developer.
- **You do not validate the developer's tags against an external
  vocabulary.** Whatever lowercase-kebab-case tags they want are
  fine — the agents will fuzzy-match against them as work proceeds.
- **You do not refuse a profile that seems unusual.** A developer
  with everything in Strong skills is unusual but legitimate (a
  senior who genuinely wants no pedagogical content); just write what
  they tell you.

## Error cases

| Condition                              | Response                                          |
|----------------------------------------|---------------------------------------------------|
| `~/.pilot/` exists but unwritable      | Refuse with permission error                      |
| Existing profile, no `--upgrade`       | Refuse; point to `/profile-review` or manual edit |
| Developer aborts mid-walk              | Save partial profile to `~/.pilot/profile.md.draft` and tell them how to resume |
| Granularity level out of range         | Ask again with the valid set                      |
| Tag format invalid                     | Show the format rule; ask again                   |

## Resuming a draft

If the developer aborts and a draft exists:

```
A profile draft exists at ~/.pilot/profile.md.draft from <date>.
Resume from where you left off, or start fresh? [resume/fresh]
```

Resume picks up at the section where the draft ends. Fresh discards
the draft.

## After writing the profile

Surface a one-line note about what to do next:

> "Profile ready. Run `/spec-implement <id> --review` or
> `--paired` to see the rationale doc calibrated to your profile."
