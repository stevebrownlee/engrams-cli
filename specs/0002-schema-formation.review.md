# Review report: 0002 — Schema Formation

**Verdict:** pass with findings
**Reviewed at:** 2026-09-05T01:52:00Z
**Reviewer:** spec-reviewer v1

## Summary

The spec is well-formed against all nine format sections and internally consistent: every AC traces to Architecture machinery, the data model is actual DDL, and the API surface carries input/output/error contracts. Two non-blocking gaps were found: unbounded growth in the staging table, and an unspecified `status` lifecycle for newly applied schemas.

## Findings

| ID  | Severity | Category             | Location                  | Message                                                                                                                                   |
| --- | -------- | -------------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| F-1 | moderate | data-unbounded-growth | Architecture, step 3; Data model, `schema_candidates` | Staging persists across scans for stability counting but no eviction is specified; dissolved clusters leave candidate rows forever while AC-10 bounds only telemetry storage. |
| F-2 | minor    | data-semantic-ambiguity | Data model, `schemas.status`; API surface, `schema scan --apply` | `status` defaults to `active` and its vocabulary includes `proposed`, but no statement of which status `--apply` creates schemas with or what transitions to `active`. |

## Recommended next steps

Proceed to Gate 2. Findings are surfaced for the developer's awareness but do not block the pipeline.
