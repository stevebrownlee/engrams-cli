# Why Your AI Coding Assistant Keeps Making the Same Mistakes (and How engrams 0.10.0 Fixes It)

---

## The Problem

Every time you start a new conversation with an AI coding assistant — Claude, GPT, Copilot, any of them — it starts with **zero memory** of your project. It doesn't know you decided to use PostgreSQL. It doesn't know you banned a certain library. It doesn't know your team's SQL parameter convention. It doesn't know that three months ago you chose to build the state machine a particular way.

So it does what any amnesiac contractor would do: it re-invents every wheel, re-questions every settled decision, and sometimes actively works against decisions you already made.

You've felt this. You re-explain your architecture for the fifth time this week. The AI suggests an approach you explicitly rejected last month. It writes code that violates a convention your team agreed on — a convention that's buried in a Slack thread or a PR comment that the AI will never read.

This isn't the AI being dumb. It's a **memory problem**. And memory problems compound.

### The cost is invisible until you measure it

| Symptom | What it looks like | What it costs |
|---|---|---|
| **Re-explanation tax** | You spend the first 10 minutes of every session explaining your project | ~12,000 tokens of context wasted on orientation that should take one command |
| **Convention drift** | The AI uses `Box<dyn ToSql>` when your team standardized on `&dyn ToSql` | Violations that survive into production, caught in review if you're lucky |
| **Decision amnesia** | The AI suggests reverting a migration that a prior session decided against | You re-litigate settled decisions, or worse, accidentally undo them |
| **Context rot** | Your project memory (if you have one) becomes a graveyard of stale decisions | The signal-to-noise ratio drops until the memory is worse than useless |

---

## The Solution: engrams 0.10.0

engrams is a **local-first memory layer for AI coding assistants**. It's a SQLite database that lives in your project, capturing decisions, patterns, progress, and the relationships between them. Your AI reads it, writes to it, and — new in 0.10.0 — gets **actively steered** by it.

This release adds three capabilities that transform engrams from a **passive database** (stuff is stored, you have to ask for it) into an **active advisor** (the system surfaces what matters, forgets what doesn't, and enforces conventions automatically).

### 1. Retrieval Scoring — the right memories surface first

**Before:** Every decision ranked equally. A two-year-old note about font sizing sat beside the architectural decision that governs your entire data model. The AI waded through noise.

**After:** Every decision and pattern now carries a **blended score** that combines:
- **Recency** — how long ago it was recorded, with Ebbinghaus-style exponential decay (60-day half-life)
- **Importance** — a 0–10 weight you set when logging (default 5), so critical decisions outrank trivia
- **Relevance** — for full-text searches, BM25 match quality from FTS5

```
score = W_recency × e^(-λ × age_days) + W_importance × (importance / 10) [+ W_query × BM25]
```

The `prime`, `relevant`, and `query` commands all rank by this score now. The AI sees your most important, most recent, most relevant context first — automatically.

Set importance with `--importance`:
```bash
engrams decision log --summary "Use the migrate command, never manual SQL" --importance 9
```

### 2. Reinforce-on-Read + Prune-Decay — memory that self-improves

**Before:** Old decisions lived forever, indistinguishable from current ones.

**After:** Two mechanisms keep memory healthy:

**Reinforce-on-Read:** Every time a decision is surfaced by `prime`, `relevant`, or `query`, its `access_count` increments. Frequently-consulted decisions get a higher "survival strength" — they resist decay.

**Prune-Decay:** The new `engrams prune` command archives decisions whose retention has decayed below a threshold:

```
retention = e^(-age_days / strength)
strength  = (importance + access_count) × 30 days
```

An important, frequently-read decision survives for years. A trivial, never-consulted one fades into the archive after a few months. Archived items are excluded from retrieval by default but never deleted.

```bash
engrams prune --dry-run     # preview what would be archived
engrams prune               # archive decayed records
engrams prune --threshold 0.05  # more aggressive
```

The memory **self-cleans**. You don't have to maintain it.

### 3. Active Enforcement — constraints delivered before the edit lands

This is the headline feature. engrams 0.10.0 doesn't just store conventions — it **enforces them at edit time and commit time**.

**`engrams advise`** — a purpose-built command that returns only actionable constraints for the files you're about to edit:

```bash
engrams advise src/ops/scoring.rs
# Returns: checkable patterns + anchored decisions + current violations
```

**Git pre-commit hook** — `engrams install --harness omp --hooks` installs a hook that runs `engrams check --staged` before every commit. Violations at `error` severity block the commit. The AI can't ship code that breaks your registered patterns.

**omp extension** — for users of the [omp](https://engrams.sh) coding harness, the same `--hooks` flag installs an extension that **intercepts edit/write tool calls before they execute**. On the first edit to each file per session:
- It runs `engrams advise` automatically
- Delivers constraints as a non-blocking steer message the AI sees
- Blocks the edit if there are error-severity violations, with a reason the AI reads and adapts to

The AI doesn't have to choose to check constraints. The system enforces them mechanically.

---

## What this means for you

| Capability | Without engrams | With engrams 0.10.0 |
|---|---|---|
| Session startup | Re-explain your project every time | One command rehydrates the full context your agent needs |
| Retrieval ranking | Everything ranks equally — the AI wades through noise | Important, recent, frequently-consulted decisions surface first |
| Memory hygiene | Old decisions pollute context forever | Memory self-cleans: unused records fade into the archive |
| Convention enforcement | You hope the AI reads your conventions. It usually doesn't | The system surfaces constraints automatically when the AI edits a relevant file |
| Violation prevention | Violations land in your codebase — caught in review if you're lucky | Violations are blocked at edit time and commit time — they never land |

---

## Getting started

```bash
# If upgrading from a previous version:
engrams migrate

# Install harness integration + git hook + omp extension:
engrams install --harness omp --hooks

# Log a high-importance decision:
engrams decision log --summary "Never use Box<dyn ToSql> for SQL params" --importance 9

# Register an enforceable pattern:
engrams pattern log --name "Borrowed SQL Params Only" \
  --check-kind regex \
  --check 'Box<dyn rusqlite::ToSql>' \
  --severity error \
  --anchor src/ops

# Start every session with:
engrams prime
```

---

## Upgrade path

Schema v6 is fully additive — four new columns on existing tables, all with safe defaults. No breaking changes, no data migration. Run `engrams migrate` and you're done. Existing decisions get `importance=5`, `access_count=0`, `archived=0`.

---

## Design notes

- **Zero new dependencies** — scoring runs entirely in SQLite via a registered `exp()` function. No embeddings, no vector database, no external services.
- **Local-first** — everything runs against a SQLite file in your project. No data leaves your machine.
- **Boring technology** — FTS5 BM25 + Ebbinghaus decay + simple weighted scoring. The math is from a 1885 memory study and a full-text search engine that ships with SQLite.

---

engrams turns your AI coding assistant from an amnesiac contractor into an engineer with institutional knowledge.

The memory you build today compounds. Every decision logged is one fewer re-explanation tomorrow. Every pattern registered is one fewer convention violation. The system gets better the longer you use it.
