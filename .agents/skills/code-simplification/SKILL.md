---
name: code-simplification
description: Reduce and consolidate code without changing observable behavior
---

<!-- managed by PILOT — generated from commands/simplify.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/simplify.md and re-run install -->

# Simplify

I will help you find and eliminate structural redundancy: duplicated logic, parallel implementations, missing abstractions, and code that can be deleted outright.

I will examine all of the commits on the active branch and review code that has been added or modified during work on the current branch.

Functionality or features not related to the purpose of the work on the current branch is out of scope.

## Guardrails
- Never change behavior during a refactor — they never share a commit
- Do not over-abstract; the wrong abstraction is worse than duplication
- Do not introduce new dependencies unless the saving is large and the dep is already consistent with the stack
- Respect public contracts (exported APIs, wire formats, persisted schemas, CLI interfaces)
- Stay in scope; note out-of-scope opportunities at the end of the report instead of acting on them
- Readability beats line count — never compress code into cleverness

## Steps

### 1. Map the Blast Radius
Build a complete picture of the related code surface:
- Identify the seed set: uncommitted changes, branch diff, recently modified files
- Trace upstream (callers), downstream (imports), and siblings (same domain concept)
- Locate all tests covering anything in the above sets
- Inventory existing utilities, base classes, installed dependencies, and language built-ins

### 2. Find Simplification Opportunities
Analyze the scoped surface in priority order:
- **Deletion:** dead code, redundant reimplementations, speculative abstractions with one user
- **Consolidation:** textual, structural, and conceptual duplication (apply Rule of Three)
- **Dependency Inversion:** high-level flows welded to concrete dependencies that could collapse into one flow + N adapters
- **Reduce/Reuse/Recycle:** flatten indirection, promote shared helpers, parameterize rather than fork, normalize to existing patterns

### 3. Report Findings
Produce a ranked report before touching anything. For each finding include:
- **What:** the opportunity, with file:line references for every occurrence
- **Why:** estimated lines removed, maintenance burden avoided, testability gained
- **How:** the concrete refactor, named with standard vocabulary (Extract Function, Invert Dependency, Inline Class, etc.)
- **Risk:** behavior-change risk, test coverage, breadth of callers affected
- **Effort:** S / M / L

Rank by `(impact × confidence) / risk`. Lead with deletions and high-occurrence duplications.

### 4. Execute Safely
For each approved refactor:
- Run the test suite to establish a green baseline first
- Write characterization tests for any code lacking coverage before touching it
- One refactoring per commit; keep tests green between every step
- Migrate callers incrementally; delete the old path only after the last caller moves
- Verify with the project's own tooling: tests, type checker, linter, build

### 5. Summarize
Report quantitative results:
- Lines removed vs. added
- Files deleted
- Duplications collapsed
- Dependencies eliminated

## Principles
- The best code is code that doesn't exist; the second best is code that already exists and is reused
- Abstraction is a tool, not a goal — prefer duplication over the wrong abstraction
- Behavior preservation is absolute
- Match the codebase's idiom; a refactor that fights conventions adds cognitive load
- When in doubt, move in smaller steps
