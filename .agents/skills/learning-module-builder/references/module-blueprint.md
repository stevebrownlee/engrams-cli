# Module Blueprint — components and code patterns

Battle-tested patterns for the interactive components. Adapt names, colors, and copy to the module's theme; keep the mechanics. All vanilla JS, all in one file.

## Contents

1. Page shell & theming
2. State + persistence
3. Hash router (one section per page)
4. Roadmap (TOC) cards
5. Quiz engine
6. Term tooltips & "in plain words" boxes
7. Code peeks
8. Pipeline stepper
9. Timeline / waterfall demo
10. Scenario-prediction game
11. Flip-card glossary (its own page)
12. Accordion Q&A with sound-bites
13. Final quiz + completion stamp
14. Section footers (prev/next)
15. Big Picture infographic page
16. Annotated line-by-line code walkthrough
17. Sources / further reading
18. Adversarial source review (source-grounded modules)
19. Consolidation & contrast patterns (recaps, contrast tables, reflection, cross-links)

---

## 1. Page shell & theming

- Fixed left sidebar (~280px): course callsign, progress bar, one nav link per section with a completion tick. Hide it under ~1020px (the roadmap page is the TOC on mobile; section footers handle navigation).
- Main column: max-width ~880px, generous padding.
- Define the entire palette as CSS variables: `--bg`, `--panel`, `--line`, `--text`, `--text-dim`, plus one accent variable per concept category. Color-code categories *consistently* — if logs are amber on the overview card, logs are amber in every diagram, chip, and demo.
- Three fonts via Google Fonts `<link>` with system fallbacks (display / body / mono). Pick characterful ones that fit the theme; never Inter/Roboto/Arial.
- Atmosphere: subtle background gradients, a faint grid or texture via CSS only, one signature animated element on the hero **drawn from the chosen metaphor's world** (moving lane dashes for a highway theme, drifting steam for a kitchen, a stamping postmark for postal, a rotating sweep for a control room — see references/theming.md; never default to a previous module's signature). No images, no external JS.
- Sections are `<section class="leg" id="leg1">…`. **The mechanical hooks are frozen**: `body.paged`, `section.leg[id]`, `.current`, `.qitem[data-answer]`, `.qopt`, `.qwhy` keep these exact names in every module — the bundled verifier (`scripts/verify-module.mjs`) depends on them. Theme everything *visible* freely (call sections "shifts", "tickets", "checkpoints" in headings, nav labels, and copy), but never rename the machine-facing classes/attributes. Themed presentation, frozen plumbing.

Paged mode CSS — content is fully readable if JS never runs (graceful degradation), and pages only collapse when the script adds `body.paged`:

```css
body.paged section.leg{display:none}
body.paged section.leg.current{display:block}
body.paged #homePage{display:none}
body.paged #homePage.current{display:block}
```

## 2. State + persistence

