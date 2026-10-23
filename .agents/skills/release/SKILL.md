---
name: release
description: Use when cutting a new release of engrams-cli, bumping the version, publishing release builds and GitHub releases, updating Homebrew formula checksums, or adding release notes to the documentation website.
---

# Release

Cut and publish a new release of `engrams-cli`: resolve the target version (defaulting to the next minor version), update all version surfaces and documentation release notes, verify the build and test suite, tag and push to trigger the cross-platform release workflow, update the GitHub release notes and Homebrew formula `sha256` checksums, and close out in Engrams.

## Usage

```
/release                  (bumps to the next available minor version: X.Y.Z -> X.(Y+1).0)
/release <version>        (uses the explicit version, e.g. 0.15.1 or 1.0.0; strip any leading 'v')
```

## Version Resolution Rule (Mandatory)

1. Read the current `[package].version` in `Cargo.toml` and check `git tag -l "v*"` so you never collide with an existing tag.
2. **If no version argument is provided:** ALWAYS bump to the **next available minor version** (`X.(Y+1).0`, resetting patch to `0` — e.g. `0.15.0` -> `0.16.0`). Never guess patch vs. minor when no argument is given.
3. **If a specific version argument is provided:** Strip any leading `v`, validate `MAJOR.MINOR.PATCH` semver format, and verify tag `v<VERSION>` does not already exist.

## Files Updated in Every Release

| File | What to update |
|---|---|
| `Cargo.toml` | `[package].version = "<VERSION>"` |
| `Cargo.lock` | Updated automatically by `cargo check` / `cargo build` after editing `Cargo.toml` |
| `Formula/engrams.rb` | `version "<VERSION>"` before tagging; the four target `sha256` hashes after CI publishes the release tarballs |
| `docs/package.json` & `docs/package-lock.json` | `"version": "<VERSION>"` (`npm --prefix docs install --package-lock-only` syncs the lockfile; `docs/astro.config.mjs` reads `Cargo.toml` for `__APP_VERSION__` automatically) |
| `docs/src/pages/index.astro` | Top-bar version badge `<span class="version">v<VERSION></span>` |
| `docs/src/pages/docs/reference/releases.astro` | Prepend a new `<h2>v<VERSION></h2>` `<div class="release-block">` section above the previous release with `<h3>What was added</h3>` (or `<h3>What was fixed</h3>`) and `<h3>Why it was done</h3>` |

## Step-by-Step Procedure

### 1. Pre-flight & Synthesis
1. Run `git status --porcelain` and confirm the working tree is clean and on `main` (`git branch --show-current`). Refuse if dirty or not on `main`.
2. Resolve `<VERSION>` (e.g. `0.16.0`) and `<TAG>` (`v0.16.0`) per the Version Resolution Rule above.
3. Inspect changes since the previous release tag (`git log <PREV_TAG>..HEAD --oneline` and `engrams decision list` / `engrams progress list`) to draft:
   - A concise release theme title (`<TITLE>`, e.g. `Schema Prompts & Reviewed Proposals`)
   - The HTML release block for `docs/src/pages/docs/reference/releases.astro` (`<h3>What was added</h3>` bullet list + `<h3>Why it was done</h3>` paragraph)
   - Matching Markdown release notes for the GitHub Release body

