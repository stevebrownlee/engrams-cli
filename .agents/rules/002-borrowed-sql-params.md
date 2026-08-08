---
id: 002-borrowed-sql-params
title: Borrowed SQL parameters only
description: dynamic SQL parameter lists must borrow via Vec<&dyn ToSql>; boxing cloned parameters is prohibited
priority: high
always_apply: false
---

# Borrowed SQL Parameters Only

When building a dynamic SQL parameter list, borrow from local bindings with `Vec<&dyn rusqlite::ToSql>`. Do **not** heap-allocate parameters with `Vec<Box<dyn rusqlite::ToSql>>` and `Box::new(value.clone())` — that is two allocations (the clone, then the box) for every parameter of every query.

The reference implementation is `query_relevant_ids` in `src/ops/anchor.rs`: a `Vec<&dyn ToSql>` pushing `&path` borrows, passed via `rusqlite::params_from_iter`.

This applies anywhere the parameter count is dynamic (`src/ops/query.rs`, `src/ops/custom.rs`, `src/ops/decision.rs`, `src/ops/progress.rs`). Static parameter lists keep using the `params![]` macro.
