# 0007 — Reviewed Schema Proposals

## Summary

Engrams can already group related notes, but a group of notes is not yet an idea. A writer's notes about a protagonist only become useful together once someone says what they mean. She cannot do what her peers can. Her dialogue is downcast. She hesitates before her first command. She is made captain and still calls herself "just the one left standing." What they mean is that she believes she is broken, and that hurt follows her as she grows into a leader.

Today the tool turns note groups into schemas named after their most common tag, with summaries stitched together from the notes themselves. It can do this in bulk without anyone reading the result. In a test novel this produced six thin schemas named "mara", "broken", "guilt", "guilt-2", "guilt-3", and "care". None of them said the thing a person would say.

This feature changes how schemas come into existence. An agent proposes a schema with four parts: a claim in plain words, an instruction for when and how to apply it, the scope where it applies, and the notes it rests on. Engrams checks the evidence:

- there are enough notes
- the notes span more than one file or day
- every note is still current
- the claim is not just one note repeated

The proposal then waits for a person. The person confirms it, edits it while confirming, or rejects it. A rejection is remembered, so the same evidence is not proposed again until something new arrives. A proposal left undecided is raised again at the start of the next session, so nothing waits unseen. Nothing becomes a schema without that decision. The bulk shortcut is removed, and groups found by the detector become starting points for proposals instead of schemas.

Once confirmed, a schema shows up wherever its scope applies, including new work it has never seen. When the writer starts chapter nineteen, the protagonist's schema is offered before the first line is written. Its scope covers the manuscript and the protagonist's name, not only the chapters its notes came from. Wherever a schema appears, it says why it appeared and which proposal it came from. Existing schemas keep working and can be given a claim and a scope.

## Acceptance criteria

### AC-1: Record a proposed schema without surfacing it

**Given** at least three current notes attached to at least two different files
**When** an agent proposes a schema citing those notes with a claim, an instruction that opens with a trigger phrase, and a scope
**Then** the proposal is listed as pending with its claim, instruction, scope, cited notes, and evidence counts
**And** no schema exists for it, no note gains a schema membership, and pre-edit advice and search return nothing new

### AC-2: Refuse a proposal that fails an evidence or wording check

**Given** a proposal that fails exactly one check: fewer than three notes; notes on one file and one day; a missing or retired note; a code file cited as a note; a counter-evidence note that is missing, retired, or also cited as evidence; a claim that repeats one cited note; or an instruction without a trigger phrase
**When** the agent submits it
**Then** nothing is recorded
**And** the error names the check that failed

### AC-3: Create the schema when a person confirms the proposal

**Given** a pending proposal
**When** a person confirms it, optionally changing its name, claim, instruction, or scope
**Then** an active schema exists with the confirmed name, claim, instruction, and scope, and with the cited notes as members
**And** the schema's instruction is ranked as authored rather than drafted
**And** the proposal is marked confirmed and points to the new schema
**And** inspecting the schema shows the proposal it came from and when it was proposed and confirmed
**And** when the name has been taken in the meantime, the schema is saved under the next free variant of the name and the output reports the change

### AC-4: Refuse a proposal that repeats rejected, pending, or confirmed territory

**Given** an earlier proposal or schema covering mostly the same notes: a pending proposal, an active schema, or a proposal a person rejected with no note created since the rejection
**When** an agent proposes a schema citing those notes
**Then** nothing is recorded
**And** the response names the earlier proposal or schema, and for a rejection also its date and reason

### AC-5: Surface a confirmed schema for a new file inside its scope

**Given** a confirmed schema about the protagonist whose scope names the manuscript folder and the protagonist's tag, built from notes on chapters one through eighteen
**When** pre-edit advice is requested for a new chapter-nineteen file that no note mentions
**Then** the schema is listed with its claim, its instruction, and the reason it matched
**And** the harness advisor's message to the agent before its first edit of that file, including the edit that creates it, shows the same claim, instruction, scope, and reason ahead of the file's rules