### 2. Bump Version Surfaces & Update Docs Website
1. Edit `Cargo.toml`, `Formula/engrams.rb` (`version` line), `docs/package.json`, `docs/src/pages/index.astro` (`.version` badge), and `docs/src/pages/docs/reference/releases.astro`.
2. Run verification gates before committing:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets
   cargo test
   cargo build --release
   npm --prefix docs install --package-lock-only
   npm --prefix docs run build
   ```
   (Restore `docs/.astro/settings.json` if touched by `astro build`.)

### 3. Commit Version Bump, Tag, and Trigger Build Publish
1. Stage the version and documentation files and run `engrams check --staged`:
   ```bash
   git add Cargo.toml Cargo.lock Formula/engrams.rb docs/package.json docs/package-lock.json docs/src/pages/index.astro docs/src/pages/docs/reference/releases.astro
   ./target/debug/engrams check --staged
   git commit -m "Bump version to <VERSION> — <TITLE>"
   ```
2. Create the annotated tag and push both `main` and `<TAG>` to `origin`:
   ```bash
   git tag -a v<VERSION> -m "v<VERSION> — <TITLE>"
   git push origin main
   git push origin v<VERSION>
   ```
   Pushing `main` with `docs/**` changes triggers `.github/workflows/publish.yml` (deploys the updated documentation website to GitHub Pages). Pushing `v<VERSION>` triggers `.github/workflows/release.yml` (builds the 4 release binaries and creates the GitHub Release).

### 4. Publish Release Builds, Update GitHub Release Notes, & Update Homebrew Formula
1. Wait for the `Release` workflow (`release.yml`) on tag `v<VERSION>` to finish building and uploading all 4 target tarballs:
   ```bash
   gh run list --workflow=release.yml --branch v<VERSION> --limit 1
   gh run watch <RUN_ID> --exit-status
   ```
2. Write the Markdown release notes to `/tmp/engrams-release-notes.md` (never pass multi-line Markdown with backticks inline to `--notes`) and update the GitHub Release:
   ```bash
   gh release edit v<VERSION> --title "v<VERSION> — <TITLE>" --notes-file /tmp/engrams-release-notes.md
   rm -f /tmp/engrams-release-notes.md
   ```
3. Download the 4 release tarballs to a temporary directory and compute their `sha256` checksums:
   ```bash
   mkdir -p /tmp/engrams-release-v<VERSION>
   gh release download v<VERSION> --dir /tmp/engrams-release-v<VERSION> --clobber
   shasum -a 256 /tmp/engrams-release-v<VERSION>/*.tar.gz
   rm -rf /tmp/engrams-release-v<VERSION>
   ```
4. Update the four `sha256` lines in `Formula/engrams.rb` (`x86_64-apple-darwin`, `aarch64-apple-darwin`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`), then commit and push:
   ```bash
   git add Formula/engrams.rb
   git commit -m "chore: update Homebrew Formula with verified sha256 checksums for v<VERSION>"
   git push origin main
   ```

### 5. Record Release in Engrams & Export
1. Log the release decision, progress entry, active-context update, and export:
   ```bash
   ./target/debug/engrams decision log \
     --summary "Released v<VERSION> — <TITLE>" \
     --rationale "<1-2 sentence summary of what shipped in v<VERSION>>" \
     --tags release,v<VERSION> \
     --anchor Cargo.toml \
     --anchor docs/src/pages/docs/reference/releases.astro
   ./target/debug/engrams progress log --status Done --description "Released v<VERSION> — <TITLE>"
   ./target/debug/engrams active-context update --patch '{"last_release": "v<VERSION>"}'
   ./target/debug/engrams export
   git add engrams_export
   git commit -m "docs(engrams): export v<VERSION> release session close"
   git push origin main
   ```

## Common Mistakes to Avoid

| Mistake | Fix |
|---|---|
| Guessing a patch bump (`0.15.1`) when no version argument is passed | Always bump to the next minor version (`X.(Y+1).0`) unless an explicit version is provided. |
| Forgetting `docs/src/pages/index.astro` `.version` badge | Update `<span class="version">v<VERSION></span>` alongside `Cargo.toml`, `Formula/engrams.rb`, `docs/package.json`, and `releases.astro`. |
| Leaving GitHub Release notes as the auto-generated PR one-liner | Always run `gh release edit v<VERSION> --title ... --notes-file ...` with the full structured release notes. |
| Leaving stale `sha256` hashes in `Formula/engrams.rb` | Download the 4 tarballs produced by `release.yml`, compute `shasum -a 256`, and commit `Formula/engrams.rb`. |
| Passing backticks inline to `gh release edit --notes "..."` | Always write notes to a temp file and pass `--notes-file`. |
