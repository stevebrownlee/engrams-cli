# 0006 — Workspace-Scoped Git and Safe Import

## Summary

When someone points engrams at a project, by naming its workspace or its database file, they expect every answer to come from that project. Today two things leak. First, facts that come from git are read from whatever folder the terminal happens to be in. Those facts include the commit stamped on every new decision and progress note, the change history used to relate files, staged-file checks, pull-request links, and the repository that receives the pre-commit hook. A writer who logs notes about a novel from inside another repository silently stamps that repository's commits on the novel's notes. The grouping step then treats those unrelated notes as related because they share a commit. Second, restoring an export can write links and file attachments that point at notes that do not exist. One such row breaks every later schema scan until someone finds and deletes it by hand.

This fix makes the named project authoritative. Every git lookup and every workspace file lookup runs against the project the user named, never against the terminal's folder. When the user names only a database file that does not sit in the standard place inside a project, engrams cannot know which project it belongs to. It then records no git-derived facts and refuses git-only operations with an explanation, instead of guessing. Exports now carry references to code files by path, so they restore cleanly into a fresh database, and import refuses to write any reference to a missing note, listing each one it skipped.

Users get notes stamped with the right history, hooks installed in the right repository, and restores that cannot break later scans.

## Acceptance criteria

### AC-1: Stamp new notes with the named workspace's commit

**Given** a workspace inside git repository A, and a shell whose current folder is inside a different git repository B
**When** a decision or a progress entry is logged with that workspace named explicitly
**Then** the recorded commit is the current commit of repository A
**And** no commit from repository B is recorded

### AC-2: Rebuild the graph from the named workspace's history only

**Given** a workspace inside git repository A whose files changed together across several commits, and a shell inside repository B
**When** the graph is rebuilt with that workspace named explicitly
**Then** change-history relationships exist only between files of repository A
**And** no file of repository B becomes a graph node

### AC-3: Read staged files from the named workspace's repository

**Given** repository A has one staged file and the shell is inside repository B, which has different staged files
**When** pattern checks run on staged files with the workspace inside A named explicitly
**Then** only repository A's staged file is checked

### AC-4: Treat a database in the standard layout as naming its workspace

**Given** a database file stored at the standard location inside a project folder that is a git repository
**When** a decision is logged naming only that database file, from a shell inside another repository
**Then** the recorded commit is the project's current commit

### AC-5: Withhold git-derived data when the workspace cannot be determined

**Given** a database file stored outside any project's standard location, and a shell inside a git repository
**When** a decision is logged naming only that database file
**Then** the decision is saved with no recorded commit
**And** nothing from the shell's repository is recorded

### AC-6: Refuse git-only operations when the workspace cannot be determined

**Given** a database file stored outside any project's standard location, and a shell inside a git repository
**When** a git-only operation is requested naming only that database file: a staged-file check, a diff-based coverage report, a hook installation, or resolving a pull request from its number alone
**Then** the command fails with an error that explains how to name the workspace
**And** the shell's repository is left unchanged

### AC-7: Skip change history when the workspace cannot be determined

**Given** a database file stored outside any project's standard location
**When** the graph is rebuilt or change history is ingested naming only that database file
**Then** the command succeeds without reading any git history
**And** its output states that change history was skipped because no workspace was named

### AC-8: Install hooks only into the named workspace's repository

**Given** a workspace inside repository A and a shell inside repository B
**When** rules and the pre-commit hook are installed with that workspace named explicitly
**Then** the hook is written into repository A
**And** repository B's hooks are unchanged

### AC-9: Restore code-file schema members from an export

**Given** a schema with a member that is a code file, exported from one database
**When** the export is imported into a fresh database
**Then** the schema's members include a code node with the same file path
**And** inspecting the schema shows that member with its path

### AC-10: Skip and list references to missing notes on import

**Given** an export containing a link, a file attachment, and a schema membership that each point at a note present in neither the export nor the database
**When** the export is imported
**Then** none of those three references is written
**And** the import output lists each skipped reference with the file it came from
**And** every other row in the export is imported
**And** no row already in the database is changed by a skipped reference

### AC-11: Keep schema scans working after an import that skipped references

**Given** a database produced by an import that skipped references to missing notes
**When** a schema scan runs
**Then** the scan completes and reports its candidates

## Out of scope

