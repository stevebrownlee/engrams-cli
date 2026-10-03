# 0005 — Schema-Guided Retrieval

## Summary

Software projects and research accumulate hundreds of separate notes, rules, and decisions. When an artificial intelligence agent works on a project, it often reads files blindly or searches through disconnected facts. This causes high token costs and inconsistent choices. Human brains do not operate by searching raw facts. Instead, human brains use schemas: high-level mental models that organize lower-level memories and tell the person when and how to act.

This feature adds cognitive schema guidance to the memory assistant across four capabilities:

1. Enriched schema inspection: When a user or agent inspects a schema, the tool returns the high-level concept together with complete descriptions, rules, and anchor locations for all linked facts in one action.
2. Standardized schema authoring: The system provides templates and prompts that guide users to author schema summaries with explicit activation triggers and operational rules.
3. Context-driven activation: When an agent receives a prompt or query, the tool matches active schemas and automatically surfaces their rules before any file inspection begins.
4. Pedagogical documentation: The documentation site includes a dedicated guide on cognitive schemas. It teaches users how human mental models work, how schemas guide agent reasoning, and how to structure schemas for software, writing, and planning projects.

As a result, agents receive high-level guidance before they access low-level project details. Users can build simulated mental models that maintain consistent behavior across sessions and reduce unnecessary file reads.

## Acceptance criteria

### AC-1: Schema detail includes enriched member details

**Given** a confirmed schema with linked members of any supported kind  
**When** the user requests the schema detail view  
**Then** the output includes the schema metadata  
**And** each member entry contains its type-specific details, including summaries, descriptions, and file anchors  
**And** schemas with more than 20 members return the first 20 members with a truncation flag  

### AC-2: Schema brief expands member details in one call

**Given** an existing confirmed schema  
**When** the user runs the composite brief command for that schema  
**Then** the response includes the schema summary and kind  
**And** the response includes enriched details for all immediate member nodes  

### AC-3: Schema refinement accepts structured trigger and rule flags

**Given** an existing confirmed schema  
**When** the user runs `schema refine` with `--trigger <CONDITION>` and `--rule <DIRECTIVE>`  
**Then** the command formats the summary with the explicit trigger and operational rule  
**And** the schema records `summary_source` as `agent`  

### AC-4: Schema validation rejects summaries without trigger conditions

**Given** a user or agent submits a manual `--summary` string to `schema refine`  
**When** the text does not define an explicit activation trigger condition  
**Then** the command returns an error response with guidance on the required format  
**And** the schema summary remains unchanged  

### AC-5: Advise command surfaces matching schema rules for edited paths

**Given** one or more target file paths  
**When** the user runs `engrams advise <paths>`  
**Then** the output identifies active schemas whose member nodes anchor to those paths  
**And** the output surfaces the trigger and operational rule of each matching schema above low-level constraints  

### AC-6: Prime command prioritizes schemas relevant to target context

**Given** active confirmed schemas in the database  
**When** the user runs `engrams prime` with `--paths` or `--tags`  
**Then** the schemas block ranks schemas that match those paths or tags above unanchored schemas  
**And** each schema entry includes its operational rule summary  

### AC-7: Documentation explains cognitive schemas and agent guidance

**Given** the built documentation website  
**When** a user navigates to the cognitive schemas guide  
**Then** the page explains how human cognitive schemas organize lower-level memories  
**And** the page explains how to design schemas that guide artificial intelligence agents  
**And** the page provides concrete examples for software architecture, research, and planning  

## Out of scope

- Neural vector embeddings or external vector database engines for schema similarity matching
- Interactive terminal question wizards that break scripted or automated agent invocation
- Automated schema splitting or merging (deferred to future adaptation specifications)
- Direct mutation of member entities (decisions, patterns, code nodes) through schema commands
- Changes to underlying clustering algorithms (`schema scan` Louvain clustering remains unchanged)

## Open questions

- What exact heuristic should determine whether a schema matches an untagged prompt query in `prime`?
- Should the cap limit of 20 enriched members in `schema show` be configurable via a command-line flag?

## Architecture

This feature enhances schema inspection, authoring, and retrieval across four existing Rust modules and adds one documentation guide:

1. **Enriched Schema Reads (`src/ops/schemas/list.rs` and `src/ops/schemas/retrieval.rs`):**
   `schema show` and `brief schema:<id>` currently return bare string references (for example, `decision:36`). We enrich each member node based on its entity kind:
   - For `decision`: `id`, `summary`, `status`, `contract`, and `anchors`.
   - For `system_pattern`: `id`, `name`, `description`, `severity`, and `anchors`.
   - For `code_node`: `id`, `path`, `symbol`, `kind`, and `line_count`.
   - For `progress_entry`: `id`, `description`, `status`, and `timestamp`.
   - For `custom_data`: `id`, `schema`, and `value_preview`.
   
   To avoid token exhaustion on large inventory schemas, member enrichment is capped at 20 members per schema. If a schema contains more than 20 members, the output includes the first 20 enriched members, sets `"truncated": true`, and reports `"total_members": N`. We chose inlined field enrichment over separate `brief` follow-up queries because single-call composite reads reduce round-trip tool latencies and prevent agent hallucination of missing details.

