//! Staleness drift: detects when an anchored file was committed *after* the
//! knowledge item documenting it.
//!
//! Trust signal for KG-first retrieval — a `stale: false` report lets an agent
//! skip the defensive file read; `stale: true` says the decision predates the
//! current shape. The `git log` walk runs lazily — only when the first drift
//! report is actually requested; commands that never ask for drift skip the
//! subprocess entirely. Commit references resolve lazily with caching.
//! Every failure mode (no git, not a repo, unknown sha) degrades to a null
//! report.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::DateTime;
use rusqlite::Connection;
use serde_json::{json, Value};

/// History depth scanned for the path→last-commit map. Newest-first, so the
/// first occurrence of a path is its last touch; deeper history is irrelevant
/// to "drifted after the decision".
const MAX_COMMITS: usize = 2000;

/// Precomputed drift context for one CLI invocation.
pub struct Drift {
    root: PathBuf,
    /// path → last-commit epoch secs; None = git unavailable. Computed on
    /// first report, not at scan time.
    map: OnceCell<Option<HashMap<String, i64>>>,
    /// commit sha → epoch secs, resolved on demand.
    sha_cache: HashMap<String, Option<i64>>,
}

impl Drift {
    /// Scan the workspace once. Fails soft: reports become null when git is
    /// missing or the root is not a repository.
    pub fn scan(root: &Path) -> Drift {
        Drift {
            root: root.to_path_buf(),
            map: OnceCell::new(),
            sha_cache: HashMap::new(),
        }
    }

    /// The commit map, walking git history on first use.
    fn commit_map(&self) -> Option<&HashMap<String, i64>> {
        let root = self.root.clone();
        self.map.get_or_init(|| git_commit_map(&root)).as_ref()
    }

    /// Drift report for one item, or null when there is no signal (no anchors,
    /// no git, no git history for any anchor).
    pub fn report(
        &mut self,
        conn: &Connection,
        item_type: &str,
        id: i64,
        base_ts: &str,
        commit_sha: Option<&str>,
    ) -> Value {
        // Base resolution (mutable sha_cache) must end before the immutable
        // map borrow below.
        let Some(base_epoch) = self.base_epoch(commit_sha, base_ts) else {
            return Value::Null;
        };
        let Some(map) = self.commit_map() else {
            return Value::Null;
        };

        let anchors = match anchor_paths(conn, item_type, id) {
            Ok(a) => a,
            Err(_) => return Value::Null,
        };
        if anchors.is_empty() {
            return Value::Null;
        }

        let mut drifted = Vec::new();
        let mut checked = 0usize;
        for path in &anchors {
            let Some(&last) = map.get(path) else {
                continue;
            };
            checked += 1;
            if last > base_epoch {
                drifted.push(json!({
                    "path": path,
                    "last_commit": iso_ts(last),
                }));
            }
        }
        if checked == 0 {
            // No anchor has git history — no basis to claim either state.
            return Value::Null;
        }

        json!({
            "stale": !drifted.is_empty(),
            "base": base_ts,
            "base_source": if commit_sha.is_some() { "commit" } else { "timestamp" },
            "checked": checked,
            "drifted_anchors": drifted,
        })
    }

    /// Epoch of the comparison base: the decision's own commit when known,
    /// else its RFC3339 timestamp. Unresolvable sha falls back to timestamp.
    fn base_epoch(&mut self, commit_sha: Option<&str>, base_ts: &str) -> Option<i64> {
        if let Some(sha) = commit_sha {
            if !sha.is_empty() {
                let entry = self
                    .sha_cache
                    .entry(sha.to_string())
                    .or_insert_with(|| commit_epoch(&self.root, sha));
                if let Some(epoch) = entry {
                    return Some(*epoch);
                }
            }
        }
        DateTime::parse_from_rfc3339(base_ts)
            .ok()
            .map(|dt| dt.timestamp())
    }
}

/// Anchors stored for an item (`item_anchors.item_type` uses the store's
/// hyphen-free forms: decision, system-pattern, progress-entry…).
fn anchor_paths(
    conn: &Connection,
    item_type: &str,
    id: i64,
) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt =
        conn.prepare("SELECT path FROM item_anchors WHERE item_type = ?1 AND item_id = ?2")?;
    let rows = stmt.query_map(rusqlite::params![item_type, id], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// One `git log` walk building path → last-commit epoch (first wins).
/// Commits are recorded with a `\x01` prefix so epoch lines can never be
/// confused with (all-digit) filenames.
fn git_commit_map(root: &Path) -> Option<HashMap<String, i64>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            &format!("-n {MAX_COMMITS}"),
            "--format=%x01%ct",
            "--name-only",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut map = HashMap::new();
    let mut ts: Option<i64> = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix('\x01') else {
            // Filename line; associate with the most recent commit epoch.
            let name = line.trim();
            if !name.is_empty() {
                if let Some(t) = ts {
                    map.entry(name.to_string()).or_insert(t);
                }
            }
            continue;
        };
        ts = rest.trim().parse().ok();
    }
    Some(map)
}

/// Epoch of a commit, or None when git rejects the sha.
fn commit_epoch(root: &Path, sha: &str) -> Option<i64> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "-1", "--format=%ct", sha])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Epoch secs → RFC3339 (UTC), for reporting drifted anchor commits.
fn iso_ts(epoch: i64) -> String {
    DateTime::from_timestamp(epoch, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}
