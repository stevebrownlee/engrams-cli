# Review report: 0004 — Schema Kind Surfacing

**Verdict:** pass with findings
**Reviewed at:** 2026-09-15T13:12:31Z
**Reviewer:** spec-reviewer v1

## Summary

The spec is well-formed: all nine required sections appear in the required order, the six acceptance criteria are sequential, Given/When/Then-shaped, and testable, and the Architecture explicitly follows the frozen-migration invariant from `AGENTS.md`. Two non-blocking findings: the Summary's confirm-time-override wording contradicts AC-1 and Out of scope, and the API surface's changed-output-contract list omits export/import. Three open questions are present, phrased as questions and none blocking.

## Findings

| ID  | Severity | Category              | Location                 | Message                                                                                                                                      |
| --- | -------- | --------------------- | ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| F-1 | moderate | summary-oos-conflict  | Summary, paragraph 2     | Summary's "unless the person confirming chooses otherwise" contradicts AC-1 and Out of scope: the machine label is copied as-is, no override. |
| F-2 | minor    | api-incomplete        | API surface, bullet list | Export/import carry the new columns per Architecture and AC-6, but are absent from the changed-output-contract list.                          |

## Recommended next steps

Proceed to Gate 2. Findings are surfaced for the developer's awareness but do not block the pipeline.

Note: per `protocols/spec-format.md`, `spec-reviewer` flags the Open questions section to the orchestrator. All three items are genuine questions and none is a blocking decision, but the orchestrator may pause the pipeline on open questions at its discretion.
