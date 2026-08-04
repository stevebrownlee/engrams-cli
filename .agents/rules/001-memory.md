---
id: 001-memory
title: Prime on conversation start
description: engrams prime must run as the first action of every conversation
priority: critical
always_apply: true
---

# Prime First

Run `engrams prime` as the very first action of every conversation — before any other tool call, file edit, or project question is answered.

It returns the product context, the active-context track for the current scope, recent decisions, patterns, progress, and a compact graph summary. Acting on this keeps every session grounded instead of blind. Add `--budget <tokens>` to cap output; `--paths <p1,p2>` / `--tags <a,b>` to scope it.

Do **not** read prompts, edit files, or reason about the project until `engrams prime` has run and its output has been read.