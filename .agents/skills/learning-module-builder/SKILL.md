---
name: learning-module-builder
description: Build interactive, single-file HTML "zero to hero" learning modules that teach any technical topic in plain language — a roadmap/TOC home page, one-section-per-page navigation with progress stamps, quizzes with instant feedback, animated concept demos, jargon tooltips, glossary flash cards, expert-conversation prep with sound-bite answers, and a final assessment. Use this skill whenever the user asks to be taught something or to deeply understand something — "teach me X", "I'm unfamiliar with X", "help me understand what I'm actually implementing", "make me a learning module / course / interactive tutorial / explainer", "zero to hero on X" — or when they share a PR, diff, repo, or document they want to understand rather than just summarize. Also use it when someone needs onboarding or teaching material for teammates. Trigger even if the user never says "HTML" or "interactive" — this is the house format for teaching deliverables.
---

# Learning Module Builder

You are building a single `.html` file that behaves like a small course. The learner is typically smart but new to the domain, and their real goal is usually **confidence in an upcoming conversation** — with a systems engineer, a reviewer, a stakeholder. Optimize for "can they explain it in their own words afterward," not for topic coverage.

## The five commitments

Everything else in this skill serves these:

1. **Plain language first.** Every technical term gets an immediate translation — a hover tooltip on the term, or an "in plain words" callout box. Untranslated jargon is the exact place learners silently give up; one unexplained acronym can cost the whole section.
2. **Grounded in their reality.** If a PR, repo, or document is provided, every claim about it must come from actually reading it. Quote real code with a "real code from your PR" badge and a plain-words caption. Real snippets convert abstractions into "oh — that's *my* file." Never invent facts about the user's system: an invented "fact" the learner repeats to an expert destroys the exact credibility this module exists to build.
3. **Interaction is retention.** Quizzes after every section (retry until correct, explanation revealed on success), animations that make invisible processes visible (pipelines, timelines), and predict-then-reveal scenario games. A learner who commits to a prediction before seeing the answer remembers the answer.
4. **One page at a time.** A roadmap home page (the TOC), each section on its own page, progress stamps as sections are passed. A long scroll overwhelms; a stamped checkpoint motivates.
5. **A theme chosen by conceptual fit — fresh every time.** The central metaphor is picked by matching the topic's *mechanics* to a familiar real-world system that shares them (deployment traffic-shifting → highway lane management; caching → the pantry vs. the warehouse run), so the metaphor lets the learner predict behavior, not just enjoy the wallpaper. Follow the selection procedure in `references/theming.md` — write the topic's core verbs, score candidate worlds, stress-test with the hardest concept, and name where the metaphor breaks. Do **not** default to a favorite theme or reuse one from a prior module; if a repo/product name gifts a theme, accept it only when it survives the same scoring. And the metaphor never buries the real term: introduce both in the same breath, quiz on the terminology.

## How people actually learn (apply these deliberately)

The learner is a working professional: time-constrained, problem-centered, arriving with uneven prior knowledge, and likely to scan and jump rather than read linearly. Six evidence-based techniques shape how the components get used — they're the difference between a module that informs and one that changes what someone can do:

- **Retrieval over re-reading.** Recall strengthens memory; re-reading doesn't. This is why quizzes follow every section — and why demos should ask for a **prediction before the reveal** ("before you press Run: which span will be longest?"). A learner who commits to a guess remembers the answer.
- **Interleaving confusable pairs.** Every domain has terms learners mix up (staleTime/gcTime, CSP/CORS, logs/traces, authn/authz). Find the module's confusable pairs, teach them **side by side** (a contrast table or twin cards), and quiz on *telling them apart* — discrimination practice beats teaching each in isolation.
- **Variation.** One example teaches the example; two or three varied examples teach the principle. For each core concept, vary the surface (different endpoint, different failure, different data) so the learner abstracts the pattern.
- **Elaboration.** Don't just show what something does — say *why it was designed that way*, *when to reach for it*, and *what the alternative was*. This is what the conversation-prep Q&As and code-peek "why" captions are for.
- **Reflection.** After a dense section or demo, a two-line "what we just did and why it worked" recap consolidates before the quiz. End the module with one open prompt: "how would you apply this to your own project?"
- **Spacing.** Revisit foundational ideas in later sections with light reminders and cross-links (plain `#section` anchors — the router makes this free), not full re-explanations. The per-section tooltip rule is spacing in disguise; lean into it.

