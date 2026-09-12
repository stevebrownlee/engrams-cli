---
trigger: always_on
---


<context>
This project is an Elixir/Phoenix umbrella app (backend/apps/) with a React/TypeScript
frontend (frontend/). These rules apply to ALL code in the repository.
</context>

<rules>

<rule name="documentation">
<instructions>
Prefer inline documentation over separate documentation files.
1. In Elixir: Use @moduledoc and @doc attributes
2. In TypeScript: Use JSDoc comments for functions and types
3. Only create README or .md documentation files when explicitly requested
4. Do NOT add @doc or JSDoc to trivial functions where the name and signature are self-explanatory
   - Trivial: `user_has_permission?(user_id, asset_key, action_key)` - name says it all
   - Trivial: `getUserById(id: string)` - obvious from name
   - Non-trivial: Complex business logic, non-obvious return values, or functions with important side effects
5. Do not copy documentation patterns from existing code without considering if documentation is actually needed
6. No ASCII-banner section dividers in `.ts` / `.tsx`. Enforced by `no-restricted-syntax` (`BANNER_COMMENT_REGEX`). File structure is conveyed by exported-symbol order and natural grouping, not decorative bars.
7. No narrator comments that restate framework, compiler, or library behavior. Comments explain non-obvious *intent* or a *constraint the code cannot convey*.
   - BAD: `// This component renders a list of users` (restates JSX)
   - GOOD: `// Round up: pharmacy billing requires whole-cent precision` (non-obvious constraint)
</instructions>
</rule>

<rule name="testing">
<instructions>
When testing, create unit tests, not script files to test functionality.
1. Backend: Use ExUnit with ExMachina for test data
2. Frontend: Use Vitest with Testing Library
3. Place test files in paths mirroring the source files
</instructions>
</rule>

<rule name="secrets">
<instructions>
Never hardcode secrets in code or configuration files. Use environment variables read at runtime
</instructions>
</rule>

<rule name="think-before-coding">
<instructions>
Don't assume. Don't hide confusion. Surface tradeoffs. Before implementing:
1. State assumptions explicitly — if uncertain, ask
2. If multiple interpretations exist, present them — don't pick silently
3. If a simpler approach exists, say so — push back when warranted
4. If something is unclear, stop — name what's confusing and ask
</instructions>
</rule>

<rule name="simplicity-first">
<instructions>
Minimum code that solves the problem. Nothing speculative.
1. No features beyond what was asked
2. No abstractions for single-use code
3. No "flexibility" or "configurability" that wasn't requested
4. No error handling for impossible scenarios
5. If you write 200 lines and it could be 50, rewrite it
The test: would a senior engineer say this is overcomplicated? If yes, simplify.
</instructions>
</rule>

<rule name="surgical-changes">
<instructions>
Touch only what you must. Clean up only your own mess.
When editing existing code:
1. Don't "improve" adjacent code, comments, or formatting
2. Don't refactor things that aren't broken
3. Match existing style, even if you'd do it differently
4. If you notice unrelated dead code, mention it — don't delete it
When your changes create orphans:
5. Remove imports, variables, functions, comments, and TODOs that YOUR changes made unused or irrelevant. If a TODO referenced deleted code, delete the TODO. Stranded `eslint-disable-*` directives are enforced by `eslint-comments/no-unused-disable`; the TODO / comment half is yours to clean.
6. Don't remove pre-existing dead code unless asked
The test: every changed line should trace directly to the user's request.
</instructions>
</rule>

<rule name="goal-driven-execution">
<instructions>
Define success criteria. Loop until verified.
Transform tasks into verifiable goals:
- "Add validation" → write tests for invalid inputs, then make them pass
- "Fix the bug" → write a test that reproduces it, then make it pass
- "Refactor X" → ensure tests pass before and after
For multi-step tasks, state a brief plan with verification steps:
1. [Step] → verify: [check]
2. [Step] → verify: [check]
Strong success criteria let you loop independently. Weak criteria ("make it work") require clarification — ask for it.
</instructions>
</rule>

<rule name="ai-rules-source-of-truth">
<instructions>
Never edit `.cursor/rules/` or `.agents/rules/` directly — those trees are generated. Change policy only under `docs/rules/` (see `docs/rules/README.md`), then run `cd backend && mix rules.sync`.
</instructions>
</rule>

</rules>
