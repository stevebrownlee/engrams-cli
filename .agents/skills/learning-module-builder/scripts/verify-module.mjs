#!/usr/bin/env node
/**
 * verify-module.mjs — generic contract verifier for learning modules.
 *
 * Enforces the paged-navigation and progress-tracking contracts mechanically,
 * so they cannot be skipped or re-derived loosely by hand. Run it as-is —
 * do NOT rewrite a bespoke smoke test instead of this. Add module-specific
 * assertions (demo counts, glossary counts) in a separate script on top.
 *
 * Usage:   npm i jsdom   (once, anywhere; run from that directory)
 *          node verify-module.mjs /absolute/path/to/module.html
 * Exits 0 on pass; exits 1 and lists failures otherwise.
 *
 * Relies on the blueprint's frozen mechanical contract:
 *   body.paged / section.leg[id] / .current / .qitem[data-answer] / .qopt / .qwhy
 * Theme all visible copy freely — never rename these hooks.
 */
import { JSDOM } from "jsdom";
import fs from "fs";

const file = process.argv[2];
if (!file) { console.error("usage: node verify-module.mjs <module.html>"); process.exit(1); }
const html = fs.readFileSync(file, "utf8");
const fails = [];
const ok = (cond, name, detail = "") => { if (!cond) fails.push(name + (detail ? ` — ${detail}` : "")); };
const tick = () => new Promise(r => setTimeout(r, 40));

/* ---------- static layer ---------- */
const scripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].map(m => m[1]).filter(s => s.trim());
ok(scripts.length >= 1, "inline <script> present (a '// placeholder' script = unfinished build)");
const js = scripts.join("\n");
const css = [...html.matchAll(/<style>([\s\S]*?)<\/style>/g)].map(m => m[1]).join("\n");
ok(css.length > 500, "inline <style> present");

// CSS <-> JS coherence: the paging classes the router toggles must have display rules
for (const cls of ["paged", "current"]) {
  const toggled = new RegExp(`classList\\.(add|remove|toggle)\\((["'])${cls}\\2`).test(js);
  ok(toggled, `router toggles '${cls}'`);
  if (toggled) ok(new RegExp(`\\.${cls}\\b[^{}]*\\{[^}]*display`).test(css),
    `CSS gives '${cls}' a display rule (jsdom can't see layout — this catches the invisible-paging bug)`);
}