Three supporting rules from the same research: **name misconceptions directly** (quiz distractors and prose callouts should say "people assume X — actually Y"; naming the wrong model is what dislodges it); **visuals carry load, text and visual reinforce rather than duplicate** (a diagram that repeats the paragraph is decoration; one that shows what prose can't is dual coding); and **terminology is frozen** — pick one name per concept and never swap synonyms for variety; every synonym is a new thing to reconcile. Finally, the cut test: if a section doesn't earn its cognitive cost, cut it or redesign it.

## Workflow

### Phase 0 — Understand the assignment

**When direction is unclear, ask — don't guess.** A learning module is a big investment of the learner's time; building the wrong one wastes an hour of theirs and all of yours. Use AskUserQuestion whenever the request leaves real forks open: What should the module **focus** on (a broad topic like "caching" hides five possible courses)? Are there **particulars to highlight** — a specific incident, a code area, a decision they need to defend? Who is the **audience**, and what conversation or decision is this preparing them for? How much **source-code depth**, and any interactive-element preferences?

Skip anything the request already specifies — don't re-ask. And if a genuine fork appears **mid-build** (e.g., the diff turns out to span three subsystems and going deep on all of them would triple the module), surface the choice to the user rather than silently picking. If running non-interactively (no user available), choose sensible defaults and say which you chose in the delivery note.

While clarifying the audience, read `references/personas.md`: two backstage archetypes — the skeptical senior engineer and the outcome-focused executive — that calibrate the module's arguments, Q&A registers, and sound-bites to whoever the learner's real conversation is with. They are never named in the module itself.

### Phase 1 — Gather facts before any HTML

- **PR provided:** don't flail — work down this ladder and stop at the first rung that works:
  1. **`gh` CLI first.** The user's own machine almost always has it authenticated. Try `gh pr view <n> --repo <org>/<repo> --json title,body,files,additions,deletions,author,state`, then `gh pr diff <n> --repo <org>/<repo>` (write to a file; strip lockfiles before reading). Important: run it **where the user's credentials live** — if your default shell is a sandbox, it likely has no `gh`/auth, so use the tool that executes on the user's actual machine (e.g., a desktop-commander/host shell) before concluding gh is unavailable.
  2. **GitHub MCP tools**, if a GitHub connector is present (search your available tools before declaring it absent).
  3. **Plain web fetch** of the PR page / `.diff` URL — works only for public repos.
  4. **Ask whether the code is local.** If gh and the connectors are all unavailable, don't give up on grounding — prompt the user (AskUserQuestion): "Is this repo cloned on your machine? Point me at the directory." A local clone is a *better* source than the PR API anyway: derive the diff with git (`git diff main...<branch>`, or `git log`/`git show` for the branch's commits), and you're already positioned for the read-around-the-diff step below.
  5. **Last resort: ask the user** to paste the diff or run `gh pr diff <n> > pr.diff` themselves and attach it.

  Read the *whole* diff. The PR description's test plan and notes are gold — they contain the author's honest open items and caveats; carry those into the module (they make the learner credible).
