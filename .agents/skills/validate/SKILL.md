---
name: validate
description: Run pre-PR validation suite and checks for backend and frontend — compile with warnings-as-errors, reset test DB, Credo, Elixir format, frontend lint:fix, type-check, unit tests, format, audit, and check-cycles.
---

<!-- managed by PILOT — generated from commands/validate.md, do not edit by hand -->
<!-- to customize, edit the source under .pilot/commands/validate.md and re-run install -->

# Backend Checks

Run `cd backend` to be in the correct directory

1. Compile with warnings as errors: `mix compile --force --warnings-as-errors`
2. Reset the test database: `MIX_ENV=test POSTGRES_HOST=localhost mix reset_db_test`
3. Run Credo for code analysis (always with `--strict`): `mix credo --strict`
4. Run Elixir formatter: `mix format`

# Frontend Checks and Fixes

Run `cd ../frontend` to be in the correct directory

Then run the following commands:

```
npm run lint:fix
npm run type-check
npm run test:unit
npm run format
npm run audit
npm run deps:check-cycles
```

Fix any of the small errors like syntax issues, compile warnings, lint issues, small type issues etc. After fixing small things run that check again to make sure it is fixed. Anything substantial you should list for me and I will give you the next steps.
