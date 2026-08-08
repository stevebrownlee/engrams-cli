# Policy Engine (Path A) — Implementation Specification

**Status:** Ratified (decision #44, 2026-08-05) · Not yet implemented
**Decision context:** #44 (Path A ratified) · #42 (install --harness omp packaging) · #43 (selective rule reading) · #30 (zero-alloc hardening intent)

---

## 1. Summary

Engrams becomes a policy engine for LLM-agent guidance. Patterns stored in `system_patterns` gain machine-checkable expressions (`regex` | `ast-grep`) and a severity (`info` | `warn` | `error`). From that single source of truth, engrams:

1. **Exports omp TTSR rule files** (`.omp/rules/*.md`) so omp enforces structural patterns mid-stream while an agent is writing an edit — the enforcement engine is omp-native, engrams never runs its own in-session matcher.
2. **Runs the same checks itself** via `engrams check` for CI and session-end review, where omp is not present.

Severity has no omp-native field; it is **export-time routing**: `error` → hard mid-stream interrupt, `warn` → advisory injection, `info` → rulebook (on-demand). Operating discipline is **warn-by-default** — hard interrupts are reserved for `error` to protect trust in the system.

Path B (briefs, negative knowledge, observability) is not abandoned: it becomes the delivery tier for everything that cannot be structurally checked, and is explicitly out of scope here.

---

## 2. What's in scope

| # | Item | Notes |
|---|---|---|
| S1 | Schema migration to v5: `system_patterns` gains `check_kind TEXT` (`regex`\|`ast`, NULL), `check_expr TEXT` (NULL), `severity TEXT NOT NULL DEFAULT 'warn'` | Additive, transactional via existing `run_migrations` |
| S2 | `pattern log` flags `--check-kind`, `--check`, `--severity`; write-time validation (regex compiles / ast pattern parses; invalid → JSON error, exit 1) | Validation must happen before insert |
| S3 | `pattern list` / `pattern show` surface the new fields | All readers of the 6-column pattern row updated |
| S4 | `engrams rules export --harness omp [--out DIR]` | One `.omp/rules/engrams-<slug>.md` per checkable pattern; anchors → `scope` globs; severity → interrupt routing; deterministic byte-identical re-runs; patterns without checks skipped (they are prose-tier) |
| S5 | Export manifest `.omp/rules/.engrams-manifest.json` (pattern ids, timestamps, content hash) | Basis for staleness detection |
| S6 | `engrams doctor` staleness check: manifest vs DB | Warns when generated files lag pattern edits |
| S7 | Write-through: `pattern log` / `pattern delete` re-export automatically **only when a manifest is already present** | Presence of manifest = opt-in; no surprise file writes otherwise |
| S8 | `engrams check [--staged \| --paths ...]` — regex checks; JSON violations `{pattern, file, line, severity, message}`; exit 0 clean / 1 violations | Regex-only in first iteration |
| S9 | ast-grep check support in `engrams check` | Dependency decision gated (see R1) |
| S10 | `engrams install --harness omp` — writes rule files + manifest into workspace `.omp/rules/`, prints JSON summary | Fulfils #42 |
| S11 | Dogfood: rules 002–005 registered as DB patterns with checks, exported into this repo, verified live in an omp session | The proof that TTSR firing works end to end |
| S12 | Docs: `docs/memory/engrams.md` command reference; `AGENTS.md` operating-loop table | Both list new commands |

## 3. What's out of scope

- **`tool_call` brief hook** (prose-tier delivery: decision rationale, negative knowledge, judgment standards). Separate follow-up spec; no `.omp/hooks/*.ts` shipped in this one.
- **Ideas 3–6 from the design discussion:** negative-knowledge vocabulary, guidance observability/telemetry, rules-as-DB-entities collapse of AGENTS.md, symbol-level anchors.
- **Non-omp harnesses** (Claude Code, Cursor). `--harness` accepts exactly one value (`omp`); no abstraction layer yet.
- **Auto-fix / code transformation.** Detection and surfacing only.
- **Check kinds beyond `regex` and `ast`.** No shell checks, no multi-file invariants in v1.
- **Severity semantics inside omp.** omp has no severity field; routing is decided entirely at export time.
- **Merging `.agents/rules/` files with DB patterns.** Files stay hand-maintained reading material (per #43); DB patterns are the enforcement copy. Convergence is Idea 5, deferred.

## 4. Expected files that will change

| File | Change |
|---|---|
| `src/schema.rs` | New `MIGRATION_V5` const (three `ALTER TABLE system_patterns ADD COLUMN …`); base `SCHEMA` updated for fresh DBs |
| `src/db.rs` | `LATEST_VERSION` 4 → 5; `run_migrations` match arm for v5 |
| `src/models.rs` | `Pattern` struct: `check_kind: Option<String>`, `check_expr: Option<String>`, `severity: String` |
| `src/ops/pattern.rs` | INSERT/upsert carries new fields; `parse_pattern_row` reads them; log-time validation of check expressions; write-through hook after mutation |
| `src/cli.rs` | Subcommands: `rules export`, `check`, `install`; `pattern log` gains `--check-kind/--check/--severity` |
| `src/ops/rules/mod.rs`, `src/ops/rules/export.rs` | **New.** TTSR rule generation, slug naming, anchor→scope glob mapping, manifest read/write |
| `src/ops/check.rs` | **New.** Check runner: file collection (`--staged` via existing git ops / `--paths`), regex engine, JSON violations, exit codes |
| `src/ops/install.rs` | **New.** Harness install: invokes export, writes manifest, prints guidance |
| `src/ops/doctor.rs` | Staleness check comparing manifest against `system_patterns` timestamps |
| `src/ops/prime.rs`, `src/ops/activity.rs`, `src/ops/anchor.rs`, `src/ops/report.rs` | Pattern SELECT lists widened (or confirmed untouched where projection is intentional) |
| `src/ops/transfer/export.rs`, `src/ops/transfer/import.rs` | New fields carried through `engrams export` / `import` round-trip |
| `Cargo.toml` | `regex` (S8); `ignore` (gitignore-aware file walk for S8); ast-grep crates gated behind S9 decision |
| `tests/cli.rs` | Integration tests per §7 |
| `docs/memory/engrams.md` | Command reference for `rules export`, `check`, `install`; new pattern fields |
| `AGENTS.md` | Operating-loop table rows for new commands |

**Generated in target workspaces (not committed to engrams-cli):** `.omp/rules/engrams-*.md`, `.omp/rules/.engrams-manifest.json`. (This repo already has `.omp/commands/`; `.omp/rules/` does not exist yet.)

## 5. Known risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | **ast-grep dependency weight.** Tree-sitter grammars are heavy: build time and binary size grow for a feature used only in `check`. | S9 is gated: measure build-time/binary delta of the ast-grep crates vs shelling out to the `sg` binary before committing; `regex`-only S8 ships first so S9 can slip without blocking the pipeline. Feature-gate if adopted. |
| R2 | **False-positive mid-stream interruptions erode trust.** A noisy rule trains users to delete the whole rules directory. | Warn-by-default; only `error` severity maps to hard interrupt; every exported check must be validated against the real codebase during dogfood (S11) before any consumer sees it. |
| R3 | **Stale generated files.** Pattern edited, rule file not regenerated → enforcement drifts from truth. | Manifest (S5) + doctor staleness check (S6) + write-through on mutation (S7). Deterministic output keeps git diffs clean so drift is visible in PRs. |
| R4 | **Migration damage to real databases.** | Additive columns with defaults only; existing `run_migrations` is transactional; BDD scenario migrates a v4 fixture DB and verifies row integrity. |
| R5 | **omp frontmatter semantics drift.** This design already survived two doc corrections (astCondition syntax, interruptMode meaning); docs may lag runtime. | S11 verifies generated rules against **live omp behavior**, not just the docs; interrupt routing table is isolated in one export function so corrections are one-site edits. |
| R6 | **Garbage check expressions logged by users.** | Write-time validation (S2): regex must compile, ast pattern must parse; rejection is a structured JSON error. |
| R7 | **Dual source of truth** between `.agents/rules/*.md` and DB patterns until Idea 5 lands. | Convention: DB is the enforcement source, files are reading material; nothing in this spec merges them. |
| R8 | **`engrams check` performance** on large repos. | Scope to `--staged`/`--paths` by default; full-tree scan only on explicit flag; gitignore-aware walker (`ignore` crate). |
| R9 | **Verdict divergence** between omp's TTSR engine and engrams' own runner (different regex/ast engines disagree on edge cases). | Parity fixtures (S9 gate): same violating file must produce the same verdict from both paths; divergence is a bug in the engrams runner, not tolerated as "close enough." |

## 6. Phases

Ordering rationale: foundation first (nothing else exists without schema), core value second (in-session enforcement, zero new dependencies), CI third, the expensive dependency last, packaging/docs throughout the final phase. Each phase is independently shippable and leaves the tool fully working.

### Phase 1 — Schema & pattern model
- `MIGRATION_V5`, `LATEST_VERSION` bump, `Pattern` struct fields, `parse_pattern_row` update.
- `pattern log` flags + write-time validation; list/show output.
- Readers widened: prime, activity, anchor, report, transfer export/import.
- **Gate:** migration test on a v4 fixture; round-trip test (log with check → export → import → fields intact).

### Phase 2 — TTSR export & dogfood
- `src/ops/rules/` module: slug, frontmatter generation, anchor→scope mapping, severity→interrupt routing, manifest write.
- `engrams rules export --harness omp`; doctor staleness check; write-through.
- Dogfood: register 002–005 as patterns; export into this repo's `.omp/rules/`; live-verify in an omp session that a seeded violation surfaces mid-edit.
- **Gate:** live TTSR verification (not docs); byte-identical re-export test.

### Phase 3 — `engrams check` (regex)
- `src/ops/check.rs`: file collection (`--staged` reuses existing git diff plumbing), `regex` + `ignore` deps, JSON violations, exit codes.
- **Gate:** BDD fixtures (violating file → exit 1 + structured violation; clean file → exit 0); demo over a staged diff in this repo.

### Phase 4 — `engrams check` (ast-grep)
- Dependency decision recorded (crate vs `sg` shell-out) with build-time measurement; implement; parity fixtures vs TTSR.
- **Gate:** parity test suite green; dependency decision logged in engrams.

### Phase 5 — Install & docs
- `engrams install --harness omp` (export + manifest + JSON guidance output).
- `docs/memory/engrams.md`, `AGENTS.md` updates; `engrams export`; decision #44 anchor confirmed.
- **Gate:** fresh-workspace install test (init DB → install → rule files present and valid).

## 7. Acceptance criteria

### S1 — Migration

```gherkin
Given a workspace database at schema version 4 with 2 existing patterns
When `engrams migrate` runs
Then PRAGMA user_version is 5
And system_patterns has columns check_kind, check_expr, severity
And the 2 pre-existing patterns are intact with severity 'warn' and NULL checks
```

```gherkin
Given a fresh workspace with no database
When any engrams command initializes the database
Then PRAGMA user_version is 5 and the new columns exist
```

### S2 — Log-time validation

```gherkin
Given I run `engrams pattern log --name "No Box params" --check-kind regex --check "Vec<Box<dyn" --severity warn`
When the regex fails to compile
Then the command exits 1 with a JSON error naming the invalid check
And no pattern row is inserted
```

```gherkin
Given a valid regex check expression
When `engrams pattern log` succeeds
Then `pattern show <id>` displays check_kind, check_expr, and severity
```

### S3 — Field surfacing in list output

```gherkin
Given a pattern logged with --check-kind regex --check "Vec<Box<dyn" --severity error
When I run `engrams pattern list`
Then the JSON entry for that pattern includes check_kind "regex", check_expr, and severity "error"
And entries logged before the v5 migration show severity "warn" and null check fields
```

### S4/S5 — Export

```gherkin
Given a pattern "No Boxed SQL Params" with check_kind regex, check_expr "Vec<Box<dyn rusqlite::ToSql>>", severity warn, anchored to src/ops
When I run `engrams rules export --harness omp`
Then .omp/rules/engrams-no-boxed-sql-params.md exists
And its frontmatter condition is the pattern's regex verbatim
And its scope covers tool:edit(src/ops/**) and tool:write(src/ops/**)
And the body contains the pattern description
And .omp/rules/.engrams-manifest.json records the pattern id and timestamp
```

```gherkin
Given an unchanged database
When I run `engrams rules export --harness omp` twice
Then the second run produces byte-identical files
```

```gherkin
Given a pattern with no check (prose-only)
When I run `engrams rules export --harness omp`
Then no rule file is generated for it
```

### S6/S7 — Staleness & write-through

```gherkin
Given rules have been exported and the manifest is present
When I log a new checkable pattern
Then the rule files are regenerated without an explicit export command
And `engrams doctor` reports no staleness
```

```gherkin
Given exported rules whose manifest predates a direct database edit
When I run `engrams doctor`
Then it reports the rules directory as stale and names the drifted patterns
```

### S8 — Check runner (regex)

```gherkin
Given a pattern with regex check "Vec<Box<dyn rusqlite::ToSql>>" scoped to src/ops
And a file src/ops/query.rs containing `Vec<Box<dyn rusqlite::ToSql>>`
When I run `engrams check --paths src/ops/query.rs`
Then the exit code is 1
And the JSON output contains a violation with the pattern name, file path, line number, and severity
```

```gherkin
Given the same pattern and a clean file src/ops/anchor.rs
When I run `engrams check --paths src/ops/anchor.rs`
Then the exit code is 0 and the violations array is empty
```

```gherkin
Given a staged diff touching src/ops/query.rs
When I run `engrams check --staged`
Then only staged files are checked
```

### S9 — ast-grep checks & engine parity

```gherkin
Given a pattern with check_kind ast, check_expr "if let Some($V) = $MAP.get(&$K) { $D.$F = $V.clone(); }", anchored to src/ops
And a file src/ops/decision.rs containing a matching get-then-clone enrichment block
When I run `engrams check --paths src/ops/decision.rs`
Then the exit code is 1
And the violation names the pattern, file, and line of the matching construct
```

```gherkin
Given the same ast-check pattern exported to .omp/rules/ and present in the engrams check corpus
When both omp TTSR and `engrams check` evaluate the same violating fixture file
Then both produce a violation verdict for the same construct
And both produce no verdict on the corresponding clean fixture
```

### S10 — Install

```gherkin
Given a workspace with an engrams database and no .omp/rules directory
When I run `engrams install --harness omp`
Then .omp/rules/ contains one file per checkable pattern and the manifest
And the JSON output lists every written path
```

### S11 — Dogfood (live verification)

```gherkin
Given rules 002–005 registered as patterns and exported in the engrams-cli repo
When an agent editing src/ops/query.rs in an omp session writes a boxed ToSql parameter
Then omp surfaces the corresponding rule before the edit completes
And a prose-only pattern produces no interruption
```