### AC-6: Find a confirmed schema through its scope tags

**Given** the confirmed protagonist schema from AC-5
**When** a search mentions the protagonist's name, or a session is primed with the protagonist's tag
**Then** the schema is returned ahead of individual notes, with the reason it matched

### AC-7: Start a proposal from a detector group and keep its evidence fixed

**Given** a group of related notes and files found by the detector
**When** an agent proposes a schema from that group
**Then** the proposal's evidence is the group's notes, without its code files
**And** the proposal records which group it came from
**And** later scans that change the group's members leave the proposal's evidence unchanged

### AC-8: Create schemas only through confirmed proposals

**Given** detector groups that pass every formation gate
**When** a person runs a scan and then asks to confirm one of its groups directly
**Then** the scan creates no schema
**And** the confirmation request fails with the command that proposes from that group
**And** session close offers a ready-to-fill propose command for each group instead of a confirm command

### AC-9: Give an existing schema a claim and a scope

**Given** a schema confirmed before this release, which has no claim or scope
**When** an agent refines it with a claim and a scope
**Then** the schema shows the claim and scope
**And** pre-edit advice lists it for a new file inside the scope

### AC-10: Round-trip proposals and schema claims through export and import

**Given** pending, confirmed, and rejected proposals, and schemas with claims and scopes
**When** the database is exported and the export is imported into a fresh workspace
**Then** every proposal returns with its status, claim, instruction, scope, cited notes, evidence counts, and decision note
**And** every schema returns with its claim, its scope, and the proposal it came from

### AC-11: Raise undecided proposals at the start of the next session

**Given** proposals that were still pending when earlier sessions ended
**When** the next session is primed
**Then** the priming output lists the pending proposals, oldest first and at most three, each with its claim, evidence counts, and ready-to-run confirm and reject commands

### AC-12: Show the schema even when a rule violation blocks the edit

**Given** a confirmed schema whose scope covers an existing file, and an error-severity rule that the file's current content violates
**When** the harness advisor blocks the agent's first edit of that file
**Then** the block reason shows the schema's claim, instruction, scope, and match reason ahead of the violations

## Out of scope

- Session-close prompts built from storylines that share a subject tag, project-type question sets (lenses), and once-per-evidence prompt notices (spec 0008)
- Schemas nested inside other schemas, including proposals whose notes contain an existing schema's notes (a later phase; the existing territory rule still applies)
- Retiring or archiving a schema; no command sets a schema's status today
- Counting recurrence across chapters in reading order, and renaming the "story" label (a later phase)
- A relationship for decisions that show a character's growth rather than a replacement (a later phase)
- Tracking evidence for and against confirmed schemas over time and flagging them for review (a later phase)
- Model calls inside engrams; claims are written by the agent, never generated by the tool
- Expiring pending proposals (see Open questions)
- Harness integrations beyond the omp advisor extension; other agents keep receiving schemas through prime, advise, and query
- A continuous-integration test workflow; the repository's workflows only build release binaries and the docs site, so the Rust and Bun checks run locally at each gate

## Open questions

- Should pending proposals expire after some number of sessions without a decision?
- Should the minimum evidence (three notes over two files or two days) differ by project type once lenses exist in spec 0008?

## Architecture

The feature adds a proposal lifecycle beside the existing detector. It adds `src/ops/schemas/propose.rs` and changes `confirm.rs`, `scan.rs`, `opportunities.rs`, `retrieval.rs`, `assimilate.rs`, and `list.rs` in that directory. It also changes `src/ops/prime.rs`, `src/ops/advise.rs`, `src/cli.rs`, `src/schema.rs`, `src/ops/transfer/`, and the omp advisor extension `src/assets/engrams-advisor.ts`.

**Three stores, kept separate:**

