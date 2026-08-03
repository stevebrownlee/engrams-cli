use anyhow::Result;

/// Canonical status vocabulary for progress entries.
pub const PROGRESS_STATUSES: &[&str] = &[
    "Todo",
    "InProgress",
    "InReview",
    "Blocked",
    "Done",
    "Dropped",
];

/// Canonical status vocabulary for decisions.
pub const DECISION_STATUSES: &[&str] = &["active", "superseded", "rejected", "revisited"];

/// Validate `status` against `valid` for the given item kind.
///
/// Returns `Ok(true)` when `force` overrode a non-canonical value (caller adds
/// `"overrides": ["status_vocabulary"]` to the result JSON), `Ok(false)` when
/// the value is canonical, and bails otherwise.
pub fn check(status: &str, valid: &[&str], force: bool, item_kind: &str) -> Result<bool> {
    if valid.contains(&status) {
        return Ok(false);
    }
    if force {
        return Ok(true);
    }
    anyhow::bail!(
        "status '{}' violates status_vocabulary for {}: valid values are [{}] (use --force to override)",
        status,
        item_kind,
        valid.join(", ")
    );
}