- **Read around the diff, not just the diff.** A diff shows what changed, never what it changed *into* — and modules built from the diff alone explain code out of context. Pull up the surrounding codebase for: the **interfaces/types the new code implements** (the diff shows a class; the base interface lives elsewhere), the **callers and entry points** that touch the changed code, the **pre-existing system it plugs into** (what did the logger/router/pipeline already do before this PR extended it?), related **config and tests**, and any **docs/ADR/plan files the PR description references**. This context is what makes the seam explanations, the annotated walkthrough, and especially the adversarial review correct — half the real review findings (other write paths not invalidated, conventions the PR breaks or follows) are invisible in the diff itself. How: the repo is usually cloned on the user's machine — find it or ask where it lives and read files directly; or fetch individual files via `gh api`/a shallow clone. Scope discipline applies: read what the module needs to explain correctly, not the whole repo.
- **Repo / files / links provided:** read or fetch them.
- **No source provided:** judge topic freshness. Fast-moving or niche → WebSearch first; stable fundamentals (SQL indexes, HTTP, caching theory) → trained knowledge is fine.
- Build a **fact sheet** as you go: exact file names, function/symbol names, env vars, endpoints, defaults, intervals, thresholds, versions, open items. The module will be fact-checked against this sheet in Phase 4.
- Record **every source you consult** (URL + one-line takeaway) in the fact sheet. These become the module's further-reading page — learners consistently want to keep going after the course, and handing them the actual documentation you used is the honest version of "further reading." Never pad this list with links you didn't open or can't vouch for.
- **Read the source critically, not at face value.** A learning module that presents flawed code as gospel teaches the flaw. While reading, log two lists in the fact sheet: *issues* (anything against best practice or likely to bite — with why it's a problem and what the correct pattern looks like) and *strengths* (deliberate good decisions — with why they're good). Both get taught: issues inline where the concept comes up ("by the way, this line has X problem — here's the standard fix") and again in the closing adversarial review section.
- Choose the **central metaphor** via `references/theming.md` (mechanics-first selection, scored candidates, stress-tested, breakdown named) and 2–4 color-coded categories (one accent color per concept type, used consistently in every diagram, card, and demo).

### Phase 2 — Architect the course

Read `references/module-blueprint.md` for the component inventory and code patterns. Then plan sections along this arc, merging or dropping steps for smaller topics:

