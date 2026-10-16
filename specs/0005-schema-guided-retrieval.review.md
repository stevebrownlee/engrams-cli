# Review report: 0005 — Schema-Guided Retrieval

**Verdict:** pass with findings
**Reviewed at:** 2026-10-03
**Reviewer:** spec-reviewer v1

## Summary

The spec is format-conformant and unusually well-grounded: every cited file path, command, flag, and schema field was verified against the codebase (`summary_source` 'drafted'|'agent' in `src/schema.rs:183`, `prime_block` in `src/ops/schemas/retrieval.rs:92`, `schemas_fts` triggers in `src/schema.rs:228-244`, existing `schema refine --summary` in `src/cli.rs:427-432`, existing `prime --paths/--tags` in `src/cli.rs:126-138`, bare `"{kind}:{id}"` members in `src/ops/schemas/list.rs:159-167`), and it respects the AGENTS.md invariants (no schema migrations, JSON-only output). The nine findings — six moderate, three minor — are one AC-level contradiction (AC-2 vs the 20-member cap), one wrong entity-kind name (`code_node` vs `code`), an undocumented docs-navigation dependency, underspecified refine flag combinations, missing browser verification for the docs AC, and an architecture section silent on the repo's Rust data-handling rules. No severe findings and no project invariant or loaded rule is violated.

[Rule sources consulted: `.agents/rules/global.md`, `001-memory.md`, `002`–`007` (Rust rules), `pre-commit-checks.md` (scoped to backend/frontend stacks, not applicable to this Rust CLI spec); `.pilot/rules/agent-browser.md`, `ui-verification.md`; no `.gemini/styleguide.md` exists. Elixir/Ecto/migration rule files target `backend/` path scopes absent from this repo.]

## Findings

| ID  | Severity | Category            | Location                          | Message |
| --- | -------- | ------------------- | --------------------------------- | ------- |
| F-1 | minor    | open-questions      | Open questions, item 1            | Asks what heuristic matches "an untagged prompt query in `prime`", but AC-6 and the API surface define only `--paths`/`--tags` matching and `engrams prime` accepts no free-text prompt input (`src/cli.rs:126-138`), so the question presupposes an undefined input surface. |
| F-2 | moderate | api-underspecified  | API surface, item 2 (`schema refine`) | Behavior is undefined when `--trigger` is passed without `--rule` (or vice versa) and when `--summary` is combined with `--trigger`/`--rule` (precedence or conflict error unspecified). |
| F-3 | moderate | verify-no-browser   | Verification strategy             | AC-7 describes rendered page content and navigation, but the strategy contains no `agent-browser` commands and manual step 5 inspects built HTML without rendering it, contrary to `spec-format.md`'s verify-no-browser check. |
| F-4 | moderate | internal-consistency | AC-2 vs Architecture, item 1     | AC-2 requires "enriched details for all immediate member nodes" while Architecture caps member enrichment at 20 with a truncation flag, so AC-2 is untestable-as-stated for schemas with more than 20 members. |
| F-5 | moderate | internal-consistency | Architecture, item 1            | Names the member kind `code_node`, but the codebase's member type string is `code` (`src/ops/schemas/kind.rs:187`, `src/ops/schemas/confirm.rs:64`), so kind-keyed enrichment written for `code_node` would never match. |
| F-6 | moderate | internal-consistency | Architecture, item 4            | Proposes `docs/src/pages/docs/guide/cognitive-schemas.astro`, a `guide/` section absent from the current layout (`reference/`, `for-you/`, `advisor/`, `for-agent/`, `concepts/`), and does not address registering the page in the hardcoded sidebar navigation (`docs/src/components/Sidebar.astro`) that AC-7's "user navigates to" depends on. |
| F-7 | minor    | internal-consistency | Architecture item 1 vs API surface item 1 | Architecture specifies `"total_members": N` on truncation while the API example reports only `member_count`, leaving it undefined whether `member_count` returns the capped length (20) or the true total when truncated. |
| F-8 | minor    | internal-consistency | Architecture, item 1            | The enrichment list enumerates five member kinds but omits the `schema` member kind the resolver supports (`src/ops/schemas/confirm.rs:65`), leaving AC-1's "any supported kind" undefined for nested-schema members. |
| F-9 | moderate | arch-rule-gap       | Architecture, items 1-2          | Is silent on `.agents/rules/004-enrich-by-move.md` (consume temporary lookup maps via `remove()`/`mem::take` in the enrichment fold), `005-no-throwaway-collections.md` (fuse/pre-size the enriched-member collection build), and `006-static-vocabulary-borrowed.md` (the accepted trigger phrases are canonical vocabulary and must stay `&'static str`); AGENTS.md marks rules 002-006 on-demand, so the implementer must discover them independently. |

## Recommended next steps

Proceed to Gate 2. Resolve F-4 and F-5 before `spec-json-builder` runs, since both would otherwise be encoded verbatim into phase work (an unsatisfiable AC-2 for >20-member schemas and an enrichment key that matches no rows).
