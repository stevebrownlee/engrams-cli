# Engrams Docs v0.9.0 Revamp — "The Active Advisor"

**Date:** 2026-08-08 · **Status:** Approved (design) · **Scope:** Full docs-site restructure + visual overhaul

## Narrative thesis
Engrams is not a database you query — it is an **advisor that compounds**. On day 1 the
knowledge graph is nearly empty and engrams can only *fetch*. As the agent logs decisions,
patterns, and links, the graph densifies and `prime` / `relevant` / `check` / `doctor` shift
from *retrieval* to *advice*: recalling why a choice was made, enforcing standards before a
violation lands, catching contradictions. The site tells this maturation story first, then
serves the human, then the agent.

## Approved decisions
- **3D:** both a decorative three.js particle hero AND an interactive `3d-force-graph` of **real data**.
- **Animation:** GSAP + ScrollTrigger for scroll-driven storytelling.
- **IA:** three-pillar restructure. No existing page preserved verbatim.
- **Data:** actual engrams DB (this repo dogfoods itself), focused on **real feature work**
  (policy engine, ontology, graph, schema) — not meta docs/system-prompt decisions.

## Real data (baked to `docs/public/graph-data.json`)
Knowledge-entity subgraph (decision / system_pattern / progress_entry), manual links only,
**filtered to feature/product work** — 12 meta decisions (tags `docs`/`agents-md`/`instructions`)
and 25 meta progress entries (README/docs-site/homepage/AGENTS.md/rules work) excluded:
- **71 nodes** (33 decisions, 6 patterns, 32 progress) · **29 reasoning links**
- Relationship types: depends_on, extends, implements, part_of, refines, relates_to, supersedes, supports, uses
- Span: 2026-07-10 → 2026-08-05 (~26 days); links carry timestamps → true chronological maturation
- Temporal buckets (real): Day 1 = 23n/1l · Week 1 = 43n/11l · Week 2 = 45n/11l · Month = 71n/29l
- Node positions: deterministic Fruchterman-Reingold baked into JSON (`viewBox 1000×620`) for smooth scroll-scrub
- Full graph (for copy): 802 edges total — 45 manual + 757 derived (629 git co-change), 67 code nodes

## Information architecture (three pillars)

```
Landing (index)  ── full rewrite
├─ Pillar 1 · The Advisor      /docs/advisor/{overview, maturity, loop}        (new concept)
├─ Pillar 2 · For You          /docs/for-you/{overview, briefing, enforcement, exploration}
└─ Pillar 3 · For Your Agent   /docs/for-agent/{overview, storing, connecting, retrieving, maintaining}
Get Started                    /docs/{getting-started, installation, quick-start}  (refresh)
Reference                      /docs/reference/{cli, releases, security-model, ai-tool-setup}  (keep; releases += 0.9.0)
Contributing                   /docs/contributing
```

- **Pillar 1 (primary focus):** what an active advisor is; how the knowledge graph matures over
  time; the advisor loop (store → link → surface → act). Interactive 3D graph is the centerpiece.
- **Pillar 2 (secondary):** the commands a *human* runs and their benefits — prime / report /
  doctor (briefing), check / install / rules (enforcement), graph / decision search / export (exploration).
- **Pillar 3 (lowest emphasis, still complete):** the *agent* storage/connection/retrieval/
  maintenance commands, reorganized from the 14 existing feature pages.

## Landing page structure
1. **Hero** — dark canvas, three.js particle network (brand colors). Headline: "Your AI assistant
   forgets. Engrams remembers — and advises."
2. **Maturation story** — GSAP ScrollTrigger pins the viewport; scrolling through Day 1 → Week 1 →
   Month 1 densifies the graph (real timestamps), each stage labeled with what engrams can now advise on.
3. **What your advisor surfaces** — cards: recalled decisions, enforced patterns, caught contradictions
   (examples drawn from real feature decisions: policy engine #44/#45, ontology #21–28).
4. **For you** — human commands with one-line benefits.
5. **Interactive 3D graph** — 3d-force-graph over `graph-data.json`; orbit/zoom, color by entity type,
   size by degree, click-to-inspect.
6. Ecosystem + install CTA.

## Visual & motion
- Keep pastel brand (Sky Aqua #56CBF9, Maya Blue #7FBEEB, Powder Blue #AFBED1, Pastel Petal #EAC5D8,
  Lavender #DBD8F0); add a **dark variant** for hero/3D/graph sections so the graph pops.
- Micro-motion: CSS reveal-on-scroll cards, animated stat counters (nodes/edges/decisions), hover lifts.

## Tech & deps
`three` (hero + maturation) · `3d-force-graph` (interactive graph) · `gsap` + ScrollTrigger ·
Astro stays vanilla (no React). Graph data is a static JSON snapshot baked at build time.

## Contracts (cross-slice)
- `graph-data.json` shape: `{ meta, nodes:[{id,group,label,ts,tags?,status?,severity?,degree}], links:[{source,target,type,ts}] }`.
  Node `id` = `"<type>:<id>"` (e.g. `decision:44`). `group` ∈ decision | system_pattern | progress_entry.
- Group → color: decision = Sky Aqua, system_pattern = Pastel Petal, progress_entry = Maya Blue.
- Components: `Hero3D.astro`, `ForceGraph.astro` (props: dataUrl, height), `ScrollStory.astro` (stages).
- Nav sections in `Sidebar.astro`: The Advisor / For You / For Your Agent / Get Started / Reference / Contributing.

## Build & verify
`npm run build` (astro) must stay green; drive key pages with **agent-browser** (repo rule — no IDE
browser tooling): confirm hero renders, scroll story advances, 3D graph loads real nodes/edges, nav works.
