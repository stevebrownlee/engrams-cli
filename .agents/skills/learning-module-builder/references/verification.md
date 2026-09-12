# Verification — run all of these before delivering

A learning module with a broken quiz or dead navigation teaches the learner to distrust the whole file. Every check below caught a real problem at least once. Run them in a shell against the finished HTML; fix and rerun until clean.

**Keep verification scratch out of the delivery folder.** Write test scripts (`smoke.js`, checkers, extracted JS) to `/tmp`, never beside the module — a stray `.js` next to the deliverable reads as "you delivered a JavaScript file." Final check before finishing: the delivery folder contains exactly one file, the `.html`.

## 0. Run the bundled verifier FIRST — it must exit 0

Progress tracking and paged navigation have shipped broken more than once, each time past a hand-written smoke test. Hand-rolled tests drift; the bundled one doesn't. From a directory where jsdom is installed (`npm i jsdom`, once):

```bash
node <skill-path>/scripts/verify-module.mjs /absolute/path/to/module.html
```

It mechanically enforces both contracts: paged boot (exactly one page visible), a walk of **every** section with exclusive visibility, bad-hash fallback, link resolution (including JS-generated hrefs), CSS↔JS coherence for the paging classes, quiz completion feedback, the **full progress chain** — visible UI change on section completion, valid JSON written to storage containing the section id, **cold-reload survival** (seeded fresh DOM renders restored stamps), **no-clobber on boot** (loading must not wipe saved progress — the classic reset bug), and **blocked-storage resilience** (module still boots and routes when localStorage throws).

Rules of engagement: run it **as-is** — do not substitute a bespoke smoke test for it, and do not edit it to make a failing module pass; fix the module. Add module-*specific* assertions (demo behavior, glossary counts, simulator outcomes) in a separate scratch script on top. The verifier depends on the blueprint's frozen mechanical hooks (`body.paged`, `section.leg[id]`, `.current`, `.qitem[data-answer]`, `.qopt`, `.qwhy`) — if it can't find them, the module broke the contract, not the other way around. Sections 1–4 below explain what it checks and cover what it can't (facts, visuals).

## 1. HTML tag balance + JS syntax + ID cross-check (one script)

```bash
python3 - <<'EOF'
import re, html.parser
src = open("MODULE.html", encoding="utf-8").read()

# extract the app script for node --check
m = re.search(r"<script>(.*?)</script>", src, re.S)
open("/tmp/module.js","w").write(m.group(1))

# tag balance
class P(html.parser.HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack=[]; self.errors=[]; self.void={"meta","link","br","img","input","hr"}
    def handle_starttag(self,tag,attrs):
        if tag not in self.void: self.stack.append((tag,self.getpos()))
    def handle_endtag(self,tag):
        if not self.stack: self.errors.append(f"extra </{tag}> {self.getpos()}"); return
        t,pos=self.stack.pop()
        if t!=tag: self.errors.append(f"mismatch <{t}> {pos} closed by </{tag}> {self.getpos()}")
p=P(); p.feed(src)
for t,pos in p.stack: p.errors.append(f"unclosed <{t}> {pos}")
print("HTML errors:", p.errors or "none")

# every getElementById target must exist
js = m.group(1)
ids_used = set(re.findall(r"getElementById\([\"']([\w-]+)[\"']\)", js))
ids_def  = set(re.findall(r"id=[\"']([\w-]+)[\"']", src))
print("missing ids:", sorted(ids_used - ids_def) or "none")

# quiz answer indexes in bounds (count loosely: class="qopt..." may have extra classes)
items = re.findall(r'<div class="qitem" data-answer="(\d+)">(.*?)(?=<div class="qitem"|</section>)', src, re.S)
bad = [(a, b.count('class="qopt')) for a,b in items if int(a) >= b.count('class="qopt')]
print("answer index out of range:", bad or "none", "| qitems:", len(items))
EOF
node --check /tmp/module.js && echo "JS syntax OK"
```

Gotcha already hit once: matching `class="qopt"` exactly misses buttons with extra classes (`class="qopt yes"`). Count with the loose prefix as above.

## 2. jsdom smoke test (routing, quizzes, persistence actually work)

`npm i jsdom` once, then adapt:

```js
const { JSDOM } = require("jsdom");
const html = require("fs").readFileSync("MODULE.html","utf8");
// GOTCHA 1: use an https URL — with file:// the origin is opaque and
// window.localStorage THROWS (SecurityError) when the test touches it.
const dom = new JSDOM(html, { runScripts:"outside-only", url:"https://example.com/m.html", pretendToBeVisual:true });
const { window } = dom; const d = window.document;
// GOTCHA 2: jsdom has no scrollTo — stub it BEFORE evaluating the app script.
window.scrollTo = function(){};
window.eval(html.match(/<script>([\s\S]*?)<\/script>/)[1]);

// Assert, with ~25ms waits after each hash change (hashchange dispatch is async):
// - body has the paged class; home page is .current by default
// - roadmap renders one card per section; every section got a generated footer
// - location.hash = "#leg3" → only leg3 has .current
// - clicking the correct .qopt in a section marks qitem done, ticks the sidebar,
//   bumps the progress %, stamps the roadmap card
// - "continue" button routes to the first unstamped section
// - dynamic components rendered (glossary cards, final quiz items, stepper stations,
//   waterfall bars — assert exact expected counts)
// - localStorage contains the saved state after a quiz pass
```

Print a single PASS line or a list of failures. If a count assertion fails after a content edit (e.g., you added glossary cards), update the expected count — that's the test doing its job.

### The progress-tracking contract (required, not optional)

Progress is the learner's investment in the module — if stamps vanish on reload or the bar lies, trust in the whole file dies. The smoke test must prove ALL of these, every time:

1. **Stamp propagation**: answering a section's quiz correctly marks the section done AND updates every surface at once — sidebar tick, progress %, roadmap card stamp. Assert all three from one quiz pass.
2. **Persistence write**: after the stamp, the storage key exists and parses, and contains the stamped section.
3. **Cold-reload survival** — the one teams forget. jsdom's localStorage doesn't persist across instances, so simulate a reload by seeding a *fresh* DOM with the saved state *before* evaluating the app script:

```js
const saved = window.localStorage.getItem(KEY);        // from the first DOM, after stamping
const dom2 = new JSDOM(html, { runScripts:"outside-only", url:"https://example.com/m.html", pretendToBeVisual:true });
dom2.window.scrollTo = function(){};
dom2.window.localStorage.setItem(KEY, saved);          // seed BEFORE the script runs
dom2.window.eval(html.match(/<script>([\s\S]*?)<\/script>/)[1]);
// assert: stamped sections show ticks/stamps, progress % matches, Continue targets the first UNstamped section
```

4. **Storage failure is harmless**: stub a throwing localStorage on a third DOM (`Object.defineProperty(window, "localStorage", { get(){ throw new Error("blocked"); } })` before eval) and assert the module still boots, routes, and quizzes — progress just doesn't persist. This is the try/catch guard doing its job; prove it rather than trusting it.
5. **Key stability across versions**: when editing an existing module, assert the storage key is unchanged from the previous version (grep the old file for the key literal) so learners' progress survives your update.

### The paged-navigation contract (required, not optional)

The single most user-visible failure a module can ship with — and it has shipped — is rendering as **one long scrolling page with dead navigation links**. This happens when the script never adds the paged class (boot error), when nav links point at ids that don't exist, or when the router was written but never wired. The graceful-degradation CSS makes this failure *quiet*: content still displays, so a careless check "looks fine." Prove all of these, every build:

1. **Paged boot**: after script evaluation, `document.body` has the paged class and **exactly one** page is visible — the home page has `.current`, and *zero* `section` elements do. Count them; don't just check the home page.
2. **Every link resolves**: collect **every** `href="#…"` in the document AND in JS-generated markup (sidebar, roadmap cards, section footers, cross-links) and assert each target id exists. One dead link is a bug; a pattern of them means the router and the sections disagree on naming.
3. **Walk every section, not a sample**: for **each** section id, set `location.hash`, wait a tick, and assert exactly that one section has `.current` and the home page doesn't. A loop over all sections costs three lines and catches the off-by-one that spot checks miss.
4. **Bad hash falls back**: `location.hash = "#nonsense"` lands on home, not a blank screen.
5. **Interactive elements inside late sections work after routing**: navigate to the last section, then run one interaction there (a quiz click or demo button). This catches wiring that only ran for elements visible at boot.

**Know the blind spot: jsdom verifies router *logic*, not what's on screen.** jsdom has no layout engine. If the script dutifully toggles `body.paged` and `.current` but the CSS rules that give those classes meaning are missing or misspelled, every classList assertion above passes — and the browser still renders one long scroll with jumpy links. This exact bug shipped once. Two additional checks close the gap:

6. **CSS↔JS coherence (static, always run — not just as a fallback).** Extract the class names the router actually toggles from the JS, then assert the stylesheet contains the rules that give them effect — a hide rule scoped to the paged class (e.g., `body.paged section.leg{display:none}`) and a show rule for the current class (`…current{display:block}`). A class the JS toggles that no CSS rule references is the fingerprint of this bug:

```python
toggled = set(re.findall(r'classList\.(?:add|remove|toggle)\(["\'](\w+)', js))
css = re.search(r'<style>(.*?)</style>', src, re.S).group(1)
for cls in toggled & {"paged", "current"}:   # the paging-critical ones
    assert re.search(r'\.' + cls + r'\b[^{}]*\{[^}]*display', css), f"class '{cls}' toggled by JS but no display rule in CSS"
```

7. **Real browser render when available (the gold check).** If headless Chromium is present (Playwright/Puppeteer), load the file, assert exactly one section has a nonzero bounding box, click two sidebar links and re-assert, and check for zero console errors. A screenshot of home + one mid-course section is cheap and definitive. When no real browser exists, check 6 is the required stand-in — you can also try `getComputedStyle(section).display` in jsdom (it resolves simple stylesheet rules), but don't trust it with complex selectors; the static coherence check is the reliable floor.

If node/jsdom is genuinely unavailable in the environment, the minimum static fallback is: the CSS↔JS coherence check above (it's pure regex — it runs anywhere Python does), the boot code adds the paged class, and every `href="#x"` (including in JS string templates) has a matching `id="x"` — then say plainly in the delivery note that live routing wasn't executed and the user should click through the sidebar once. Never deliver silently unverified navigation.

## 3. Grounding audit — every codebase claim traces to real code, no assumptions

The module's authority comes entirely from being *true about their code*. This step verifies that — mechanically where possible, sentence-by-sentence where not.

**3a. Presence check (programmatic).** Assert the module contains the exact strings that matter — file names, env vars, endpoints, service names, thresholds:

```python
checks = ["theExactEndpoint", "the.exact.metric.name", "THE_ENV_VAR", "theFileName.ts"]
missing = [c for c in checks if c not in src]
```

**3b. Excerpt diffing (programmatic).** Every code block badged "real code" must be verbatim. Extract the module's code blocks, HTML-unescape and normalize whitespace, then verify **each line exists in the actual diff/source file** — a script, not an eyeball:

```python
import re, html as h
blocks = re.findall(r'<pre>(.*?)</pre>', src, re.S)          # pair with their badge/fname headers
source = open("the/real/file.ts").read()                      # or the diff
for line in code_lines(blocks):                               # strip <span> tags, unescape, trim
    assert norm(line) in norm(source), f"NOT IN SOURCE: {line}"
```

Any line that fails is either a transcription error (fix it) or an invention (delete it or re-badge the block "illustrative").

**3c. Identifier sweep (programmatic).** Collect every code-styled identifier the module names (function names, env vars, config keys, metric names, paths) and grep each against the diff/repo. An identifier that exists nowhere in the codebase is a hallucination wearing a monospace font.

**3d. Claims ledger (manual, sentence-by-sentence).** Reread every sentence that asserts something about the user's code or system and write down where the evidence lives (file + line, or PR-description quote). The rule is absolute: **if you didn't read it, the module doesn't claim it.** Claims with no evidence get one of three fates — go read the code that would prove it (see "read around the diff" in SKILL.md), soften it into an explicit question ("worth confirming whether other write paths invalidate this key"), or delete it. Be extra suspicious of: fields/attributes the system "sends" (verify each is in the code), behavior under failure (verify the guard/try-catch actually exists), anything "always/never/only" (absolutes need the strongest evidence), and adversarial-review items (each issue's evidence must be verbatim-quotable; each "correct pattern" clearly marked as proposed, not existing code).

"I don't know" costs nothing; a wrong claim repeated to an expert costs the learner the exact credibility this module exists to build.

## 4. Visual sanity

If a browser/screenshot tool is available, open the file and check: fonts loaded, sidebar and roadmap render, one section navigates, a quiz answers, nothing overflows on a narrow viewport. If no browser is available, say so in the delivery note and rely on §1–§3.