1. Why this exists — the pain without it, told as a concrete day-in-the-life scenario
2. The core mental model(s) — the 2–4 concepts everything else hangs on
3. Deep-dive the scariest term — the word that made them ask for help
4. The standard / ecosystem — where this fits in the wider world
5. End-to-end journey — one animated walk through the whole pipeline/lifecycle
6. Their source, mapped — every file/change in one sentence, plus an **annotated line-by-line walkthrough** of the 1–3 files that carry the lesson (source-grounded modules)
7. Obstacles and gotchas — the honest "what can go wrong / what's still open"
8. The tooling they'll touch — what they'll actually click and look at
9. **The Big Picture** — a single infographic page that compresses the entire module into one visual (the central diagram with key terms, thresholds, and color legend pinned to it)
10. **Adversarial review** (source-grounded modules) — a balanced critique of the PR/source: the issues (what, why it matters, the correct pattern) and the strengths (what, why it's good), framed as training the learner to run this review themselves
11. Conversation prep — likely expert questions with strong answers ending in a **sound-bite they can say verbatim**, tuned per `references/personas.md` (engineering-reasoning answers for the skeptical senior engineer; outcome-language sound-bites for the executive) and finished with the two-lens check it describes
12. **Glossary** — the flip-card deck as its own page (not buried inside another section; learners return to it)
13. Final assessment — a cumulative quiz with a completion stamp, followed by **further reading** (the annotated sources list from Phase 1)

Default sizing for a "zero to hero" request: 8–12 sections, ≥15 quiz items across sections, ≥20 glossary terms, roughly 1500–2500 lines of HTML. For a "quick explainer" request: 4–5 sections (fold Big Picture, glossary, and sources into fewer pages rather than dropping them). Every teaching section ends with 1–4 quiz items — that's what stamps progress.

**The non-negotiables.** However the arc gets merged or themed, a full module always ships with: the visual roadmap landing page, **at least one animated process demo** (stepper, timeline, or simulator — many learners are visual-first, and this is consistently the most-praised element), per-section quizzes, **inline term tooltips throughout every section** (since pages are viewed in isolation, each technical term self-explains on first use per section — learners call this their orientation system), a glossary page of its own, the Big Picture infographic page, a further-reading/sources page, and — when a PR/repo/doc was provided — the annotated code walkthrough **and the adversarial review section**. Treat these as the definition of "done," not as options.

One quality bar on the animated demo: **animate the subject, not the scaffolding.** The demo must show the domain's actual moving parts — requests as dots flowing to cache vs. database, traffic shifting between blue and green, a span tree growing under a click — because "watching the cycle happen" is the thing visual learners came for. A generic stepper advancing through labeled boxes is navigation, not visualization; if the module's only animation is a stepper, add a real simulator of the core process.

### Phase 3 — Build

Single self-contained file, vanilla JS only (no frameworks, no CDN scripts — the file must work as a local double-click artifact). Google Fonts with system fallbacks so it degrades offline. Three-font system (display / body / mono), CSS variables for the whole palette, and a committed aesthetic — never generic defaults.

Build in chunks: `Write` the skeleton + full CSS first, then `Edit`-append sections a few at a time, then `Edit`-append the script. Writing a 2000-line file in one shot invites truncation errors; chunks keep each step verifiable.

Use the component menu from the blueprint: roadmap cards with stamps, hash router, quiz engine, term tooltips, code peeks with plain-words captions, annotated line-by-line walkthrough, pipeline stepper, timeline/waterfall demo, scenario-prediction game, Big Picture infographic, flip-card glossary page, accordion Q&A, sources/further-reading list, final quiz with certificate stamp, guarded progress persistence.

Content principles while writing:

- **Numbers stick.** Thresholds, defaults, intervals — always paired with *what moves them*.
- **Honest limitations are a feature.** "This part is still blocked on X" and "this doesn't scan free-text messages" are what make the learner sound trustworthy in the real conversation.
- **Quiz wrong-answers should be plausible** — each distractor represents a real misconception, and the explanation names why it's wrong.
- **The metaphor never replaces the term.** Say "a span — one timed stopwatch lap," not just "a lap."

### Phase 4 — Verify (not optional)

**Start with the bundled verifier**: `node scripts/verify-module.mjs <module.html>` (needs `npm i jsdom` once) **must exit 0** — it mechanically enforces the paged-navigation and progress-tracking contracts, including reload survival and the no-clobber rule. Run it as-is; never replace it with a hand-rolled smoke test (hand-rolled tests are how these bugs shipped twice) and never edit it to make a failing module pass. Then run the remaining checks in `references/verification.md`: HTML tag balance, extracted-JS syntax check, referenced-ID cross-check, quiz answer-index bounds, the jsdom smoke test for module-specific behavior, the **paged-navigation contract** (exactly one section visible at a time, every nav link resolves, every section reachable — the "one long page with dead links" failure has actually shipped, and the graceful-degradation CSS makes it look deceptively fine), and the **progress-tracking contract**: stamp propagation to all three surfaces, persistence write, cold-reload survival (seeded fresh DOM), harmless-storage-failure, and key stability across versions (the reference shows exactly how, including the jsdom gotchas — https origin, scrollTo stub). Then the **grounding audit**: every codebase claim traces to real code — script-diff the "real code" excerpts against the source, sweep every named identifier, and run the sentence-level claims ledger. If you didn't read it, the module doesn't claim it. Fix and rerun until clean. A learning module with a broken quiz or amnesiac progress bar teaches the learner to distrust the whole file.

### Phase 5 — Deliver and iterate

**The delivery folder contains exactly one file: the .html module.** Whatever sits next to the deliverable is what the user opens first — a stray `smoke-test.js` or `vcheck.py` beside the module reads as "you delivered a JavaScript file." Run verification scripts from a temp directory, and before finishing, delete anything you wrote alongside the deliverable.

Save to the user-visible outputs folder (short filename), present the file, and summarize in 2–3 sentences — what it covers and the one or two most interesting things inside. Offer the obvious next iterations: more depth on any section, restructuring, new sections. When iterating on an existing module, keep the same localStorage key so the learner's progress survives the update.