- Repairing databases that already contain references to missing notes; `doctor` already lists dangling links, and listing file attachments whose note is missing is a separate change
- Making schema scan tolerate dangling rows that are already in a database
- Changing workspace discovery when neither a workspace nor a database file is named; the upward search from the current folder stays as it is
- Path translation for workspaces that are a subfolder of a larger repository; git paths keep their current repository-relative form
- New import modes such as an all-or-nothing strict mode or a dry run

## Open questions

- Should a later release add a strict import mode that aborts the whole import when any reference is missing, for automated restores that must be exact?

## Architecture

**One resolved workspace per invocation.** `src/db.rs` gains a resolver that returns a `Workspace { db_path, root: Option<PathBuf> }`, built once in `src/main.rs` and passed through `ops::dispatch` (`src/ops/mod.rs:40`) to every handler that needs git or workspace files. The resolution rules are:

1. `--workspace <dir>` sets `root = dir`. The database is `--db` when both flags are given, else `<dir>/engrams/context.db`.
2. `--db <path>` or `ENGRAMS_DB` without `--workspace` sets `root` with the existing `workspace_root_from_db` (`src/db.rs:217`). That helper accepts only the unambiguous `<root>/engrams/context.db` layout; any other path yields `None`.
3. When neither is given, the existing upward discovery from the current folder runs unchanged, and its discovered folder is the root.

The current-folder fallback `db::workspace_root()` (`src/db.rs:227`) is deleted. Each of its callers takes the resolved root instead: `advise.rs:93`, `anchor.rs:339`, `brief.rs:245`, `check.rs:34`, `coverage.rs:21`, `install.rs:77` and `:171`, `graph/code.rs:46`, and `rules/mod.rs:30`.

**Git against the root.** Every function in `src/ops/git.rs` takes `root: &Path` and runs `git -C <root>` through `std::process::Command`. These are `head_sha`, `origin_base`, `staged_files`, `changed_since`, `toplevel`, and `commit_file_groups`. The process-wide `HEAD_SHA` `LazyLock` (`git.rs:15`) is removed. The workspace handle caches HEAD in a `std::sync::OnceLock` so a command spawns `git rev-parse` at most once. `drift.rs` and `coverage.rs` already pass a directory (`-C` and `current_dir`) and switch to the resolved root.

When `root` is `None`, behavior fails closed:

- Commit stamping writes NULL (`decision.rs:89`, `progress.rs:62`).
- `check`, `advise`, and `relevant` with `--staged`, `coverage --diff`, `install --hooks`, and numeric PR resolution (`pr.rs:14`) fail with one named error.
- `graph rebuild` behaves as `--no-git` and reports the skip. `graph ingest` returns its existing not-ingested shape with a new reason.
- `doctor` skips the stale-decision check and lists it as skipped.

We chose an explicit root parameter over a process-global set at startup because unit tests run many workspaces in one process, and a parameter keeps the dependency visible at every call site. We chose to fail closed rather than fall back to the current folder because the project invariant forbids current-folder reliance when a database is named explicitly.

**Code references travel by path.** `src/ops/transfer/export.rs` adds the file path wherever a reference's kind is `code`:

- schema member objects gain `path` (`export.rs:509-515`)
- manual link files gain `source_path` and/or `target_path`
- retrieval-surface rows gain `path`

Code nodes are not exported, and a fresh database numbers them differently, so an ID alone cannot survive a restore. `src/ops/transfer/import.rs` resolves a code reference that has a path through `graph::code::upsert_file` and rewrites the ID. A code reference without a path, from an older export, resolves only if that ID exists.

**Import checks references before it writes.** Import first parses every file in the export. It then builds the set of items that will exist once the import finishes: rows already in the database, plus item files that parsed. A reference row is written only when every endpoint it names is in that set. This covers:

- `item_anchors` items
- `context_links` endpoints of the entity kinds decision, system_pattern, progress_entry, custom_data, schema, and code (`pr` is external and always allowed)
- schema memberships and `schema_suggestions` items
- `retrieval_surfaces` nodes

