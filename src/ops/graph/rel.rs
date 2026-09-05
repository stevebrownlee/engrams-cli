//! Canonical relationship vocabulary and its algebraic properties.
//!
//! Unknown relationship names are preserved (free-form passthrough) and
//! treated as symmetric `relates_to`-equivalent edges by analytics.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symmetry {
    Directed,
    Symmetric,
}

#[derive(Debug)]
pub struct RelSpec {
    pub canonical: &'static str,
    pub symmetry: Symmetry,
    /// Reserved for future transitive-closure analytics (Step 5 seam).
    #[allow(dead_code)]
    pub transitive: bool,
    pub inverse: Option<&'static str>,
    /// Allowed source item types; empty = any.
    pub domain: &'static [&'static str],
    /// Allowed target item types; empty = any.
    pub range: &'static [&'static str],
    /// Source item type must equal target item type.
    pub same_type: bool,
    /// At most one incoming edge of this rel per target.
    pub functional_to: bool,
    /// Rels that must not coexist with this one on the same item pair.
    pub disjoint_with: &'static [&'static str],
}

const RELS: &[RelSpec] = &[
    RelSpec {
        canonical: "relates_to",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "depends_on",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("depended_on_by"),
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &["conflicts_with"],
    },
    RelSpec {
        canonical: "part_of",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("contains"),
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "implements",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("implemented_by"),
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "refines",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("refined_by"),
        domain: &[],
        range: &[],
        same_type: true,
        functional_to: false,
        disjoint_with: &["supersedes"],
    },
    RelSpec {
        canonical: "supersedes",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("superseded_by"),
        domain: &[],
        range: &[],
        same_type: true,
        functional_to: true,
        disjoint_with: &["refines"],
    },
    RelSpec {
        canonical: "conflicts_with",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
        domain: &[],
        range: &[],
        same_type: true,
        functional_to: false,
        disjoint_with: &["depends_on"],
    },
    RelSpec {
        canonical: "co_changes",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "anchored_to",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("anchors"),
        domain: &[],
        range: &["code"],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "implemented_in",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: None,
        domain: &[],
        range: &["pr", "commit"],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "causes",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("caused_by"),
        domain: &[],
        range: &[],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "derived_from",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("derives"),
        domain: &["system_pattern"],
        range: &["progress_entry"],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
    RelSpec {
        canonical: "member_of",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("has_member"),
        domain: &[
            "decision",
            "progress_entry",
            "system_pattern",
            "custom_data",
            "schema",
            "code",
        ],
        range: &["schema"],
        same_type: false,
        functional_to: false,
        disjoint_with: &[],
    },
];

/// Look up a canonical relationship spec by its canonical name.
pub fn lookup(name: &str) -> Option<&'static RelSpec> {
    RELS.iter().find(|r| r.canonical == name)
}

/// Normalize a relationship name to `(canonical, swap)`.
///
/// - canonical name → `(name, false)`
/// - known inverse name → `(canonical, true)`; the caller must swap
///   source/target so the link is stored in the canonical direction
/// - unknown name → `(name, false)` passthrough
pub fn normalize(name: &str) -> (String, bool) {
    if lookup(name).is_some() {
        return (name.to_string(), false);
    }
    if let Some(spec) = RELS.iter().find(|r| r.inverse == Some(name)) {
        return (spec.canonical.to_string(), true);
    }
    (name.to_string(), false)
}

