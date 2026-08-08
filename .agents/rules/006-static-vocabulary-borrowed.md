---
id: 006-static-vocabulary-borrowed
title: Static vocabulary stays borrowed
description: canonical vocabularies return &'static str or Cow<str>, never String; a .to_string() on a static means the signature is wrong
priority: medium
always_apply: false
---

# Static Vocabulary Stays Borrowed

Canonical vocabularies — `RELS` in `src/ops/graph/rel.rs`, the status lists in `src/ops/status.rs` — are `&'static str`. Functions returning them must return `&'static str` or `Cow<str>`, never `String`.

Calling `.to_string()` on a literal or static to satisfy a signature means the signature is wrong: widen it to `&str`/`Cow` instead. `rel::normalize()` today heap-allocates on every `link add`, including when the input is already canonical and when returning a `&'static` canonical name; `Cow<'_, str>` makes both static cases free and allocates only for unknown-name passthrough.

Serde field defaults like `"manual".to_string()` in `models.rs` are exempt — an owned `String` field genuinely requires one.