One state object, one guarded storage key. localStorage can throw (file:// in some browsers, privacy modes) — never let persistence break the module:

```js
var LEGS = ["leg1","leg2", /* … */];
var state = { done:{}, last:"home" };
function loadState(){
  try{
    var raw = localStorage.getItem("<module-slug>");
    if(raw){ var p = JSON.parse(raw); if(p && p.done) state = p; }
  }catch(e){}
}
function saveState(){
  try{ localStorage.setItem("<module-slug>", JSON.stringify(state)); }catch(e){}
}
function markLegDone(id){
  if(!state.done[id]){ state.done[id]=true; saveState(); refreshProgress(); }
}
```

`refreshProgress()` updates the sidebar ticks, the % bar, and re-renders roadmap cards. When shipping a v2 of an existing module, keep the same storage key so progress survives.

Implement this pattern **verbatim, as one path**: quiz completion → `markLegDone()` → `saveState()` + `refreshProgress()` — never stamp UI surfaces directly from a quiz handler or write storage from more than one place. Every progress bug that has shipped came from splitting this path (UI updated but state never saved, or state saved but a boot-time reset clobbered it). The bundled verifier tests the whole chain, including reload survival and the no-clobber rule.

## 3. Hash router

```js
function showPage(id){
  if(id!=="home" && LEGS.indexOf(id)<0) id = "home";
  document.querySelectorAll("#homePage, section.leg").forEach(function(el){ el.classList.remove("current"); });
  ((id==="home") ? document.getElementById("homePage") : document.getElementById(id)).classList.add("current");
  // sidebar active state, scrollTo(0,0), state.last = id, saveState()
}
function route(){ var h=(location.hash||"").replace("#",""); showPage(h || state.last || "home"); }
window.addEventListener("hashchange", route);
// boot: loadState(); document.body.classList.add("paged"); …wire everything…; route();
```

All navigation is plain `<a href="#legN">` — sidebar links, roadmap cards, section footers. Back/forward buttons work for free.

## 4. Roadmap (TOC) cards

Home page = hero (what you'll be able to do afterward — as concrete "you will be able to say…" bullets) + a grid of section cards. Each card: section number chip, title, 1-line description, time estimate, and a stamp state ("quiz to stamp" → "✓ STAMPED"). Keep card metadata in one `LEG_META` object so cards, footers, and titles never drift apart. Add a "Continue" button that jumps to the first unstamped section.

## 5. Quiz engine

One engine for section checks AND the final quiz. Markup contract:

```html
<div class="qitem" data-answer="1">
  <div class="qq">The question?</div>
  <div class="qopts">
    <button class="qopt">Wrong</button><button class="qopt">Right</button><button class="qopt">Wrong</button>
  </div>
  <div class="qwhy"><b>Right.</b> The explanation — teach here, not just confirm.</div>
</div>
```

Behavior: wrong click → brief red flash, stays enabled (retry is the learning loop); correct click → lock the item, reveal `.qwhy`, record whether the *first* attempt was right (for final-quiz scoring), and check whether every `.qitem` in the section is done → `markLegDone(sectionId)`. Explanations only reveal on success so the reveal never spoils retries.

Distractors must be plausible misconceptions; the `.qwhy` names why the tempting wrong answer is wrong ("people assume X — actually Y").

Two quiz patterns worth reaching for deliberately: **discrimination items** for the module's confusable pairs — "Which one is this: CSP or CORS?" / "staleTime or gcTime?" — because telling twins apart is a distinct skill from knowing each; and **prediction items placed *before* a demo** ("Before you press Run: which span will be longest?") so the demo becomes the answer reveal. A learner who committed to a guess remembers the outcome; one who just watched, doesn't.

Final quiz: render from a data array so a Reset button can rebuild it; score = first-try correct count; show the completion stamp (an animated bordered "certified" panel) only above a threshold (e.g., 10/12).

## 6. Term tooltips & plain-words boxes

Inline term with hover definition (CSS-only, works on the `data-def` attribute):

```html
<span class="term" data-def="One timed unit of work: name, start, end, labeled details.">span</span>
```

```css
.term{border-bottom:1.5px dotted var(--accent);cursor:help;position:relative;white-space:nowrap}
.term:hover::after{content:attr(data-def);position:absolute;left:0;bottom:calc(100% + 8px);
  width:270px;white-space:normal; /* panel styling */ }
```

"In plain words" box: a visually distinct callout (left accent border) used after every dense passage. If a section has three paragraphs with no plain-words box, it's too dense — add one or simplify.

Tooltip density rule: because sections are viewed **in isolation**, never assume the reader saw an earlier page's definition. Every technical term gets a tooltip on its first use *within each section*, even if it was formally introduced two sections ago. Learners report the always-available hover definition is one of the highest-value features — it's their orientation system when they jump around. Cheap to add, so err on too many.

For rows of measured things (metrics, limits, config values): a badge-per-row layout where the badge tooltips the formal definition, the row shows the spelled-out name, thresholds as good/poor chips, and a "what moves it" line. Learners asked for exactly this level when they said "more detail."

## 7. Code peeks

```html
<div class="codepeek">
  <div class="cphead"><span class="fname">src/…/theFile.ts</span><span class="real">real code from your PR</span></div>
  <pre>…8–15 line excerpt, trimmed, minimal <span> highlighting…</pre>
  <div class="cpsay"><b>In plain words</b>What this excerpt means and why it exists.</div>
</div>
```

Rules: excerpts must be verbatim from the fact sheet source (badge only says "real" if it is; illustrative examples get an "illustrative" badge instead). Escape `<` and `>` in code. Every peek carries a plain-words caption — never show code and assume it explains itself. Showing a wire-format payload (the actual JSON that travels) is consistently one of the highest-value peeks: it demystifies "protocol" words instantly.

## 8. Pipeline stepper

For any multi-stage process (a request's journey, a build pipeline, data flow): a horizontal station map (icon circles + connecting dashes) with prev/next buttons and a body panel per station — plain-words paragraph + which real file/system implements it. Track position in one index variable; render map + body from a `STATIONS` array. Let users click stations directly. Final station's Next button becomes "Journey complete ✓" (disabled).

A stepper is **explanation**, not visualization — it can't be the module's only animated element. Pair it with a simulator (§9-style) that animates the subject itself: the learner should be able to *watch the core cycle happen* (dots of traffic actually moving between versions, requests actually bouncing off the cache), not only read stations about it. When feedback compares modules, the one whose animation shows the domain's real moving parts wins every time.

## 9. Timeline / waterfall demo

For anything with durations and nesting (traces, request lifecycles, render pipelines): rows = labeled items, bars positioned/sized by start/duration percentages, animated in sequence on a "Run" button (transition width, staggered setTimeout), click a bar → detail panel with its attributes and a teaching note. Make one bar visibly the culprit — the demo should let the learner *diagnose* something, not just watch. If items come from different systems (browser vs backend), color them differently and say so in the detail note.

## 10. Scenario-prediction game

A quiz variant for rule systems (gates, permissions, failure modes): each item is a concrete scenario, options are usually just Yes/No ("does it get through?"), and the explanation traces *which rule* decided the outcome. 4–6 scenarios covering each rule at least once, including one where the system is fine but the *record* is filtered (rule-vs-record distinction). This is the single best format for "what determines whether X happens" content.

## 11. Flip-card glossary (its own page)

Grid of cards: front = term + tiny category hint, back = 1–2 sentence plain definition (rotateY flip on click). 20+ terms for a full module. Write backs as standalone — each card must make sense to someone who skipped every section. Render from a data array.

Give the glossary its **own roadmap card and nav entry** — don't bury it inside conversation prep. It's the page learners revisit most after finishing; a dedicated entry makes it one click from anywhere. (Its quiz-free nature is fine: mark it stamped on first visit, or leave it stampless.)

## 12. Accordion Q&A (conversation prep)

`<details class="acc">` per likely expert question. Body: 1–2 honest paragraphs (including tradeoffs and limitations — those build credibility) ending in a highlighted **sound-bite answer** box: one or two sentences the learner can say verbatim. 8–10 questions. Source real ones from: the PR's design choices ("why X instead of Y?"), the domain's classic objections (cost, security, privacy, performance, vendor lock-in), and anything still broken/open.

## 13. Final quiz + completion stamp

12-ish questions spanning every section, rendered from data (see §5). Below it, a "your 30-second summary" plain-words box: one memorizable paragraph compressing the whole module — this is frequently the single most-used artifact of the entire course.

## 14. Section footers

Generated in JS from `LEG_META` (never hand-written per section): home link, prev link, spacer, "Next — <title> →" (last section links back home). Keeps navigation impossible to desync from the section list.

## 15. Big Picture infographic page

One dedicated page that compresses the **entire module into a single visual** — the poster a learner would pin above their desk. Build it from the module's central diagram (the pipeline, lifecycle, or architecture), then pin everything onto it: the key terms at the spot where they live, the important numbers (defaults, thresholds, intervals) as small chips, the color legend for the module's concept categories, and a one-line takeaway per major stage.

Mechanics: pure CSS/SVG (no images), ideally fits one to two viewport heights, gets its own roadmap card ("The Big Picture"), and reuses the exact colors/terms from the rest of the module so it reads as a summary, not a new lesson. A learner who has finished the course should be able to re-derive every section from this one page; a learner who hasn't should still find it a legible map of what's coming. Place it after the deep-dive sections, before conversation prep.

## 16. Annotated line-by-line code walkthrough

For source-grounded modules (PR/diff/repo provided), short code peeks are not enough for the files that carry the lesson — those deserve a **full annotated walkthrough**: the real code with per-line or per-block annotations explaining what each piece does and why it's there.

Mechanics: a two-column grid (code left, annotations right) or numbered markers on lines that expand/highlight a note on click or hover; annotation and line highlight together. Rules: verbatim code only (badge it "real code"); annotate meaningful units (a guard clause, a config block), not literally every semicolon; each annotation says what the unit does AND why it exists — the "why" is the teaching. Limit to the 1–3 most instructive files, and keep using compact code peeks (§7) everywhere else. Learners consistently rate this the single most valuable element of PR-grounded modules — it's the moment abstract concepts snap onto their actual code.

## 17. Sources / further reading

A short annotated list, rendered near the end (own small page, or the closing block of the final section): each entry = title, a type chip (official docs / article / repo / the PR itself), a link, and one line of "read this if…" guidance so learners know which door to open next.

Populate it from the Phase 1 fact sheet's source log — only links actually consulted during research, plus the official documentation of the libraries/tools taught and (for grounded modules) the PR/files themselves. Never invent or guess URLs: a dead or wrong link in a "further reading" list quietly poisons trust in everything else. If nothing was fetched (a stable-fundamentals module built from trained knowledge), list only the canonical official docs and say so plainly.

## 18. Adversarial source review (source-grounded modules)

A dedicated section near the end that refuses to take the PR/source at face value — because a module that presents flawed code as gospel *teaches the flaw*, and because the learner's real upcoming conversation (an eng review!) will contain exactly these critiques. The goal is to train them to run the review themselves.

Structure it two-sided and specific:

- **Issues** — each one: the verbatim code (or the precise location), a severity chip (e.g., "will bite" / "worth raising" / "nit"), *why* it's a problem in plain words, and **the correct pattern shown as code**. Draw from the Phase 1 issues log: inconsistencies, best-practice violations, missing tests, edge cases, drift between config and code.
- **Strengths** — each deliberate good decision with *why* it's good ("fail-open means the cache can never take down a request — that's the right default"). Praising the right things teaches judgment as much as catching the wrong things.

Rules: every claim traces to the actual source (severity and critique are judgment, but the *evidence* is verbatim); don't manufacture issues to seem rigorous — if the source is clean, a short "what I'd double-check anyway" list is honest and still useful; and connect each item to the conversation ("if asked about X, here's your answer"). Also teach issues **inline** at the moment the relevant concept comes up ("notice this line hard-codes the default the config already owns — here's the fix"), then aggregate them here. A closing quiz item that asks the learner to *spot* one of the issues in a fresh code snippet is a strong finisher.

## 19. Consolidation & contrast patterns

Small, cheap patterns that convert reading into retention — use them throughout rather than as a section of their own:

- **Recap box** ("What we just did and why it worked"): 2–3 lines at the end of a dense section or after a demo, *before* the quiz. It's the breath between learning and testing — a compressed restatement in already-introduced vocabulary, never new material.
- **Contrast table / twin cards** for confusable pairs: two columns, the same 4–5 rows (what it is, who enforces it, when it bites, how to check it), so the eye can diff. Follow immediately with a discrimination quiz item (§5). One well-chosen contrast beats two separate explanations.
- **Varied examples**: when a core concept gets one example, give it a second in different clothes (another endpoint, another failure mode, another data shape) — same principle, different surface. Two examples that rhyme teach the pattern; label the rhyme explicitly ("same rule, new costume").
- **Cross-links as spacing**: when a later section touches an earlier idea, add a one-line reminder plus a `#section` anchor ("batching again — same mail-bag trick from Leg 05") instead of re-explaining or assuming memory. The hash router makes these free, and each re-encounter is spaced practice.
- **Closing reflection prompt**: one open question at the very end, after the certificate — "Where in *your* project would this apply first?" Unlike quizzes it has no right answer; its job is transfer, moving the module's ideas onto the learner's own work. One is plenty; ritualized reflection prompts everywhere lose their signal.
