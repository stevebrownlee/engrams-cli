# 0008 — Schema Prompts at Session Close

## Summary

A schema proposal needs someone to notice the pattern first. Writers and developers rarely stop to ask what their last month of notes adds up to, and the end of a working session is the natural moment to ask. This feature has engrams do the asking.

When a session closes, engrams looks for storylines that keep coming back but that no schema explains yet. These include groups the detector found, even the uncertain ones it used to hide, and runs of notes that share a subject tag, such as a character's name across many chapters. It hands the agent at most three prompts. Each prompt lists its notes in reading order, asks questions suited to the kind of project, and includes a ready-to-fill command to propose a schema. The agent drafts a claim from the prompt and asks the person, in plain words, whether it is right.

The questions come from small project-type question sets called lenses:

- The fiction lens asks what false belief a character holds, what wound caused it, how it shows in dialogue and action, and which notes contradict it.
- The software lens asks what must always hold true, where the boundary is, and what breaks if it is violated.

Lenses also name generic tags, such as "plot" or "chapter", that never form a storyline on their own. Engrams ships fiction, software, and general lenses, and a project can replace any of them with its own file.

Prompts respect the person's time:

- A prompt appears once for a given set of notes, and comes back only when a new note joins it.
- A subject already explained by a schema, or waiting in a proposal, is not prompted again.
- Evidence a person rejected stays quiet until something new arrives.

Session close also names the schemas that guided the work, so the value is visible each time. The test is concrete: in the test novel, the protagonist's storyline is among the first three prompts and the decoy material is not.

## Acceptance criteria

### AC-1: Offer the protagonist's unexplained storyline among the top three prompts

**Given** the test novel with planted storylines and decoy notes, and no schemas or proposals
**When** a session closes with the fiction lens
**Then** at most three prompts are returned
**And** one of them is the protagonist's storyline
**And** no prompt is built from decoy notes

### AC-2: List each prompt's evidence in reading order with lens questions

**Given** a storyline whose notes are attached to the files for chapters 2, 10, and 1
**When** its prompt is produced
**Then** the notes are listed in the order chapter 1, chapter 2, chapter 10
**And** the prompt carries the lens's questions with the storyline's subject filled in
**And** it includes a propose command that names the storyline and its notes

### AC-3: Show a prompt again only after a new note joins it

**Given** a prompt that was shown at one session close
**When** later sessions close while notes are removed from or regrouped within that storyline, but no note is added
**Then** the prompt is not shown again
**But** it is shown again at the first session close after a newly created note joins the storyline

### AC-4: Skip subjects and evidence that are already handled

**Given** one subject storyline whose tag is in an active schema's scope, one whose tag is in a pending proposal's scope, and one storyline matching a rejected proposal with no newer note
**When** a session closes
**Then** none of the three storylines is prompted
**And** a detector group whose notes are mostly members of an active schema is not prompted either

### AC-5: Keep generic tags from forming storylines

**Given** many notes tagged with a category word that the active lens ignores, such as "plot"
**When** prompts are produced with that lens
**Then** no prompt is built around that tag
**And** the same notes can still appear in prompts built around their subject tags

### AC-6: Prompt from detector groups of any label

**Given** a detector group that passes the formation gates but is labeled unclear
**When** a session closes
**Then** the group can appear as a prompt, with its label shown

### AC-7: Take questions from a project lens file when one exists

**Given** a lens file named fiction in the project's workspace
**When** prompts are produced with the fiction lens
**Then** the questions and ignored tags come from the project's file
**And** requesting a lens that does not exist fails with an error listing the available lenses

### AC-8: Preview prompts without marking them shown

**Given** prompts that have not been shown yet
**When** the prompts are previewed outside a session close
**Then** the same prompts are returned
**And** the next session close still shows them

### AC-9: Record where a prompted proposal came from

**Given** a prompt for a subject storyline
**When** an agent proposes a schema with the prompt's command
**Then** the proposal records the storyline as its source and the lens it used

### AC-10: Name the schemas that guided the session

**Given** schemas that surfaced during a session through priming, pre-edit advice, or search
**When** the session closes
**Then** the output names each of those schemas with how many times it surfaced, most-used first

### AC-11: Carry a storyline from notes to a working schema using only printed commands

**Given** the test novel after ordinary note logging, with no schemas or proposals
**When** a session closes, the agent runs the propose command printed in the protagonist's prompt with its own claim, and the confirm command printed by the proposal is run after the person says yes
**Then** the next primed session and the pre-edit advice for a new chapter both show the confirmed claim and instruction
**And** no step needed an ID or a command other than the ones printed by the step before it