/// Unknown rels are treated as symmetric for analytics.
pub fn is_symmetric(name: &str) -> bool {
    match lookup(name) {
        Some(spec) => spec.symmetry == Symmetry::Symmetric,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_canonical_passthrough() {
        assert_eq!(normalize("depends_on"), ("depends_on".to_string(), false));
    }

    #[test]
    fn normalize_inverse_swaps() {
        assert_eq!(normalize("contains"), ("part_of".to_string(), true));
        assert_eq!(normalize("anchors"), ("anchored_to".to_string(), true));
    }

    #[test]
    fn normalize_unknown_passthrough() {
        assert_eq!(normalize("whatever"), ("whatever".to_string(), false));
    }

    #[test]
    fn symmetry_defaults() {
        assert!(is_symmetric("relates_to"));
        assert!(!is_symmetric("depends_on"));
        assert!(is_symmetric("unknown_rel"));
    }

    #[test]
    fn same_type_constraints() {
        for name in ["supersedes", "refines", "conflicts_with"] {
            assert!(
                lookup(name).unwrap().same_type,
                "{name} should be same_type"
            );
        }
        assert!(!lookup("depends_on").unwrap().same_type);
    }

    #[test]
    fn range_constraints() {
        assert_eq!(lookup("implemented_in").unwrap().range, ["pr", "commit"]);
        assert_eq!(lookup("anchored_to").unwrap().range, ["code"]);
        assert!(lookup("depends_on").unwrap().range.is_empty());
    }

    #[test]
    fn functional_to_constraints() {
        assert!(lookup("supersedes").unwrap().functional_to);
        assert!(!lookup("refines").unwrap().functional_to);
    }

    #[test]
    fn disjoint_pairs_are_mutual() {
        assert_eq!(
            lookup("depends_on").unwrap().disjoint_with,
            ["conflicts_with"]
        );
        assert_eq!(
            lookup("conflicts_with").unwrap().disjoint_with,
            ["depends_on"]
        );
        assert_eq!(lookup("supersedes").unwrap().disjoint_with, ["refines"]);
        assert_eq!(lookup("refines").unwrap().disjoint_with, ["supersedes"]);
        for spec in RELS {
            for partner in spec.disjoint_with {
                let partner_spec =
                    lookup(partner).unwrap_or_else(|| panic!("{partner} not canonical"));
                assert!(
                    partner_spec.disjoint_with.contains(&spec.canonical),
                    "{} disjoint with {} but not vice versa",
                    spec.canonical,
                    partner
                );
            }
        }
    }

    #[test]
    fn unrestricted_rels_have_empty_domain_range() {
        for name in [
            "relates_to",
            "depends_on",
            "part_of",
            "implements",
            "co_changes",
        ] {
            let spec = lookup(name).unwrap();
            assert!(spec.domain.is_empty(), "{name} domain should be empty");
            assert!(spec.range.is_empty(), "{name} range should be empty");
        }
    }

    #[test]
    fn causes_spec_and_normalization() {
        let causes = lookup("causes").unwrap();
        assert!(causes.transitive);
        assert!(!causes.same_type);
        assert!(!causes.functional_to);
        assert!(causes.domain.is_empty() && causes.range.is_empty());
        assert_eq!(causes.inverse, Some("caused_by"));
        assert_eq!(normalize("caused_by"), ("causes".to_string(), true));
        assert_eq!(normalize("causes"), ("causes".to_string(), false));
    }

    #[test]
    fn derived_from_domain_range() {
        let spec = lookup("derived_from").unwrap();
        assert!(!spec.transitive);
        assert_eq!(spec.domain, ["system_pattern"]);
        assert_eq!(spec.range, ["progress_entry"]);
        assert_eq!(spec.inverse, Some("derives"));
        assert_eq!(normalize("derives"), ("derived_from".to_string(), true));
    }

    #[test]
    fn member_of_spec_and_normalization() {
        let spec = lookup("member_of").unwrap();
        assert!(!spec.transitive);
        assert!(!spec.same_type);
        assert!(!spec.functional_to);
        assert_eq!(
            spec.domain,
            [
                "decision",
                "progress_entry",
                "system_pattern",
                "custom_data",
                "schema",
                "code"
            ]
        );
        assert_eq!(spec.range, ["schema"]);
        assert_eq!(spec.inverse, Some("has_member"));
        assert_eq!(normalize("has_member"), ("member_of".to_string(), true));
        assert_eq!(normalize("member_of"), ("member_of".to_string(), false));
    }
}