| Store | Holds | Written by | Lifetime | Surfaced to agents |
|---|---|---|---|---|
| `schema_candidates` | detector groups (evidence) | `schema scan` | persistent staging: a scan updates each group it finds again, refreshing members and signature, and leaves the others untouched (`scan.rs:194-245`) | never |
| `schema_proposals` (new) | claim, instruction, scope, cited notes, provenance, decision | `propose`, `confirm`, `reject` | durable, exported | pending ones in `prime` only |
| `schemas` | confirmed concepts | `confirm` | durable, exported | prime, advise, query, brief, the advisor extension |

We chose a separate `schema_proposals` table over the reserved but unused `schemas.status = 'proposed'` value (no code writes it), for three reasons:

1. Every reader of `schemas` filters on `status = 'active'` and assumes membership means confirmation. Examples are `retrieval.rs:120-121` and `:281-282`, and `assimilate.rs:65-66`.
2. A status-based proposal would have to write `member_of` links. `graph/model.rs` loads every link into every analytic, so those links would feed back into detection before anyone had decided.
3. Pending human decisions already live outside main tables; `schema_suggestions` (suggested, accepted, declined) is the precedent.

The status vocabulary is also inconsistent today: `schema.rs:184` says `archived`, while `cli.rs:418`, the list tests, and decision 91 say `retired`. The baseline comment becomes `'active' | 'needs_review' | 'retired'`.

**Propose.** `schema propose` validates everything before writing anything; the full order is in API surface. In short:

- cite current knowledge notes only, at least three, spread across two files or two days
- the claim must not restate any single note: token overlap below 0.8, using `item_tokens` (`assimilate.rs:42`) and `jaccard` (`gates.rs:39`)
- the instruction must pass `refine_text` (`list.rs:238`)
- the evidence must not repeat pending, rejected, or confirmed territory

When the scope is omitted, its tags default to those carried by at least two cited notes, and its paths to the deepest directory shared by every cited note's anchors. Evidence counts are computed by engrams, never supplied by the agent, because a model's self-rated confidence tends to be overconfident (Xiong et al. 2023, arXiv:2306.13063).

**Provenance is a snapshot.** `members_json` holds the cited keys and is the authoritative evidence. `source_ref` keeps the detector group's signature as plain text, never as a key, because a scan rewrites a group's signature whenever it refreshes the group's members (`scan.rs:215-219`).

**Confirm and reject.** `schema confirm proposal:<id>` re-runs every check on the possibly edited values. In one transaction it then:

- inserts the schema with `summary_source = 'agent'`, `kind = 'schema'`, a `centroid()` built from the members, the `statement`, and `scope_json`
- writes `member_of` links through the existing write path in `confirm.rs`
- marks the proposal `confirmed` and records `schema_id`

`confirm <schema id|name>` keeps its re-confirmation bump (`confirm.rs:529-562`). `schema reject proposal:<id> --reason` records the decision.

**Cutover.** These are deleted:

- `--apply` and its loop (`scan.rs:340-373`), which supersedes decisions 81 and 85
- candidate promotion in `confirm.rs`; a detector signature now errors with the matching `propose --from` command

`opportunities.rs` emits `propose_command` in place of `confirm_command`. The schema sections of `AGENTS.md`, `src/assets/instructions.md`, `docs/memory/engrams.md`, and `docs/src/pages/` are rewritten to the new flow, along with the `tests/cli.rs` tests that drive the removed paths.

**Scope matching is its own lookup.** It does not extend member matching. `active_for_members` returns early when no note is attached to the advised path (`retrieval.rs:723-725`), which is exactly the new-file case. A new `active_for_scope(conn, paths)` matches `scope.paths` with the exact-or-under semantics of `path_match_counts` (`retrieval.rs:193-229`). `advise` returns the union of both lookups, deduplicated by schema. `path_match_counts` and `tag_match_counts` (prime), `centroid_match` (query), and `match_schemas` (log-time suggestions) treat scope as part of the schema's vocabulary.

**Every appearance explains itself:**