### AC-12: Keep upgraded databases from repeating suggestions already shown

**Given** a database from the previous release in which session close showed a detector group's suggestion and no later scan has refreshed that group
**When** the database is upgraded and the next session closes
**Then** that group is not prompted
**But** it is prompted once a newly created note joins its evidence

## Out of scope

- Detecting character or place names in note text; this release uses tags only
- Adding "same subject" links to the grouping step; the test book measures that idea instead (Verification strategy)
- Counting recurrence across chapters in reading order, and renaming the "story" label (a later phase)
- Remembering a default lens for a workspace (see Open questions)
- Model calls inside engrams; the agent answers the prompts
- Lenses that change detection; lenses only shape questions, ignored tags, and evidence order
- Enforcing how an agent words its question to the person; engrams supplies the prompt and the instructions, and the agent follows them

## Open questions

- Should a workspace remember its lens, so session close needs no flag?
- Are subject tags enough to find most storylines in real manuscripts, or does name detection need to follow soon?
- Should the subject-tag share cap (80% of tagged notes) be tuned per lens after dogfooding?

## Architecture

A new module, `src/ops/schemas/prompts.rs`, builds prompts. It is read-only and deterministic: sorted inputs, fixed tie-breaks, and no wall-clock in ranking. `src/ops/session.rs` replaces its `schema_opportunities` block with `schema_prompts`. `src/ops/schemas/opportunities.rs` is deleted (clean cutover), and its `suggest_schema_name` helper moves to `src/ops/schemas/propose.rs`, which spec 0007 already uses for the default proposal name.

**Two evidence sources, one shape** (`source`, `subject`, evidence keys):

1. **Candidates:** staged `schema_candidates` rows that pass `gates_pass` (`gates.rs:17`), of any `kind`. Only their knowledge members count as evidence.
2. **Subject threads:** for each tag, the active notes carrying it. A tag forms a thread only if all of these hold:
   - at least `MIN_EVIDENCE` (3, from spec 0007) active notes carry it
   - it is not in the lens's `ignore_tags`
   - at most `THREAD_MAX_SHARE = 0.8` of tagged active notes carry it

**Coverage.** A subject thread is skipped when its tag appears in the scope tags of an active schema or a pending proposal. Tags are compared convention-aware through `ident_parts`. Once a schema speaks for a subject, leftover notes about that subject reach it through log-time suggestions rather than new prompts. A candidate is skipped when fewer than half of its knowledge members are uncovered. A note is covered when it is a member of an active schema or is cited by a pending proposal. A remaining prompt survives these filters, in order:

1. Its uncovered evidence must pass spec 0007's propose checks: at least three notes, spread over at least two files or two days. Every prompt can therefore become a valid proposal.
2. It is dropped when it overlaps a higher-ranked prompt at Jaccard 0.7 or more (`jaccard`, `gates.rs:39`).

Prompts rank by uncovered notes, then files, then days (all descending), then key (ascending). The top `K = 3` are returned.

**Recurrence uses evidence, not keys.** A scan rewrites a candidate's signature whenever it refreshes the group (`scan.rs:215-219`), so a key built from it cannot tell an old prompt from a new one. Each time session close shows a prompt, it records the prompt's evidence in `schema_prompt_notices`. A prompt is held back when an earlier notice, or a rejected proposal, overlaps its evidence at Jaccard 0.7 or more and none of its notes was created after that notice's `notified_at` (or the proposal's `decided_at`). Removing or regrouping notes never adds a newly created note, so it cannot revive a prompt. `schema prompts` previews and never writes. `schema_candidates.notified_at` is dropped, because one notice mechanism now covers both sources. Notices are session state and are not exported.

**Upgrades carry suppression over.** Before the column is dropped, the migration copies into `schema_prompt_notices` the suggestions the old release is still holding back:

- The old release only ever showed `kind = 'schema'` candidates (`opportunities.rs:26`).
- It showed a candidate again as soon as a later scan refreshed it (`notified_at < last_seen_at`).

So the copy takes exactly the `kind = 'schema'` rows whose `notified_at` is at or after their `last_seen_at`. Both timestamps use the same RFC 3339 seconds format (`scan.rs:31-33`), so comparing them as text orders them correctly. These rows' members have not changed since they were shown, so the copied evidence is what the person saw. Only knowledge members are kept, so the comparison matches what prompts now use. A suggestion the person has already seen stays quiet after the upgrade under the newer-note rule, and nothing the old release would have shown again is newly hidden.

