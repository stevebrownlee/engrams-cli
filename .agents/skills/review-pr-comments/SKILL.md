---
name: review-pr-comments
description: Review and address comments on a GitHub Pull Request. Retrieve PR comments via the GitHub CLI, judge each on validity, implement fixes for valid comments, commit them via the commit skill, and reply to each review comment individually with a terse resolution note.
---

# Review PR Comments

Review the comments on the GitHub Pull Request in scope. Use the GitHub CLI skill
(`.agents/skills/github-cli/SKILL.md`) to retrieve them.

1. Determine whether each comment is valid.
2. For each valid comment, devise a fix strategy.
3. Implement the fix for each valid comment.
4. After all fixes are made, use the commit skill
   (`.agents/skills/commit/SKILL.md`).
5. Respond to every comment with a terse description of the resolution (if any).
   Do **not** leave a new PR-level comment — reply to each review comment
   individually.
