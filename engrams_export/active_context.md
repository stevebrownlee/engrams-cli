---
identifier: active_context
title: Active Context
created: 2026-09-05T10:07:48Z
---
# Active Context

```json
{
  "name": "default",
  "content": {
    "current_focus": "schema formation v0.13.0 shipped via PR #3; v0.14 adaptation (drift hook, merge/split, needs_review surfacing) is next frontier",
    "current_task": "v0.10.0 complete: scoring, prune-decay, observability shipped. v0.11.0 advise+hooks shipped. Both verified, 85 tests pass.",
    "current_work": "v0.13.0 schema formation - spec 0002 complete, no open questions, ready for /spec-implement",
    "decisions_this_session": [
      72,
      73,
      74,
      75,
      76,
      77,
      78,
      79
    ],
    "focus": "All outstanding items implemented: batch decision nested-transaction bug fixed, tests/cli.rs clippy warnings fixed, DB hygiene cleared, Tier-3 memory quality suite (tests/memory_quality.rs) shipped. 110 tests pass across 5 suites.",
    "key_context": "Docs v0.9.0 shipped: three-pillar IA, 3D viz (three.js/3d-force-graph/GSAP), real graph data. 22 pages build green. Decisions #46-49.",
    "last_updated": "2026-08-05",
    "next": [
      "monitor adoption and feedback"
    ],
    "next_steps": [
      "run pipeline on specs/0002-schema-formation.md",
      "phase-1 dogfood gate owns sweeping the five launch constants"
    ],
    "notes": {
      "decisions_latest": "#50 hero title, #51 ScrollStory removal",
      "landing": "ScrollStory maturation scroll removed (not useful). Hero title: An advisor with a memory that compounds. Interactive 3D ForceGraph is the sole real-data viz. gsap dep dropped."
    },
    "open_items": [],
    "policy_engine": {
      "commands": [
        "pattern log --check-kind/--check/--severity",
        "rules export --harness omp",
        "install --harness omp",
        "check --staged/--paths"
      ],
      "dogfood_patterns": "7-10",
      "schema_version": 5,
      "spec": "specs/policy-engine.md",
      "status": "implemented",
      "tests": "50 green"
    },
    "recent_work": "Released v0.9.0 — Policy Engine + Docs Revamp. Tag pushed, CI built 4 targets, GitHub release live with full notes, Formula bumped with verified sha256.",
    "release_notes_style": "beginner-friendly 3-pillar breakdown (Consolidation & Decay, Contradiction Gate, Causal Hindsight)",
    "session": "v0.12.0-feedback",
    "status": "Spec complete, implementation not started",
    "v0.10.0": {
      "status": "Done",
      "summary": "Retrieval scoring, prune-decay, read-observability implemented. 85 tests pass."
    },
    "recent_decisions": [
      72,
      73,
      74,
      75,
      76,
      77,
      78,
      79,
      80,
      81,
      82
    ]
  },
  "version": 45,
  "updated_at": "2026-09-05T10:07:48Z"
}
```
