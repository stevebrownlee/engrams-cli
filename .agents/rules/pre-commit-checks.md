---
trigger: always_on
---


# Pre-Commit Checks

Before creating any git commit, you MUST run the full sequence:

1. `cd backend && mix compile --force --warnings-as-errors`
2. `cd backend && mix reset_db_test`
3. `cd backend && mix credo --strict`
4. `cd backend && mix format`
5. `cd frontend && npm run lint:fix`
6. `cd frontend && npm run format`
7. `cd frontend && npm run type-check`
8. `cd frontend && npm run lint`

Run backend checks (steps 1-4) in parallel with frontend checks (steps 5-8).

Fix any small errors (syntax, compile warnings, lint issues, minor type issues) and re-run that check to confirm. For anything substantial, list the issues and ask the user for direction before proceeding.

Do NOT commit until all checks pass with zero errors.

## Cascading Re-runs

Any code change — even a one-line fix — can invalidate previously-passing checks. After fixing a failing check, re-run **format**, **lint**, **credo**, **type-check**, and **compile** for the affected stack before moving on.

1. Fix the failing check (test, type error, compile warning, etc.)
2. Re-run all fast checks for the affected stack (backend: compile, credo, format; frontend: lint:fix, format, type-check, lint)
3. Only then move on to the next check or commit