A code endpoint that carries a path always resolves, because import creates its code node. One without a path resolves only if that ID already exists. A progress entry whose parent is missing is imported without its parent link. Every omitted reference is returned in a new `skipped_references` list, with the file it came from, the reference, and the reason. Nothing is written and then removed, so an omitted row can never displace an existing row that shares its ID; the item inserts use `INSERT OR REPLACE`. Existence checks reuse one kind-to-table map: `kind_table` in `src/ops/schemas/confirm.rs:56` moves to a shared module, and table names stay `&'static str` (rule 006). Dynamic `IN` lists borrow parameters (rule 002).

The strictness is deliberate: import stays tolerant, but nothing is omitted silently. We chose this over aborting the whole import for two reasons. Import already collects per-file errors and commits everything else (`import.rs:90-98`). An abort would also make every older export containing a code-file schema member unrestorable, including this project's own: `performance-rules` has a `code` member.

## Data model

_No data model changes._

No table changes. Export files gain path fields on code references; see API surface.

## API surface

1. **Global flags.** `--workspace <dir>` names the project root and `--db <path>` names the database file; both may be given together. `ENGRAMS_DB` behaves like `--db`.
2. **Missing-workspace error** (stderr, exit 1), returned by `check`/`advise`/`relevant --staged`, `coverage --diff`, `install --hooks`, and PR-number resolution when no root is known:
   `{"error": "<operation> needs a workspace: pass --workspace <dir> or keep the database at <dir>/engrams/context.db"}`
3. **`decision log`, `progress log`.** `commit_sha` is `null` when no root is known or the root is not a git repository.
4. **`graph rebuild`.** The output gains `"cochange_skipped": "no workspace"` when no root is known. The existing keys `rebuilt`, `edges_by_source`, and `code_nodes_pruned` are unchanged.
5. **`graph ingest`.** With no root it returns `{"ingested": false, "reason": "no workspace named", "edges_by_source": {"cochange": 0}}`, the same shape as the existing `"not a git repository"` case.
6. **`doctor`.** The output gains `"skipped_checks": ["stale_decisions: no workspace"]` when no root is known. `stale_decisions` stays an array.
7. **Export format** (backward compatible; import accepts files without the new fields). Schema member objects gain `path` for code members:
   `{"kind": "code", "id": "31975", "path": "src/ops", "origin": "manual", "source": "schema_confirm"}`
   Link files gain `source_path`/`target_path` for code endpoints. Retrieval-surface rows gain `path` for code nodes.
8. **Import output.** It gains a `skipped_references` array. The `imported` counts exclude skipped rows, and `errors` is unchanged:
   ```json
   {
     "path": "./engrams_export",
     "imported": {"links": 13, "anchors": 9, "schemas": 3},
     "skipped_references": [
       {"file": "anchors/41.md", "reference": "decision:99", "reason": "no decision 99"},
       {"file": "progress/12.md", "reference": "progress_entry:3", "reason": "parent missing; entry imported without its parent link"}
     ],
     "errors": []
   }
   ```

## Dependencies

Internal:
- `src/db.rs` discovery (`resolve_db_path`, `workspace_root_from_db`)
- `src/ops/graph/code.rs` `upsert_file`
- `src/ops/schemas/confirm.rs` `kind_table`
- Existing tests that pair a nonstandard `--db` with `.current_dir(<repo>)` must name the workspace instead, because they encode the current-folder behavior this spec removes: `tests/cli.rs` `test_pr_provenance_git_derivation` (`:1194`) and `test_doctor` (`:1495`). `tests/policy.rs` install tests can now cover `--hooks`.

External:
- None. Uses `std::process::Command`, `std::sync::OnceLock`, and the existing `rusqlite`.

## Verification strategy

Per-phase verification commands (Gate 3):

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test workspace_
cargo test import_
```

Full-suite verification (Gate 5):

```bash
cargo test
cargo build
```

Manual:
1. Create two temporary git repositories A and B with one commit each. From a shell inside B, run `engrams --workspace <A> decision log ...` and confirm the recorded commit equals A's `git rev-parse HEAD`.
2. Run `engrams --db /tmp/copy.db decision log ...` from inside B. Confirm `commit_sha` is null. Then confirm `engrams --db /tmp/copy.db check --staged` fails with the missing-workspace error.
3. Export this project's database to a temporary folder and import it into a fresh temporary workspace. Confirm `schema show performance-rules` lists the `src/ops` member with its path, and that `schema scan` completes.
4. Edit a copy of that export so one anchor file references a missing decision. Import it, confirm the anchor appears under `skipped_references`, and confirm `schema scan` completes.
