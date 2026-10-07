# Design: Schema Adoption Surfacing at Session Close

**Date:** 2026-10-04  
**Status:** Draft (Awaiting user review)  
**Topic:** Cognitive schema adoption via session-end opportunity suggestions, benefit visibility, and guided counts  

---

## 1. Problem Statement

Cognitive schemas in `engrams` provide high-level context before low-level retrieval. However, adoption suffers from three critical frictions:

1. **Discovery Friction:** Schema candidates are clustered and staged quietly by `schema scan`, but unless a developer actively runs `scan`, they never know candidates exist.
2. **Authoring Friction:** Drafting a schema name, trigger condition, and rule directive from scratch requires cognitive overhead.
3. **Payoff Invisibility:** While `retrieval_surfaces` already records whenever a schema guides retrieval, no human-facing tool exposes what schemas have saved the developer.

This design targets the primary human-facing ritual—`engrams session close`—to surface both actionable creation opportunities (the ask) and concrete guidance metrics (the pitch) in one unified interaction.

---

## 2. Goals & Non-Goals

### Goals
- Surface gate-passing, unconfirmed schema candidates at `session close` with prefilled, executable `schema confirm` commands.
- Display observed schema guidance metrics for the current session in `session close` using existing `retrieval_surfaces` telemetry.
- Expose a `guided` usage counter on `engrams schema list` items to provide visibility into long-term schema utility.
- Deduplicate gate-passing logic and territory-overlap checks between `scan.rs` and `confirm.rs`.
- Ensure dormant candidates and previously notified candidates do not nag repeatedly unless new evidence (member drift) arrives.

### Non-Goals
- Fully autonomous schema creation or promotion without human confirmation.
- Developing a separate interactive TUI or wizard.
- Adding complex dismissal or snooze state tables (handled minimally via `notified_at` timestamp).
- Modifying `engrams doctor` or agent-facing `prime` outputs in this phase.
- Modifying the underlying Louvain clustering or kind-labeling algorithms.

---

## 3. Architecture & Component Decomposition

```
+-----------------------------------------------------------------------+
|                         engrams session close                         |
+-----------------------------------------------------------------------+
        |                                                   |
        v                                                   v
+-------------------------------+           +-------------------------------+
|  ops::schemas::opportunities  |           |      ops::session (ROI)       |
+-------------------------------+           +-------------------------------+
| - Recomputes gates_pass()     |           | - Queries retrieval_surfaces  |
| - Filters claimed_territory() |           |   for session window          |
| - Checks notified_at drift    |           | - Computes guided_retrievals  |
| - Proposes kebab-case name    |           |   and distinct_schemas        |
| - Formats confirm_command     |           +-------------------------------+
| - Stamps notified_at          |
+-------------------------------+
        |
        v
+---------------------------------------------------------------+
|                         SQLite Tables                         |
|  - schema_candidates (ALTER: +notified_at TEXT)               |
|  - schemas                                                    |
|  - retrieval_surfaces                                         |
|  - context_links                                              |
|  - usage_log                                                  |
+---------------------------------------------------------------+
```

### 3.1. Shared Predicates & Refactoring
To prevent divergence, two existing duplicated checks are extracted into shared `pub(crate)` helpers:

1. **`gates_pass(density: f64, stability: i64, reward: i64) -> bool`**  
   - Extracted from `src/ops/schemas/scan.rs` and `src/ops/schemas/confirm.rs`.
   - Compares candidate metrics against launch gate constants (`DENSITY_GATE`, `STABILITY_GATE`, `REWARD_GATE`).
   - Shared location: `src/ops/schemas/gates.rs` or `src/ops/schemas/mod.rs`.

2. **`claimed_territory(conn: &Connection, knowledge_members: &[String]) -> Result<Option<(i64, String)>>`**  
   - Extracted from `src/ops/schemas/confirm.rs:437-478`.
   - Filters out `schema:` nodes to compare pure knowledge members.
   - Evaluates:
     - Candidate subset of existing schema (`covered == knowledge_members.len()`)
     - Existing schema subset of candidate (`covers == s_knowledge.len()`)
     - Jaccard similarity $\ge 0.70$ (`JACCARD_IDENTITY`).
   - Returns the claiming `(schema_id, schema_name)` if claimed, or `None`.
   - `confirm()` calls this and bails if claimed; `opportunities` calls this and silently filters out dormant/claimed candidates.

### 3.2. Opportunity Detection (`src/ops/schemas/opportunities.rs`)
Called during `session close`. It queries `schema_candidates` and filters rows where:
1. `kind = 'schema'` (rejecting `story`, `inventory`, `unclear`).
2. `gates_pass(density, stability_count, reward_hits)` is `true`.
3. `claimed_territory(conn, &knowledge_members)?` is `None`.
4. Re-notification check: `notified_at IS NULL OR notified_at < last_seen_at`.