We chose subject threads as a separate evidence source rather than "same subject" links inside Louvain grouping. A strong subject link would pull a character's wound and her romance into one "everything about her" bucket, which is a topic rather than a theme. The agent, not the grouping step, picks the notes that support a claim. Verification measures this prediction on the test book. We chose tags over name detection because tags are already curated by the agent that logs the notes, and they keep detection deterministic.

**Reading order.** A lens's `order` field controls how evidence is listed:

- `story` sorts notes by their first anchor path using a new `natural_cmp` helper, which compares digit runs numerically so `ch2` sorts before `ch10`. No crate is added, because no natural-sort helper exists in `src/`. Ties fall back to timestamp.
- `time` sorts by timestamp.

**Lenses.** Lenses are JSON files compiled in with `include_str!`, following the precedent of `src/assets/instructions.md`: `src/assets/lenses/general.json`, `fiction.json`, and `software.json`. A workspace file at `<root>/engrams/lenses/<name>.json` replaces the built-in lens of the same name. The root comes from spec 0006; with no workspace, only built-ins load. Lens files are parsed with serde `deny_unknown_fields`. They contain only strings, so loading one runs no code. Questions may contain `{subject}`, which is replaced by the thread tag, or by "these notes" for candidates. The default lens is `general`.

**Visible value.** `schema_value_rollup` (`session.rs:226-252`) already counts schema surfacings in the session window. It also groups them by schema and returns names with counts.

**The agent loop is documented where agents read it.** The Session End Protocol and command tables in `AGENTS.md`, `src/assets/instructions.md` (printed by `engrams instructions`), `docs/memory/engrams.md`, and the cognitive-schemas guide page all gain the same steps:

1. When `schema_prompts` is non-empty, read each prompt's evidence and answer its questions.
2. Draft one claim, instruction, and scope, and run `schema propose`.
3. Show the person the claim, the instruction, the scope, and the notes behind them in plain words, and ask whether to confirm, edit, or reject.
4. Run `schema confirm` or `schema reject` with the person's own words.
5. Raise `pending_proposals` from `prime` the same way at session start.
6. When a surfaced schema shapes your work, name it in your reply.

**Provenance.** `schema propose --thread <tag> --lens <name>` records `source_kind = 'thread'`, `source_ref = <tag>`, and `lens`. This extends spec 0007's snapshot provenance.

Rules 002, 005, and 006 apply: dynamic `IN` lists borrow parameters; tag counting and ranking fold iterators without throwaway collections; and lens order and source-kind vocabulary are `&'static str`.

## Data model

Migration `MIGRATION_V17` with identical shapes in the baseline `SCHEMA`; `LATEST_VERSION = 17`.

```sql
CREATE TABLE IF NOT EXISTS schema_prompt_notices (
  source_key    TEXT PRIMARY KEY,  -- 'thread:<tag>' | 'candidate:<signature when shown>'; display identity only
  evidence_json TEXT NOT NULL,     -- sorted evidence keys as shown; recurrence compares these, never keys
  notified_at   TEXT NOT NULL
);
ALTER TABLE schema_proposals ADD COLUMN lens TEXT;
-- carry over the suggestions the old release still holds back: shown schema-kind rows that no later scan has refreshed
INSERT OR IGNORE INTO schema_prompt_notices (source_key, evidence_json, notified_at)
SELECT 'candidate:' || c.cluster_sig,
       (SELECT json_group_array(j.value) FROM json_each(c.member_keys_json) j
         WHERE j.value NOT LIKE 'code:%' AND j.value NOT LIKE 'schema:%'),
       c.notified_at
FROM schema_candidates c
WHERE c.kind = 'schema' AND c.notified_at IS NOT NULL AND c.notified_at >= c.last_seen_at;
ALTER TABLE schema_candidates DROP COLUMN notified_at;
```

`schema_proposals.source_kind` gains the value `'thread'`.

The copy step is a one-time data carry-over inside `MIGRATION_V17`. A fresh database runs only the baseline `SCHEMA`, which has no rows to copy. `member_keys_json` is already sorted (`scan.rs:41-45`), and `json_each` walks it in order, so the copied evidence stays sorted.

`ALTER TABLE ... DROP COLUMN` needs SQLite 3.35 or later; the bundled SQLite in `rusqlite` 0.32 is newer than that.

Lens file shape:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    pub name: String,
    pub description: String,
    pub questions: Vec<String>,   // may contain {subject}
    pub ignore_tags: Vec<String>, // compared convention-aware via ident_parts
    pub order: Order,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Order { Story, Time }

