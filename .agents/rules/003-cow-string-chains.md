---
id: 003-cow-string-chains
title: One allocation per string chain
description: keep Cow<str> borrowed through string-transform chains; allocate once at the ownership boundary
priority: medium
always_apply: false
---

# One Allocation Per String Chain

`String::from_utf8_lossy()` returns `Cow<str>`, which borrows when the input is valid UTF-8 — the common case for git and CLI output. Keep that `Cow` borrowed through `trim()`, `lines()`, and `map()` chains, and allocate exactly once at the point ownership is actually required.

Do **not** write `String::from_utf8_lossy(&out.stdout).trim().to_string()`: the lossy conversion may borrow for free, and the trailing `.to_string()` forces an allocation anyway, paying for a string twice. The same applies to slice-then-own forms like `s[prefix.len()..].to_string()` — return or pass `Cow`/`&str` instead.

The worst offender today is the git boundary (`src/ops/git.rs`); treat it as the reference for both the problem and the fix.
