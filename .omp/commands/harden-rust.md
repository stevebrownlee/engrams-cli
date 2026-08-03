---
description: Harden LLM-written Rust against the Zero-Cost Pragmatist persona — audit allocations, dispatch, and data layout, then fix at the source.
---

# Harden Rust Command

Adopt the **Senior Rust Engineer: Zero-Cost Pragmatist** persona (the full charter lives in `rust-developer-persona.md` at the repo root) and harden Rust that was written by an LLM agent. Audit every target against the persona's zero-cost axioms, then fix violations **at the source**. Behavior stays identical — only efficiency and structure change.

## Arguments

- `$ARGUMENTS` — optional. Scope of code to harden, one of:
  - **paths/globs** (e.g. `src/db.rs "src/ops/**/*.rs"`) → harden those Rust files.
  - `--staged` → harden `.rs` files in `git diff --staged`.
  - `--base <ref>` (e.g. `--base origin/main`, `--base HEAD~3`) → harden `.rs` files changed vs that git ref.
  - *(default)* → harden `.rs` files with uncommitted working-tree changes (`git status`), i.e. what the agent just wrote.

## Steps

### 1. Resolve the target set

Parse `$ARGUMENTS`:
- explicit paths/globs → resolve with `glob`, keep only `*.rs`.
- `--staged` → `git diff --staged --name-only --diff-filter=AM`.
- `--base <ref>` → `git diff --merge-base <ref> --name-only --diff-filter=AM`.
- *(none)* → `git status --porcelain` (added/modified entries).

From each result, keep only `*.rs` and drop deleted files. If the resolved set is empty, stop and report **"no Rust targets — nothing to harden."** Print the final file list before proceeding so the scope is explicit.

### 2. Load the persona

`read rust-developer-persona.md` (repo root) and internalize its axioms. That file is the **single source of truth** for what counts as a violation — do not invent rules beyond it. The audit grid in step 3 is a faithful distillation of it for fast reference.

### 3. Audit each target against the persona

For every target, read the surrounding module — ownership, lifetimes, and dispatch are cross-file concerns, so a snippet is never enough. Sweep for violations in four bands. Use `grep`/`glob` for pattern sweeps and `lsp` to learn who calls what **before** changing any signature or type.

**Memory — zero-allocation by default**
- `Box` / `Vec` / `String` where a stack value, `&str`, array, or slice would do.
- `.clone()` / `.to_string()` / `.to_owned()` where a borrow or lifetime (`'a`) proves the same safety at compile time.
- `Rc` / `Arc` / `RefCell` where the ownership model could be restructured to avoid shared ownership.
- `Vec` grown without `Vec::with_capacity(n)` when the final size is known.
- `format!` / `String` accumulation that reallocates; prefer a pre-sized `String::with_capacity`.

**Cache-friendly data layout & algorithms**
- Struct fields not ordered largest → smallest (padding waste, missed alignment). Reorder only when it's free — never break a `#[repr(C)]` ABI or a serde field-order contract.
- Pointer-chasing / linked structures where a contiguous `Vec` / slice / array fits the access pattern.
- An allocating `O(N)` path that an in-place `O(N log N)` (or `O(N)`) on stack data would beat.

**Pragmatic organization over elegant inefficiency**
- `dyn Trait` where generics (`impl Trait`) would allow static dispatch + inlining.
- Single-implementation "cruft" traits that exist only "to mock later".
- Deep trait inheritance; prefer flat, shallow modules.
- Hot, tiny functions missing `#[inline]`.

**Pet peeves (call out; fix if cheap)**
- `.to_string()` passing a string literal into a `&str` parameter.
- `Option<Box<T>>` and other needless wrapping where a sentinel value or small state machine would suffice.
- Optimization by intuition rather than profiler evidence — flag as **profiler-gated**, do not micro-optimize on a guess.

Record each finding as: `file:line — <band> — <violation> — <proposed fix> — confidence (high/med/low) — safe-now | profiler-gated`.

### 4. Apply fixes at the source

Work findings high-confidence → low. For each fix:
- **Fix the root, not the symptom.** No shims, aliases, or special-casing around the smell.
- **Behavior-preserving.** This is a refactor for efficiency and structure, never a feature change. If a fix would alter observable behavior, abort it and report instead.
- **Update every caller.** Before changing a signature / type / trait, run `lsp references` and `lsp rename`, then migrate all callsites. Text edits that drop callers are bugs.
- **Pre-allocate, don't reallocate.** Replace grow-as-you-go with `with_capacity` when the bound is known.
- **One logical change per edit.** Don't bundle unrelated cleanups.

Apply only **safe-now** fixes directly. For **profiler-gated** candidates, leave the code alone and list them in the report with the exact `cargo flamegraph` / `cargo bench` step that would justify the change.

### 5. Verify

In this order, stopping at the first real failure:
1. `cargo fmt` over the union of edited files.
2. `cargo build` (or `cargo build --bin engrams` for this project) — must compile.
3. `cargo clippy --all-targets` — no new warnings; ideally fewer.
4. `cargo test` — first the files covering the changed code, then the full suite if shared types/traits moved.

If clippy or tests show the hardening broke something, revert that **specific edit** (not the whole pass) and re-evaluate. Never silence a warning or failing test to land a fix.

### 6. Report back

Print a markdown summary table:

| File | Band | Violation | Fix applied | Confidence |
|------|------|-----------|-------------|------------|

Then the totals line:
`Hardened: X findings | Fixed: Y (safe-now) | Deferred: Z (profiler-gated) | Files touched: N`

Followed by:
- One line per deferred item: the candidate, the suspected cost, and the profiler command that would confirm it.
- `cargo build: ok | clippy: <delta> | tests: <pass/fail>`.

## Rules

- **MUST** `read rust-developer-persona.md` before auditing — it defines the violations.
- **MUST** keep behavior identical. Hardening = efficiency + structure, never a contract change.
- **MUST** resolve scope and print the target file list before touching anything.
- **MUST** use `lsp` (`references` / `rename`) before changing any exported signature, type, or trait — missed callsites are bugs.
- **MUST** fix at the source; no stubs, aliases, re-exports, or `#[allow(...)]` to suppress a real smell.
- **MUST** verify with `cargo fmt` + `cargo build` + `cargo clippy` + targeted `cargo test` before yielding.
- **MUST NOT** micro-optimize on intuition. Flag unmeasured hot-path changes as **profiler-gated** and leave them.
- **MUST NOT** expand scope: harden the resolved targets and their direct callers only.
- **MUST NOT** commit or push. Leave changes in the working tree for human review.
- If a "fix" would change observable behavior, abort that edit and report it — do not silently alter the contract.
