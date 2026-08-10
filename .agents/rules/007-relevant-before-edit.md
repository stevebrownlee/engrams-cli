---
id: 007-relevant-before-edit
title: Query relevant context before editing source files
description: Before modifying any source file, run engrams relevant for those paths to surface anchored decisions and patterns
priority: critical
always_apply: true
---

# Query Relevant Context Before Editing

Before modifying any source file under `src/`, `tests/`, or `docs/`, run `engrams relevant <paths>` (or `engrams relevant --staged` for git-added files). Treat the output as **required reading** alongside the file itself.

This surfaces prior decisions, system patterns, and graph links anchored to those files — constraints and conventions that reading the source code alone will not reveal. Skipping it risks duplicating an existing convention, violating a recorded constraint, or re-litigating a settled design choice.

Run it once per batch of related edits, not once per file. If the output is empty, proceed with confidence that no anchored context exists for those paths.