pub const THREAD_MAX_SHARE: f64 = 0.8;
pub const PROMPTS_PER_CLOSE: usize = 3;
pub const COVERED_SHARE: f64 = 0.5; // candidates with fewer uncovered members than this share are skipped
```

Built-in fiction lens:

```json
{
  "name": "fiction",
  "description": "Novels and long-form narrative",
  "questions": [
    "What false belief does {subject} hold about themselves or the world?",
    "What wound or past event caused it?",
    "How does it show in dialogue and in action?",
    "Which notes contradict it, and is it changing?"
  ],
  "ignore_tags": ["plot", "chapter", "scene", "character", "dialogue", "draft", "pov", "style", "voice", "prose"],
  "order": "story"
}
```

## API surface

1. **`engrams session close [...] [--lens <name>]`.** The output key `schema_opportunities` is replaced by `schema_prompts`:
   ```json
   "schema_prompts": [{
     "key": "thread:mara",
     "source": {"kind": "thread", "ref": "mara"},
     "label": null,
     "lens": "fiction",
     "questions": ["What false belief does mara hold about themselves or the world?"],
     "evidence": [{"kind": "decision", "id": 4, "summary": "...", "anchors": ["manuscript/ch01.md"], "timestamp": "..."}],
     "spread": {"notes": 11, "files": 9, "days": 6},
     "propose_command": "engrams schema propose --thread mara --lens fiction --members decision:4,... --name <name> --statement \"<claim>\" --summary \"When <condition>: <directive>\""
   }]
   ```
   For candidate prompts, `label` is the detector's kind label and `source` is `{"kind": "candidate", "ref": "<signature prefix>"}`.
2. **`schema_value`** keeps `guided_retrievals` and `distinct_schemas`, and gains `"schemas": [{"id": 4, "name": "mara-broken-leader", "guided": 5}]`, most-used first. It is omitted under the same rule as today, when no session key is known.
3. **`engrams schema prompts [--lens <name>] [--limit <n>]`** returns `{"status": "success", "lens": "<name>", "prompts": [...]}` with the same entries. It never writes notices. `--limit` defaults to 3.
4. **`engrams schema propose`** gains `--thread <tag>` (sets the source) and `--lens <name>` (recorded). Proposal payloads gain `lens`.
5. **Errors:**
   - Unknown lens: `{"error": "unknown lens 'fantasy': available general, fiction, software"}`
   - Malformed lens file: `{"error": "lens file engrams/lenses/fiction.json: unknown field `qestions`"}`

## Dependencies

Internal:
- Spec 0006 (resolved workspace root for lens files)
- Spec 0007 (`schema_proposals`, `schema propose`, propose evidence checks, scope, `pending_proposals` in prime, `tests/fixtures/manuscript-book.json`)
- `src/ops/schemas/gates.rs` (`gates_pass`, `jaccard`) and `scan.rs` staging
- `src/ops/session.rs` (`schema_value_rollup`)

External:
- None. Uses the existing `serde`, `serde_json`, `rusqlite`, and `include_str!`.

## Verification strategy

Per-phase verification commands (Gate 3):

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test prompt
cargo test --test cli session_close
cargo test schema_loop
cargo test migration
```

Full-suite verification (Gate 5):

```bash
cargo test
cargo build
npm --prefix docs install && npm --prefix docs run build
```

Manual:
1. In a temporary workspace loaded from the test-book fixture, run `engrams session close --lens fiction` and read the three prompts. Propose and confirm the protagonist schema from its prompt's command. Run `engrams advise manuscript/ch19.md` and `engrams session close` again, and confirm two things: the protagonist's storyline is no longer prompted, and `schema_value.schemas` names the protagonist schema.
2. Measurement for the design choice. In a copy of that workspace, add `relates_to` links between every `mara`-tagged note, run `schema scan` three times, and record whether the wound notes and the romance notes land in one group. Store the result in custom data `research/human-level-schema-emergence-2026-10`.
3. Export this project's database and import it into a fresh temporary workspace. Run `engrams schema prompts --lens software` there, confirm no prompt repeats the `performance-rules` or `what-to-build-next` territory, and judge whether the prompts name real recurring concerns.
4. Run `engrams instructions` and confirm the session-end steps describe proposing, asking the person, and recording their answer.
5. Copy this project's database file into a temporary workspace (the live file stays untouched) and run `engrams schema prompts` there, which upgrades the copy. Confirm no group that an earlier session close already showed is prompted again.
