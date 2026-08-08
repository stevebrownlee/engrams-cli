---
id: 004-enrich-by-move
title: Enrich by move, not get().clone()
description: consume temporary lookup maps with remove() or mem::take when enriching result rows; never clone out of a map that dies after the loop
priority: high
always_apply: false
---

# Enrich By Move, Not get().clone()

The recurring pattern in list/prime handlers is: build a temporary `HashMap<id, Vec<String>>` of anchors or PR URLs, then fold it into result rows. If the map is dead after that loop, consume it — `map.remove(&id)` or `map.get_mut(&id)` + `std::mem::take` moves the `Vec` into the row.

Do **not** write `d.field = map.get(&id)?.clone()`: it deep-clones every `String` in every `Vec` for every row, then drops the originals. This runs per result in `decision.rs`, `pattern.rs`, and `prime.rs` — and `prime` executes at the start of every session, so the cost is paid constantly.

A `clone()` out of a map is only acceptable when the map is genuinely reused afterwards; if it is, say so in a comment.