- Each surfaced schema carries `matched_by`, the first reason found in this order: member anchor, scope path, scope tag, member tag.
- `schema show` and `brief schema:<id>` carry an `origin` joined from `schema_proposals`.
- `prime` lists up to three pending proposals, oldest first.
- The advisor extension (`src/assets/engrams-advisor.ts`) changes in four places:
  - `runAdvise` keeps `schemas` alongside `constraints` and `violations`; today it drops them (`:101-105`).
  - The pre-edit message lists each schema first: claim, then instruction, then scope and match reason, ahead of the constraints. It is sent when either list is non-empty, not only when constraints exist (`:195`).
  - When error-severity violations block the edit (`:185-192`), the block reason carries the same schema entries ahead of the violations. The block reason is the one message guaranteed to reach the agent on a blocked edit. We do not rely on a message sent just before the hook blocks, because omp's ordering for that case is not documented. A blocked path stays un-advised (`:190`), so the retry after a fix delivers the normal message.
  - A `write` that creates a new file is advised with the path as written. Today `resolveFilePath` returns nothing for a file that does not exist yet, so the hook returns before calling `advise` (`:69-83`, `:158-159`). Only ambiguous bare file names are still skipped.

  The message builders (`formatSteer`, `formatBlockReason`) and the mapping from `advise` output become named exports that take plain data. That lets tests check them without omp or the engrams binary (see Verification strategy).

Rules 002, 004, 005, and 006 apply: dynamic `IN` lists borrow parameters, member enrichment consumes its lookup maps, evidence counting folds iterators, and the trigger phrases, match reasons, and status vocabulary stay `&'static str`.

**Test book.** `tests/fixtures/manuscript-book.json` is a hermetic fixture modeled on `tests/fixtures/schema-kind-dogfood.json`. It holds 18 chapter paths and about 30 notes in planted storylines: the protagonist's wound, a romance, magic rules, siege logistics, style rules, and a betrayal. It also holds decoys, tags, anchors, and backdated timestamps. Tests load it through direct `rusqlite` inserts (`tests/cli.rs:2294-2321`).

## Data model

Migration `MIGRATION_V16` (ALTER and CREATE only) with identical shapes in the baseline `SCHEMA`; `LATEST_VERSION = 16`.

```sql
ALTER TABLE schemas ADD COLUMN statement TEXT;  -- human-level claim; NULL for schemas confirmed before v16
ALTER TABLE schemas ADD COLUMN scope_json TEXT NOT NULL DEFAULT '{"tags":[],"paths":[]}';

CREATE TABLE IF NOT EXISTS schema_proposals (
  id            INTEGER PRIMARY KEY,
  uuid          TEXT NOT NULL UNIQUE,
  name          TEXT NOT NULL,                  -- proposed schema name; confirm settles the final name (API surface 2)
  statement     TEXT NOT NULL,                  -- the claim
  summary       TEXT NOT NULL,                  -- instruction; opens with a TRIGGER_PREFIXES phrase
  scope_json    TEXT NOT NULL,                  -- {"tags":[...],"paths":[...]}, sorted and deduplicated
  members_json  TEXT NOT NULL,                  -- sorted "kind:id" knowledge keys: the evidence snapshot
  contrary_json TEXT NOT NULL DEFAULT '[]',     -- sorted "kind:id" counter-evidence, informational
  evidence_json TEXT NOT NULL,                  -- {"notes":n,"files":n,"days":n,"contrary":n}, computed
  source_kind   TEXT NOT NULL,                  -- 'manual' | 'candidate'
  source_ref    TEXT,                           -- detector signature at propose time; provenance text, never a key
  session       TEXT,                           -- --session or ENGRAMS_SESSION when known
  status        TEXT NOT NULL DEFAULT 'proposed', -- 'proposed' | 'confirmed' | 'rejected'
  decision_note TEXT,                           -- rejection reason
  schema_id     INTEGER,                        -- set on confirm; origin joins on it
  created_at    TEXT NOT NULL,
  decided_at    TEXT
);
CREATE INDEX IF NOT EXISTS ix_schema_proposals_status ON schema_proposals(status);
CREATE INDEX IF NOT EXISTS ix_schema_proposals_schema ON schema_proposals(schema_id);
```

