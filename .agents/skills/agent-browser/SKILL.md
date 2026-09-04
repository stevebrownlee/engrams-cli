---
name: agent-browser
description: Browser automation via agent-browser CLI for UI verification, testing, and DOM inspection (snapshots, refs @e1/@e2, SPA pushstate, batching, screenshots).
---

# agent-browser (UI Verification & Inspection)

`agent-browser` (v0.27+) is a CLI for AI-agent browser automation via Chrome DevTools Protocol. It uses a compact accessibility-tree snapshot model where interactive elements get deterministic refs (`@e1`, `@e2`, …).

## When to use

- Verifying user-visible behavior (rendered text, form state, navigation)
- Gate 3 or Gate 5 verification requiring manual or visual checks
- Confirming UI regression fixes or interactive state changes
- When prompted: "Use agent-browser to self-verify UI changes"

Do NOT use when the behavior is fully testable via unit/integration tests.

## Core workflow

Load the agent-browser skill for the full command reference. Browse available skills:

```bash
agent-browser skills get core --full
```

### Opening the UI

```bash
agent-browser open http://localhost:5173         # Start session
agent-browser snapshot -i                        # Get interactive element refs
agent-browser fill @e3 "admin@example.com"       # Interact by ref
agent-browser click @e5                          # Click by ref
agent-browser wait --load networkidle            # Wait for async
agent-browser snapshot                           # Re-snapshot after state change
agent-browser screenshot ./evidence/result.png   # Visual evidence
agent-browser close                              # Always close
```

## Key commands

| Category    | Commands |
|-------------|----------|
| **Navigate** | `open <url>`, `pushstate <path>` (SPA nav), `back`, `forward`, `reload` |
| **Inspect**  | `snapshot`, `snapshot -i`, `screenshot`, `get text/url/title/value` |
| **Interact** | `click`, `fill`, `type`, `press`, `select`, `check`, `hover`, `scroll` |
| **Assert**   | `get text @ref`, `get count`, `is visible`, `is enabled`, `is checked` |
| **Wait**     | `wait --load networkidle`, `wait --text "..."`, `wait <selector>` |
| **Batch**    | `batch "cmd1" "cmd2" "cmd3"` (single invocation) |
| **Diff**     | `diff snapshot` (compare before/after accessibility trees) |
| **React**    | `open --enable react-devtools`, `react tree`, `react inspect`, `vitals` |

## SPA navigation

For TanStack Router / React SPA applications, use `pushstate` for client-side navigation instead of `open` (which causes a full reload):

```bash
agent-browser open http://localhost:5173    # Initial load only
agent-browser pushstate /patients           # Client-side nav
agent-browser pushstate /patients/123       # Navigate to detail view
```

## Token efficiency

- Use `snapshot -i` (interactive only) when you only need form controls
- Use `get text @ref` for specific elements instead of full snapshots
- Use `batch` to combine multiple commands in one invocation
- Re-snapshot only after significant DOM changes

## Waiting

Prefer condition-based waits over fixed delays:

```bash
agent-browser wait --load networkidle                              # Network idle
agent-browser wait --text "Patient Name"                           # Specific text
agent-browser wait "#data-table"                                   # Element visible
agent-browser wait --fn "!document.body.innerText.includes('Loading...')"  # JS condition
```

## Produce Evidence

Once the user inteeface described in the specifications/requirements has been fully tested in both mobile and desktop mode, generate a video that shows all modified UI. Discover the specific commands needed using the command below. 

```bash
agent-browser record --help
```