For each qualifying candidate:
- **Name Suggestion:** Evaluates dominant member tag and common anchor directory path. Formats into a kebab-case string. Checks uniqueness against `schemas.name`; appends numeric suffix (`-2`, `-3`) if collision occurs.
- **Confirm Command:** Emits `engrams schema confirm <cluster_sig_prefix> --name <suggested_name>`.
- **Notification Stamp:** Updates `schema_candidates.notified_at = datetime('now')` within the session close database transaction.

### 3.3. Session Value Counter (`src/ops/session.rs`)
`session close` calculates session-scoped value metrics:
- Identifies session start timestamp from `MIN(usage_log.timestamp) WHERE session = ?1`.
- Queries `retrieval_surfaces`:
  ```sql
  SELECT COUNT(*), COUNT(DISTINCT node_id)
  FROM retrieval_surfaces
  WHERE node_kind = 'schema' AND ts >= ?1
  ```
- Emits `"schema_value": { "guided_retrievals": N, "distinct_schemas": M }`.

### 3.4. `schema list` Guided Counter (`src/ops/schemas/list.rs`)
- Factors the surfacing aggregation query out of `src/ops/schemas/retrieval.rs:80`:
  ```sql
  SELECT node_id, COUNT(*)
  FROM retrieval_surfaces
  WHERE node_kind = 'schema'
  GROUP BY node_id
  ```
  into `pub(crate) fn surface_counts(conn: &Connection) -> Result<HashMap<i64, usize>>`.
- Both `prime_block` and `list` consume this helper.
- `schema list` JSON payload includes `"guided": count` on every schema object.

---

## 4. Data Model & Schema Migration

Following the **frozen migrations** invariant:
- Increment `LATEST_VERSION` to `15` in `src/schema.rs` and `src/db.rs`.
- Define `MIGRATION_V15`:
  ```rust
  pub const MIGRATION_V15: &str =
      "ALTER TABLE schema_candidates ADD COLUMN notified_at TEXT;";
  ```
- Update baseline `SCHEMA` definition to include `notified_at TEXT` in the `schema_candidates` table definition.

---

## 5. Output Specifications

### `engrams session close` (Enhanced Output)
```json
{
  "status": "success",
  "session": "ses_01J9...",
  "reads_skipped": 12,
  "reads_required": 4,
  "tokens_saved": 8500,
  "schema_value": {
    "guided_retrievals": 6,
    "distinct_schemas": 2
  },
  "schema_opportunities": [
    {
      "signature": "decision:10,decision:12,pattern:4",
      "member_count": 3,
      "stability_count": 3,
      "reward_hits": 2,
      "suggested_name": "ast-traversal-rules",
      "confirm_command": "engrams schema confirm decision:10,dec --name ast-traversal-rules"
    }
  ]
}
```

### `engrams schema list` (Enhanced Output)
```json
{
  "status": "success",
  "schemas": [
    {
      "id": 1,
      "name": "performance-rules",
      "summary": "Fires whenever I edit Rust code...",
      "summary_source": "agent",
      "status": "active",
      "kind": "unclear",
      "member_count": 5,
      "guided": 14,
      "updated_at": "2026-09-18T05:05:49Z"
    }
  ]
}
```

---

## 6. Error Handling & Edge Cases

- **Zero Opportunities:** When no candidates qualify, `schema_opportunities` is an empty array `[]` (no noisy alerts).
- **Zero Surfacings:** When schemas were not queried in the session, `guided_retrievals` and `distinct_schemas` are `0`.
- **Candidate Member Drift:** If new members join a previously notified candidate, `last_seen_at` advances beyond `notified_at`, allowing it to resurface naturally.
- **Name Collisions:** If the suggested name already exists in `schemas.name`, appends an incremental counter (e.g., `performance-rules-2`).
- **Transaction Safety:** Stamping `notified_at` on candidates executes within the same SQLite transaction as the session close record insertion.

---

## 7. Testing Strategy

1. **Unit Tests (`src/ops/schemas/opportunities.rs`):**
   - Candidate filtering: validates exclusion of non-schema kinds, failing gates, claimed territory (subsets, supersets, high Jaccard), and already-notified candidates.
   - Name suggestion generator: tests tag dominance, directory prefix extraction, and collision suffixing.

2. **Integration Tests (`tests/cli.rs`):**
   - End-to-end Opportunity Workflow:
     1. Seed candidate records passing gates.
     2. Run `engrams session close`.
     3. Verify `schema_opportunities` contains the expected candidate and command.
     4. Execute the emitted `confirm_command` verbatim.
     5. Verify schema confirms successfully.
     6. Run `session close` again; verify `schema_opportunities` is empty (no nagging).
   - Session Value Counter:
     - Seed `usage_log` and `retrieval_surfaces` records inside and outside the session window; assert exact counts.
   - `schema list` Guided Count:
     - Seed `retrieval_surfaces` for schema 1; verify list output reflects the correct count.