The baseline comment on `schemas.status` becomes `'active' | 'needs_review' | 'retired'`. `matched_by` and `origin` are computed at read time and add no columns.

Rust types in `src/ops/schemas/propose.rs`:

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct Scope { pub tags: Vec<String>, pub paths: Vec<String> }   // sorted, deduplicated

#[derive(Serialize)]
pub struct Evidence { pub notes: usize, pub files: usize, pub days: usize, pub contrary: usize }

pub enum ProposalStatus { Proposed, Confirmed, Rejected }               // as_str() -> &'static str
pub enum MatchVia { MemberAnchor, ScopePath, ScopeTag, MemberTag }      // as_str() -> &'static str

pub const MIN_EVIDENCE: usize = 3;
pub const RESTATEMENT_GATE: f64 = 0.8;
pub const PENDING_IN_PRIME: usize = 3;
```

## API surface

1. **`engrams schema propose`**
   - Flags:
     - `--statement <text>` (required)
     - `--summary <text>`, or `--trigger <condition> --rule <directive>` (required; same rules as `refine`)
     - `--members <kind:id,...>` (required unless `--from` is given; replaces the seeded members when both are given)
     - `--from <candidate-signature-prefix>`
     - `--name <slug>` (default: the existing `suggest_schema_name` over the members)
     - `--scope-tags <a,b>`, `--scope-paths <p,q>`
     - `--contrary <kind:id,...>`
     - `--session <key>` (default: `ENGRAMS_SESSION`)
     - `--force`
   - Checks, in order:
     1. Keys in `--members` and `--contrary` are knowledge notes: decision, system_pattern, progress_entry, or custom_data. `code` and `schema` keys are refused. `--from` drops the group's code members and reports them.
     2. Every key in both lists exists and is current: decisions need `status = 'active' AND archived = 0`, and patterns need `archived = 0`. No key may appear in both lists. `evidence.contrary` counts only contrary keys that pass these checks.
     3. There are at least `MIN_EVIDENCE` notes.
     4. The notes span at least two distinct anchor files or two distinct UTC days.
     5. The claim's token overlap with each note is below `RESTATEMENT_GATE`.
     6. The instruction passes `refine_text`.
     7. A pending proposal with member overlap of at least `JACCARD_IDENTITY` (0.7) returns `similar`.
     8. A rejected proposal at the same overlap, with no cited note newer than its `decided_at`, returns `rejected_before`.
     9. Active-schema territory is checked with `claimed_territory` (`gates.rs:78`).

     `--force` bypasses checks 7 and 8 only, mirroring `decision log`'s similarity gate (`decision.rs:70-78`).
   - Success: `{"status": "success", "inserted": true, "proposal": {"id", "name", "statement", "summary", "scope", "members", "contrary", "evidence", "source": {"kind", "ref"}, "status": "proposed", "created_at"}, "confirm_command": "engrams schema confirm proposal:<id>", "reject_command": "engrams schema reject proposal:<id> --reason \"<why>\""}`
   - Name clash: when the name is already used by a schema or a pending proposal, the success output also carries `"name_conflict": {"with": "schema" | "proposal", "id": 2}`. It is a warning, not an error, so the agent can choose another name before asking the person.
   - Overlap gates (success JSON, nothing written): `{"inserted": false, "similar": [{"id", "overlap"}]}` or `{"inserted": false, "rejected_before": [{"id", "decided_at", "reason", "overlap"}]}`
   - Errors: `proposal needs at least 3 notes (got 2)`; `cited notes must span 2 files or 2 days`; `decision:99 does not exist`; `decision:9 is not current`; `decision:12 is cited as both evidence and contrary`; `code:14 is a file, not a note: cite notes and put files in --scope-paths`; `statement repeats decision:7 (overlap 0.86 >= 0.8)`; the existing instruction-prefix error; `notes already covered by schema 3 (mara-arc)`.
2. **`engrams schema confirm <target>`**
   - Edit flags (proposal targets only): `--name`, `--statement`, `--summary` or `--trigger --rule`, `--scope-tags`, `--scope-paths`.
   - `proposal:<id>` returns `{"status": "success", "schema": {"id", "name", "statement", "summary", "summary_source": "agent", "scope", "kind": "schema", "members", "origin"}, "proposal": {"id", "status": "confirmed"}}`.
   - A name never blocks an approval. When the final name is taken at confirm time, confirm applies the existing `unique_name` rule (`confirm.rs:274-297`), which yields `mara-2`, `mara-3`, and so on, and the output gains `"renamed_from": "mara"`.
   - A schema ID or name keeps the existing re-confirmation output. Edit flags with a schema target are an error.
   - A detector signature is an error: `detector groups are evidence, not schemas: run engrams schema propose --from <prefix> --statement ... --summary ...`.
   - A proposal that is not pending is an error: `proposal <id> is already confirmed`.
3. **`engrams schema reject proposal:<id> [--reason <text>]`** returns `{"status": "success", "proposal": {"id", "status": "rejected", "decision_note", "decided_at"}}`. A non-pending proposal is an error.
4. **`engrams schema proposals [--status proposed|confirmed|rejected|all]`** (default `proposed`) lists proposals with enriched members, capped at 20 per proposal with the same `truncated`/`total_members` contract as `schema show`.
5. **`engrams schema refine <schema>`** gains `--statement`, `--scope-tags`, and `--scope-paths`. At least one of these, or `--summary`, or `--trigger` with `--rule`, is required. The instruction rules are unchanged.
6. **`engrams schema scan`.** `--apply` is removed and the output is `{"candidates": [...]}`.
7. **`engrams session close`.** `schema_opportunities[].confirm_command` is replaced by `propose_command`: `engrams schema propose --from <sig-prefix> --name <suggested> --statement "<claim>" --summary "When <condition>: <directive>"`.
8. **Schema payloads.**
   - The `prime` schema block, `advise` `schemas`, `query` schema tier, `schema list`/`show`, and `brief schema:<id>` gain `statement` (nullable) and `scope`.
   - Surfaced entries in `prime`, `advise`, and `query` gain `"matched_by": {"via": "member_anchor" | "scope_path" | "scope_tag" | "member_tag", "value": "manuscript"}`.
   - `schema show` and `brief schema:<id>` gain `"origin": {"proposal_id": 12, "source": {"kind": "candidate", "ref": "decision:4,decision:7"}, "proposed_at": "...", "confirmed_at": "...", "session": "..."}`. It is `null` for schemas confirmed before this release.
9. **`engrams prime`** gains `"pending_proposals": [{"id", "name", "statement", "evidence", "created_at", "confirm_command", "reject_command"}]`, oldest first, at most `PENDING_IN_PRIME`, and omitted when none are pending.
10. **Advisor extension message** (`src/assets/engrams-advisor.ts`, written by `engrams install --harness omp`). `runAdvise` returns `{schemas, constraints, violations}`. The message is sent before the first edit of a file, including the edit that creates it, whenever schemas or constraints apply. When error-severity violations block the edit, the block reason starts with the same schema entries and then lists the violations. Each schema shows its claim first, then its instruction, then its scope and match reason, all before the file's rules:
    ```text
    engrams schemas for manuscript/ch19.md:
      [schema] mara-broken-leader
        Mara believes she is broken; that hurt carries with her as she grows into a leader
        When writing Mara under pressure: she doubts herself aloud and credits others, even as her actions grow stronger
        scope: paths manuscript; tags mara (matched: scope path manuscript)
    engrams constraints for manuscript/ch19.md:
      ...
    ```
11. **Export and import.**
    - Proposals export to `<path>/schema_proposals/{id}.md` in the existing frontmatter-plus-JSON format.
    - Schema JSON gains `statement` and `scope_json`.
    - Proposal member snapshots are stored text that no graph or scan code dereferences, so import writes them as-is; confirm re-checks them.

## Dependencies

Internal:
- Spec 0006 lands first. Pre-edit advice and drift checks then use the named workspace instead of the current folder, which keeps the test-book tests independent of the folder `cargo test` runs from.
- Spec 0002 formation pipeline (`schema_candidates`, `claimed_territory`, `centroid()`, `member_of` write path)
- Spec 0005 retrieval surfaces (`prime_block`, `active_for_members`, `centroid_match`, `refine_text`)
- `src/ops/schemas/opportunities.rs` `suggest_schema_name`
- `src/ops/install.rs:175`, which writes the advisor extension from `include_str!`. Existing omp users re-run `engrams install --harness omp` to receive the updated extension.

External:
- No new Rust crates. Uses the existing `rusqlite`, `serde`, `serde_json`, `clap`, and `uuid`.
- Bun `^1.3`, for development only (omp already requires it), to run `bun test src/assets/`. It is a stated prerequisite for the Gate 3 and Gate 5 commands of any phase that changes the extension; nothing else in the build needs it. No package manifest is added: the extension's only package import is the type-only `ExtensionAPI` import, which Bun erases.

## Verification strategy

Per-phase verification commands (Gate 3):

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test proposal
cargo test --test cli schema
cargo test --test memory schema
bun test src/assets/   # phases that change src/assets/engrams-advisor.ts
```

