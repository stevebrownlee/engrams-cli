---
identifier: active_context
title: Active Context
created: 2026-08-08T15:44:28Z
---
# Active Context

```json
{
  "content": {
    "current_task": "v0.9.0 release: docs revamp complete, Formula/engrams.rb sha256 + git commit remaining",
    "focus": "Guidance architecture: Path A ratified (decision 44), spec at specs/policy-engine.md. Phase 1 next: schema v5 (check_kind/check_expr/severity on system_patterns). Pending: implement phases 1-5; engrams export + git commit of rules/AGENTS.md/spec (awaiting user review).",
    "key_context": "Docs v0.9.0 shipped: three-pillar IA, 3D viz (three.js/3d-force-graph/GSAP), real graph data. 22 pages build green. Decisions #46-49.",
    "last_updated": "2026-08-05",
    "next_steps": [
      "Update Formula/engrams.rb sha256 after binaries built",
      "Git commit docs revamp",
      "Run engrams export + push"
    ],
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
    "status": "Spec complete, implementation not started"
  },
  "name": "default",
  "updated_at": "2026-08-08T15:44:28Z",
  "version": 33
}
```
