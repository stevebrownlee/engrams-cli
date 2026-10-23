---
description: Cut and publish a new release of engrams-cli — bump to the next minor version (or an explicit version), publish release builds and GitHub Release notes, update Homebrew formula checksums, and update the documentation website.
---

# Release Command

Load and follow `.agents/skills/release/SKILL.md` end-to-end.

## Arguments

- `$ARGUMENTS` — optional target version (e.g. `0.16.0` or `0.15.1`).
  - If omitted or empty: bump `[package].version` in `Cargo.toml` to the **next available minor version** (`X.Y.Z` -> `X.(Y+1).0`).
  - If provided: strip any leading `v` and use that exact `MAJOR.MINOR.PATCH` version.