// every #href (static AND inside JS string templates) resolves to a real id.
// "home" is the router's virtual fallback page (element is usually #homePage) — always valid.
const ids = new Set([...html.matchAll(/id=\\?["']([\w-]+)\\?["']/g)].map(m => m[1]));
ids.add("home");
const hrefs = [...html.matchAll(/href=\\?["']#([\w-]+)/g)].map(m => m[1]);
for (const h of new Set(hrefs)) ok(ids.has(h), `nav link target #${h} exists`);

/* ---------- booted layer ---------- */
function boot(mode) {
  const dom = new JSDOM(html, { runScripts: "outside-only", url: "https://example.com/m.html", pretendToBeVisual: true });
  dom.window.scrollTo = () => {};
  if (mode === "THROW") {
    Object.defineProperty(dom.window, "localStorage", { get() { throw new Error("storage blocked"); } });
  } else if (mode && typeof mode === "object") {
    for (const [k, v] of Object.entries(mode)) dom.window.localStorage.setItem(k, v);
  }
  try { for (const s of scripts) dom.window.eval(s); }
  catch (e) { fails.push(`script threw on boot (${mode === "THROW" ? "blocked-storage" : "normal"}): ${e.message}`); }
  return dom;
}

const dom = boot(null); const w = dom.window; const d = w.document;
await tick();

// paged boot: exactly one page visible, and it isn't a teaching section
ok(d.body.classList.contains("paged"), "body has 'paged' class after boot");
const legs = [...d.querySelectorAll("section.leg")].map(s => s.id).filter(Boolean);
ok(legs.length >= 3, "found section.leg[id] sections", `found ${legs.length}`);
ok(d.querySelectorAll(".current").length === 1, "exactly ONE .current element at boot",
  `found ${d.querySelectorAll(".current").length}`);
ok(!d.querySelector("section.leg.current"), "boot lands on home, not a section");

// walk EVERY section — exclusive visibility each time
for (const id of legs) {
  w.location.hash = "#" + id;
  await tick();
  const cur = [...d.querySelectorAll(".current")];
  ok(cur.length === 1 && cur[0].id === id, `routing to #${id} shows exactly that section`,
    `current: ${cur.map(c => c.id || c.className).join(",")}`);
}
// bad hash falls back to a non-section page
w.location.hash = "#zzz-nonexistent";
await tick();
ok(d.querySelectorAll(".current").length === 1 && !d.querySelector("section.leg.current"),
  "bad hash falls back to home");

/* ---------- progress contract ---------- */
const firstLeg = legs[0];
w.location.hash = "#" + firstLeg;
await tick();
const keysBefore = new Set(Object.keys(w.localStorage));
const items = [...d.querySelectorAll(`#${firstLeg} .qitem`)];
ok(items.length >= 1, "first section has .qitem quiz items");
const bodyClassesBefore = [...d.querySelectorAll("[class]")].map(e => e.className).join("|");
for (const item of items) {
  const ans = parseInt(item.dataset.answer, 10);
  const opts = item.querySelectorAll(".qopt");
  ok(!isNaN(ans) && ans < opts.length, "qitem data-answer in bounds");
  // exercise the retry path (no strict assertion — feedback styles vary), then answer correctly.
  if (opts.length > 1) opts[(ans + 1) % opts.length].click();
  const before = item.outerHTML;
  opts[ans].click();
  // behavioral contract only: a correct answer must produce visible feedback of SOME kind
  ok(item.outerHTML !== before, "correct answer visibly changes the quiz item (lock/reveal/state)");
}
await tick();
// stamp visible somewhere: the DOM must have changed in class terms
const bodyClassesAfter = [...d.querySelectorAll("[class]")].map(e => e.className).join("|");
ok(bodyClassesAfter !== bodyClassesBefore, "completing the section visibly changes the UI (stamp/tick/progress)");
// persistence write
const newKeys = Object.keys(w.localStorage).filter(k => !keysBefore.has(k));
const allKeys = Object.keys(w.localStorage);
ok(allKeys.length >= 1, "a localStorage key exists after completing a section");
const storeKey = newKeys[0] ?? allKeys[0];
let savedRaw = null;
try { savedRaw = w.localStorage.getItem(storeKey); JSON.parse(savedRaw); ok(true, ""); }
catch { fails.push("saved progress state is not valid JSON"); }
ok(savedRaw && savedRaw.includes(firstLeg), "saved state records the completed section id",
  `key='${storeKey}'`);

// cold-reload survival: seed a FRESH dom with the saved state before the script runs
if (savedRaw) {
  const dom2 = boot({ [storeKey]: savedRaw });
  await tick();
  const d2 = dom2.window.document;
  const fresh = boot(null);
  await tick();
  const freshClasses = [...fresh.window.document.querySelectorAll("[class]")].map(e => e.className).join("|");
  const seededClasses = [...d2.querySelectorAll("[class]")].map(e => e.className).join("|");
  ok(seededClasses !== freshClasses, "seeded reload renders differently than a fresh boot (stamps restored)");
  // clobber check: booting must not wipe or reset previously saved progress
  const after = dom2.window.localStorage.getItem(storeKey);
  ok(after != null && after.includes(firstLeg), "boot does not clobber saved progress (the classic reset bug)");
}

// storage-failure resilience: module still boots and routes with throwing storage
const dom3 = boot("THROW");
await tick();
const d3 = dom3.window.document;
ok(d3.body.classList.contains("paged"), "boots with blocked localStorage");
dom3.window.location.hash = "#" + legs[1];
await tick();
ok(d3.querySelector(`#${legs[1]}`)?.classList.contains("current") === true, "routes with blocked localStorage");

/* ---------- verdict ---------- */
if (fails.length) {
  console.error(`FAIL — ${fails.length} contract violation(s):`);
  for (const f of fails.filter(Boolean)) console.error("  ✗ " + f);
  process.exit(1);
}
console.log(`PASS — paged-navigation + progress contracts hold (${legs.length} sections walked, key='${typeof storeKey !== "undefined" ? storeKey : "n/a"}')`);
process.exit(0); // modules run setInterval/rAF animations that would otherwise keep node alive forever