Full-suite verification (Gate 5):

```bash
cargo test
cargo build
bun test src/assets/
npm --prefix docs install && npm --prefix docs run build
```

Manual:
1. In a temporary workspace loaded from `tests/fixtures/manuscript-book.json`, use the built binary to propose the protagonist claim citing the eight wound notes with scope tags `mara` and paths `manuscript`. Run `engrams prime` and confirm the proposal is listed as pending. Confirm it, then run `engrams advise manuscript/ch19.md` and confirm the schema is listed with its claim and `"matched_by": {"via": "scope_path", "value": "manuscript"}`.
2. Propose the same notes again and confirm it is refused, naming the confirmed schema. Reject a second proposal, propose its notes again, and confirm the rejection is cited.
3. In a copy of that workspace, run `engrams install --harness omp`. In an omp session, ask the agent to create `manuscript/ch19.md`, a file that does not exist yet. Confirm the pre-edit message names the protagonist schema with its claim first, then its instruction, scope, and match reason.
4. Export this project's database and import it into a fresh temporary workspace. Refine `performance-rules` with a claim and `--scope-paths src/ops`, then confirm `engrams advise src/ops/new_module.rs` lists it.
5. Confirm `engrams schema scan --apply` is rejected as an unknown flag, and that `schema confirm <detector-signature>` prints the propose command.

The Bun tests (`src/assets/engrams-advisor.test.ts`) run in two layers:

- **Pure helpers**, with plain data and no binary:
  - the `advise`-output mapping keeps `schemas`
  - `formatSteer` puts each schema's claim, instruction, scope, and reason before any constraint
  - `formatBlockReason` puts the same schema entries before the violations
- **The `tool_call` handler**, with a stand-in for omp's extension API that records `sendMessage` calls and the handler's return value. It runs against a temporary workspace with the built debug binary linked at `<tmp>/target/debug/engrams`, where the extension already looks first (`engrams-advisor.ts:31`). It covers two cases:
  - A `write` of a new `manuscript/ch19.md` produces the schema message.
  - An `edit` of an existing chapter whose content violates an error-severity rule is blocked, and the block reason shows the schema first.

The phase that changes the extension lists these tests in its acceptance. The live check inside omp stays manual (step 3 above).
