---
trigger: glob
globs: backend/**/test/**/*.exs
description: "Elixir test conventions and patterns"
---


<rules>

<rule name="test-infrastructure">
<instructions>
1. Use the pre-configured `conn` from ConnCase setup (includes auth, org_id, etc.) — don't rebuild auth/user setup in each test
2. Use ExMachina factories (`insert(:thing)`) for test data — Factory is imported in ConnCase
3. Run tests with `cd backend && mix test` (use `--failed` to rerun only failed tests)
</instructions>
</rule>

</rules>