2. **Standardized Authoring and Validation (`src/ops/schemas/list.rs` and `src/cli.rs`):**
   We extend `engrams schema refine` with optional `--trigger <CONDITION>` and `--rule <DIRECTIVE>` flags. When provided, the tool synthesizes the summary as `"Fires when <CONDITION>: <DIRECTIVE>"`.
   If a user passes a manual `--summary` string, a validation function inspects the text. The summary must start with an accepted conditional phrase (`"Fires when"`, `"When"`, `"Applies if"`, `"Activates when"`). If no condition is found, the command bails with an error response that explains the required structure. We chose to store the trigger and rule directly within the existing `summary` column rather than adding new database columns. This keeps SQLite storage backward-compatible, ensures full-text search (`schemas_fts`) indexes both triggers and rules, and requires no schema migrations. A `--force` bypass flag is deliberately omitted to enforce operational guidance across all agent-authored schemas.

3. **Context-Driven Activation (`src/ops/advise.rs` and `src/ops/schemas/retrieval.rs`):**
   - In `src/ops/advise.rs`, we resolve active schemas whose member nodes anchor to the inspected paths. A new `"schemas"` key in the JSON output displays matching schemas above low-level constraints.
   - In `src/ops/schemas/retrieval.rs`, `prime_block` accepts optional target paths and tags. Schemas with member anchors or centroid tags matching the target context rank above unanchored schemas.

4. **Pedagogical Documentation (`docs/src/pages/docs/guide/cognitive-schemas.astro`):**
   A dedicated Astro documentation page teaches cognitive schema theory, explains how schemas guide LLM retrieval, and provides concrete examples for software architecture, research, and planning.

## Data model

`_No data model changes._`

The existing `schemas` table and `schemas_fts` virtual table accommodate structured summaries without changes.

## API surface

1. **`engrams schema show <target>`**
   - Input: `<target>` (schema ID or exact name).
   - Output change: The `members` array changes from string keys to an array of enriched objects:
     ```json
     {
       "status": "success",
       "schema": {
         "id": 1,
         "name": "problem-first-messaging",
         "summary": "Fires when writing public documentation: use problem-first risk messaging.",
         "members": [
           {
             "kind": "decision",
             "id": 36,
             "summary": "Redesign homepage from docs-style to marketing landing page",
             "anchors": ["docs/src/pages/index.astro"]
           }
         ],
         "member_count": 9,
         "truncated": false
       }
     }
     ```
   - If `member_count > 20`, `members` contains the first 20 entries and `"truncated": true`.

2. **`engrams schema refine <target> [--summary <text>] [--trigger <cond>] [--rule <dir>]`**
   - Flags: `--summary <text>`, or `--trigger <cond>` paired with `--rule <dir>`.
   - Error: Returns error JSON `{"error": "summary must begin with a conditional trigger phrase (e.g., 'Fires when...', 'When...')"}` if validation fails.

3. **`engrams advise <paths> [--staged]`**
   - Output change: Adds a top-level `"schemas"` array:
     ```json
     {
       "schemas": [
         {
           "id": 1,
           "name": "problem-first-messaging",
           "summary": "Fires when writing public documentation: use problem-first risk messaging.",
           "kind": "schema"
         }
       ],
       "constraints": [],
       "violations": []
     }
     ```

4. **`engrams prime [--paths <p>] [--tags <t>]`**
   - Output change: The `schemas` array prioritizes schemas matching the provided paths or tags.

## Dependencies

- **Internal:**
  - Spec 0002 (schema formation, member_of links)
  - Spec 0004 (schema kind surfacing)
  - `src/ops/anchor.rs` (path resolution)
  - `src/ops/drift.rs` (staleness checks)
  - `src/ops/schemas/list.rs` (schema operations)
  - `src/ops/schemas/retrieval.rs` (retrieval surfaces)
- **External:** None. Uses existing dependencies (`rusqlite`, `serde_json`, `clap`).

## Verification strategy

### Gate 3 (per-phase verification)

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test --bin engrams schemas
cargo test --test cli schema
```

### Gate 5 (full-suite verification)

```bash
cargo test
cargo build
```

**Manual:**
1. Run `engrams schema refine <target> --summary "Invalid summary without condition"` and verify that the command fails with a validation error.
2. Run `engrams schema refine <target> --trigger "editing documentation" --rule "apply marketing framing"` and verify the formatted summary.
3. Run `engrams schema show <target>` and verify enriched member structures.
4. Run `engrams advise docs/src/pages/index.astro` and verify that matching schemas appear under the `"schemas"` key.
5. Inspect the documentation site at `docs/dist/docs/guide/cognitive-schemas/index.html` to confirm the educational guide is present and renders properly.
