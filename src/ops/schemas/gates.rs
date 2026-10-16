//! Shared gate thresholds, identity metrics, and territory claims (spec 0002/0003/0005).

use anyhow::Result;
use rusqlite::Connection;

/// Launch-default gate thresholds (spec 0002 Architecture step 4). Sweep
/// constants owned by the phase-8 dogfood gate; the reward floor of zero
/// exists because `retrieval_surfaces` ships empty and any higher floor
/// would block all formation until telemetry exists.
pub(crate) const DENSITY_GATE: f64 = 0.5;
pub(crate) const STABILITY_GATE: i64 = 3;
pub(crate) const REWARD_GATE: i64 = 0;
/// Jaccard threshold above which a cluster and a staged row share identity.
pub(crate) const JACCARD_IDENTITY: f64 = 0.7;

#[inline]
pub(crate) fn gates_pass(density: f64, stability: i64, reward: i64) -> bool {
    density >= DENSITY_GATE && stability >= STABILITY_GATE && reward >= REWARD_GATE
}

pub(crate) fn failed_gates(density: f64, stability: i64, reward: i64) -> Vec<&'static str> {
    let mut failed = Vec::with_capacity(3);
    if density < DENSITY_GATE {
        failed.push("density");
    }
    if stability < STABILITY_GATE {
        failed.push("stability");
    }
    if reward < REWARD_GATE {
        failed.push("reward");
    }
    failed
}

/// Jaccard similarity of two ascending-sorted member sets (merge
/// intersection). Sortedness is a precondition enforced by every caller —
/// staging sorts member strings, stored members round-trip that order, and
/// `BTreeSet` iteration is ordered.
pub(crate) fn jaccard(a: &[&str], b: &[&str]) -> f64 {
    let (mut i, mut j, mut inter) = (0usize, 0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// Territory checks: a candidate whose members sit inside — or fully
/// cover — an existing schema's knowledge members IS that schema,
/// not a new concept; both orientations dilute or tighten past the
/// J >= 0.7 identity rule, so they are pinned explicitly here.
/// Drifted/partial shapes (some overlap, some not) stay with Jaccard.
/// Claimed territory frees only via prune/archive — the v0.14
/// adaptation owns the retire path (decision 76); nothing in the
/// confirm covenant un-claims it.
/// NOTE: Strict subset rejection (Item 3) is intentionally active in v0.13.0
/// to prevent duplicate promotion; hierarchical sub-schemas (permitting
/// child concepts inside parent territories) remain pending for v0.14.0.
///
/// Returns `Some((schema_id, schema_name))` if `knowledge_members` (the
/// candidate's members with `schema:` refs filtered out) is a subset of a
/// confirmed schema's knowledge members, a superset of them, or matches at
/// [`JACCARD_IDENTITY`] — the Jaccard itself runs over the candidate's
/// knowledge members against the confirmed schema's full member set,
/// exactly as confirm has always scored it.
pub(crate) fn claimed_territory(
    conn: &Connection,
    knowledge_members: &[String],
) -> Result<Option<(i64, String)>> {
    let knowledge_strs: Vec<&str> = knowledge_members.iter().map(String::as_str).collect();
    for s in super::confirm::confirmed_schemas(conn)? {
        let s_knowledge: Vec<&String> = s
            .members
            .iter()
            .filter(|m| !m.starts_with("schema:"))
            .collect();
        let covered = knowledge_members
            .iter()
            .filter(|m| s_knowledge.contains(m))
            .count();
        let covers = s_knowledge
            .iter()
            .filter(|k| knowledge_members.contains(k))
            .count();
        if covered == knowledge_members.len()
            || (!s_knowledge.is_empty() && covers == s_knowledge.len())
            || jaccard(
                &knowledge_strs,
                &s.members.iter().map(String::as_str).collect::<Vec<_>>(),
            ) >= JACCARD_IDENTITY
        {
            return Ok(Some((s.id, s.name)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gates_pass_evaluation() {
        assert!(gates_pass(0.5, 3, 0));
        assert!(gates_pass(0.8, 5, 2));
        assert!(!gates_pass(0.49, 3, 0));
        assert!(!gates_pass(0.5, 2, 0));
        assert!(!gates_pass(0.5, 3, -1));

        let failed = failed_gates(0.4, 2, 0);
        assert_eq!(failed, vec!["density", "stability"]);
    }

    #[test]
    fn jaccard_matches_sorted_merge_semantics() {
        assert!((jaccard(&["a", "b"], &["b", "c"]) - 1.0 / 3.0).abs() < 1e-9);
        assert!((jaccard(&["a"], &["a"]) - 1.0).abs() < 1e-9);
        // Existing identity rule: two empty sets score 0, not 1.
        assert_eq!(jaccard(&[], &[]), 0.0);
    }
}
