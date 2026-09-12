---
name: verify-acs
description: Mechanically verify all acceptance criteria for a spec using ExUnit, IEx, SQL, and browser testing. Produces concrete evidence and a structured AC verification report.
---

# Verify Acceptance Criteria

Rigorous QA verification agent protocol. Mechanically test every acceptance criterion (AC) in a spec against the running codebase. Produce evidence, not opinions. Every AC gets a verdict: ✅ PASS, ❌ FAIL, or ⚠️ BLOCKED (with reason).

## 0. Identify the spec

If the user did not specify a spec file, ask which spec to verify. Specs live in the `specs/` directory at the project root. Read the spec file to extract every AC heading (`### AC-N: ...`) and its description.

Build a working list of all ACs with their numbers, titles, and descriptions. This list is your checklist — every AC must receive a verdict before you finish.

## 1. Activate engrams

Call `get_relevant_context` on the engrams MCP server to load any prior context about the spec, its ACs, and previous verification attempts. If no relevant context exists, proceed without it.

## 2. Ensure the dev servers are running

Before any testing, verify that both the backend API and frontend dev server are available. Check active terminal sessions for running processes.

**Backend (Phoenix):**
- Look for a running `mix phx.server` or `iex -S mix phx.server` process.
- If none is found, start one: `cd backend && iex -S mix phx.server`
- Wait for the server to be ready (look for `[info] Running ... on http://localhost:4000`).

**Frontend (Vite):**
- Look for a running `npm run dev` process in the `frontend/` directory.
- If none is found, start one: `cd frontend && npm run dev`
- Wait for the Vite ready message.

Both servers must be confirmed running before proceeding to any verification step. If either fails to start, report the error and stop.

## 3. Classify each AC by verification strategy

For every AC, determine which strategy applies (prefer automated over manual):

### Strategy A: ExUnit tests (highest confidence)
Use when the AC tests pure logic, business rules, or database side effects that have existing test coverage.

1. Search for existing ExUnit tests covering the AC behavior using `grep`.
2. Run the test file(s) with `--trace` to get per-test output.
3. Map specific test names to AC requirements.
4. Record: test file path, line range, test count, pass/fail.

### Strategy B: IEx / SQL (runtime verification)
Use to confirm runtime behavior beyond unit tests, or to verify persisted state in the dev database.

**IEx — for pure function smoke tests:**
1. Write a `mix run --no-start -e '...'` script that calls the functions under test and prints actual vs expected values.
2. Run it and compare outputs. Use `IO.puts("PASS: #{match?(..., result)}")` for machine-readable verdicts.

**SQL — for database state verification:**
1. Write queries against the dev database (`core_dev`) to verify persisted state.
2. Use `PGPASSWORD=postgres psql -h localhost -U postgres -d core_dev -c "..."`.
3. Check record existence, column values, timestamps, foreign key relationships.

### Strategy C: Browser (agent-browser)
Use when the AC describes user-visible behavior that cannot be verified by unit tests or database queries alone.

1. `agent-browser open http://localhost:5173` — start session
2. Log in via dev login form using "admin@example.com"
3. Navigate using `pushstate` for SPA routes
4. Use `snapshot -i` to get interactive element refs
5. Interact with elements by ref (`click @e5`, `fill @e3 "value"`)
6. Use `wait --load networkidle` after navigation and data-loading actions
7. Use `snapshot` to verify rendered content
8. Take `screenshot` for visual evidence, saving to artifacts directory
9. `agent-browser close` — always close when done

## 4. Execute verification

Work through each AC systematically:

1. State which strategy(ies) you are using and why.
2. Execute the verification — run tests, queries, or browser commands.
3. Record the evidence — test output, query results, accessibility tree excerpts, screenshots.
4. Assign a verdict: ✅ PASS, ❌ FAIL, or ⚠️ BLOCKED.

**If an AC fails:** Record exact failure; do not attempt to fix code. Continue to next AC.

## 5. Produce the verification report

Create a walkthrough artifact (`walkthrough.md`) with summary, coverage matrix, phase details (ExUnit, IEx/SQL, Browser), failures, and blocked items.

## 6. Log results to engrams

Call `log_progress` on the engrams MCP server with summary of ACs, spec identifier, and verification methods used.

## Rules

1. Evidence over assertion.
2. One AC at a time.
3. Static code analysis is a last resort.
4. Do not modify source code.
5. Do not skip ACs.
