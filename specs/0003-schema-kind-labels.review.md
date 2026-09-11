# Review report: 0003 — Schema Kind Labels

**Verdict:** pass with findings
**Reviewed at:** 2026-09-11T16:18:21Z
**Reviewer:** spec-reviewer v1

## Summary

Well-formed spec: all nine required sections present and in order, eight sequential testable G/W/T criteria, architecture names concrete hook points (a new `src/ops/schemas/kind.rs` running inside `schema scan` after the gates), the V13 migration follows the additive-column pattern of V5/V10–V12, and every claimed dependency surface was verified to exist (`schema_candidates`, `retrieval_surfaces` in V12; `check_kind`/`check_expr` in V5; the apply `skipped` list in `scan.rs`). Findings are two moderate inaccuracies/assumption risks and two minor notes; none are severe.

## Findings

| ID | Severity | Category | Location | Message |
| --- | -------- | ---------------------------- | ---------------------------- | ------- |
| F-1 | moderate | dependency-map-inaccuracy | Dependencies, internal bullet 1 | Cites `src/ops/schemas/detect.rs` for the staging upsert; no `detect.rs` exists anywhere under `src/` — the staging INSERT lives in `src/ops/schemas/scan.rs` (line 202), so `kind.rs` wiring targets `scan.rs`. |
| F-2 | moderate | unstated-assumption | AC-8 / Architecture design choice 4 | The hand-labeled evaluation set exists nowhere: no fixtures directory, and engrams has zero hits for the dogfood judgments ("inventory", "campaign"); in autonomous mode the implementer must author the answer key from the spec's narrative, which weakens the independent-human-judgment property AC-8 exists to test — the developer must review the key at or after the replay phase. |
| F-3 | minor | unresolved-constants | Open questions 1–3 | Silence gap, file-node share threshold, and trigger-confirmation strictness are tuning constants the implementer must pin; spec 0002 precedent applies (phase-1 dogfood gate swept the five launch constants, then recorded them as decision #78). |
| F-4 | minor | verification-command-scope | Verification strategy, Gate 3 | `cargo test schemas` filters by test name, not by the `tests/schemas.rs` filename; new BDD tests must carry `schemas` in their test or module names or the Gate 3 command exercises nothing from the new suite. |

## Recommended next steps

Proceed to Gate 2.
