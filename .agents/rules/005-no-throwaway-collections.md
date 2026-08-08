---
id: 005-no-throwaway-collections
title: No throwaway intermediate collections
description: never materialize a collection only to consume it immediately; fuse iterators or pre-size with with_capacity
priority: low
always_apply: false
---

# No Throwaway Intermediate Collections

Never build a `Vec` solely to consume it in the same expression. `tags.iter().map(|_| "?").collect::<Vec<_>>().join(",")` allocates N one-character `String`s plus a `Vec`, all discarded immediately — a `String::with_capacity(2 * n - 1)` and a `push_str` loop does the same with one allocation.

Fuse iterator chains instead of collecting between stages, and when the final length is computable from an input collection, pre-size with `Vec::with_capacity(n)` / `String::with_capacity(n)`. Working examples already in the codebase: `prime.rs` (track summaries) and `export.rs` (JSON string escaping).

Note `collect()` from a sized iterator already reserves via `size_hint` — this rule targets intermediates that exist only to be joined, folded, or re-iterated, not terminal collects.
