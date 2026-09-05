# Personas — the people the module must survive

Adapted from the allhailai/core onboarding personas. These two archetypes are **backstage context, never named in the module itself**. They represent the two audiences every technical learning module ultimately serves: the expert the learner will have to converse with, and the decision-maker the topic ultimately answers to. Use them to calibrate the module's arguments, Q&A prep, sound-bites, and adversarial review — a module that satisfies both lenses prepares the learner for any room.

## How to use them in this skill

- **Phase 0**: figure out which persona (or both) the learner's upcoming conversation is with. "I want to hold my own with our systems engineer" → Marcus. "I have to justify this at the platform review" → Dana. Both appear in most eng-review settings.
- **Section 1 (why this exists)**: write the opening pain story so both lenses see stakes — Marcus needs the *failure mode* ("here's what breaks and when"), Dana needs the *cost/risk* ("here's what it costs us per quarter not to have this").
- **Conversation prep**: tune each Q&A to its likely asker. Marcus-type questions get engineering reasoning; Dana-type questions get outcome language. Where useful, give a sound-bite in **both dialects** — the learner may need to translate mid-meeting.
- **Adversarial review**: run it to Marcus's standard (below). If a critique or defense wouldn't survive Marcus, it doesn't belong in the module.
- **The module's own claims**: every technical argument in the module should pass the **Marcus test** — no appeals to authority, no hand-waving, evidence and failure modes only.

---

## Persona 1: Marcus — the Skeptical Senior Engineer

*The person the learner is most often preparing to talk to.*

**Who he is.** A senior technical lead, ~12 years in — sharp, opinionated, respected, and genuinely good at his job. He evaluates everything through a **technical quality** lens: is this the right abstraction, will it scale, is it maintainable, what does it cost at runtime. These instincts have served him for a decade, which is exactly why he applies them everywhere — including places they don't transfer.

**How he argues.**

- **Dismisses authority and process arguments outright.** "Because the vendor requires it," "because that's the standard," "because the PR author said so" — he nods politely and keeps pushing. He responds to *engineering reasoning only*: show him the failure mode, show him what breaks and when, and he takes it seriously. This is why the module's every claim must trace to evidence — the learner will be quoting it at Marcus.
- **Pattern-matches new designs to past pathologies.** Overhead, premature abstraction, over-engineering, unnecessary indirection — he has correctly fought all of these before, and a new architecture (an observability seam, a caching layer, a vendor-agnostic boundary) triggers those matchers first. The module must show *what is different about this context* that makes the design valuable, not assert that it is.
- **Questions the design itself, not just details.** His opening move is "why does this exist at all?" — the module's learner needs an answer to the fundamental challenge, not just the implementation trivia.
- **His blind spot is unfamiliar economics.** Concepts outside his lived experience (platform/upstream models, telemetry cost curves, cache-consistency tradeoffs at fleet scale) aren't obvious to him — not from incapacity but from never having needed them. The module should hand the learner the concrete "here's what goes wrong without it" story that fills that gap, without a whiff of condescension.

**The register he speaks in (calibrate Q&A difficulty to this):**

> "I've looked at this for two weeks — it's adding complexity without clear benefit. Why are we maintaining this at all?"

> "Show me what we'd lose by removing it. Not policy — an actual failure mode."

> "I've seen this pattern before: flexibility that never materializes while everyone pays the complexity tax today."

**What satisfies him:** verbatim evidence, named failure modes, honest tradeoffs ("here's the cost we accepted and why"), and the speaker knowing what's still broken. Nothing wins Marcus over faster than the learner volunteering the design's weaknesses before he finds them — which is exactly what the adversarial-review section trains.

---

## Persona 2: Dana — the Outcome-Focused Technology Executive

*The register the topic ultimately answers to — and often the second conversation the learner didn't prepare for.*

**Who she is.** VP of Technology / CTO-scale. A strong engineer years ago — can still read a PR — but her job now is technology strategy: cost, risk, optionality, and whether an investment can be explained upward. She evaluates through a **business value** lens: what does this cost now and over time, what risk does it create or mitigate, does it accelerate or constrain us in 12 months, can I explain it to the board.

**How she operates.**

- **Speaks in outcomes, not implementations.** She never says "we need clean abstractions"; she says "we can't afford to lose next quarter's platform improvements." Sound-bites aimed at her must translate the technical decision into cost, risk, or speed — the module should teach the learner both dialects.
- **Worries about what she can't see.** Slow drift, silent failures, invisible costs accumulating until they surface as a crisis — her deepest fear. Topics like observability, monitoring, and boundary maintenance are *literally answers to her fear*; framing them that way gives the learner an instant executive narrative.
- **Protects the long game under short-term pressure — imperfectly.** Under enough delivery pressure she becomes the person who says "just ship it, we'll fix it later," undercutting her own principle — and she knows it. Honest modules acknowledge this tension instead of pretending policy is self-enforcing.
- **Her blind spot is assuming the business case is obvious.** She's lived in these economics for years and forgets that a working engineer may never have seen them. The module bridges exactly this gap: it gives the Marcus-type learner the Dana-level *why*, concretely, with what-goes-wrong stories instead of strategy slogans.

**The register she speaks in (calibrate exec-facing sound-bites to this):**

> "Every time we skip this, we're writing a check our future selves have to cash. I need the team to understand that."

> "My board doesn't want to hear about directory structures. They want to hear the investment is paying off."

> "What does this cost us — now, and over time? And what does it save?"

**What satisfies her:** a one-breath outcome statement (the module's "30-second summary" should work on Dana verbatim), a named risk being retired, a cost dial she can point to (sampling rates, kill switches, per-signal toggles), and evidence the team understands *why* — not just *that* — the practice exists.

---

## The two-lens check (quick pass before delivery)

Scan the finished module once per lens:

- **Marcus pass**: does every claim carry evidence? Does the module name real failure modes and honest tradeoffs? Would the conversation-prep answers survive his "show me, don't tell me"? Is anything justified by authority alone? (Fix those.)
- **Dana pass**: does the opening story translate to cost/risk? Does at least one sound-bite per major Q&A work in outcome language? Does the 30-second summary work on someone who will never read the code?

A module that passes both is conversation-ready in any room the learner walks into.
