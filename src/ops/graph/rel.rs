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
}

const RELS: &[RelSpec] = &[
    RelSpec {
        canonical: "relates_to",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
    },
    RelSpec {
        canonical: "depends_on",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("depended_on_by"),
    },
    RelSpec {
        canonical: "part_of",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("contains"),
    },
    RelSpec {
        canonical: "implements",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("implemented_by"),
    },
    RelSpec {
        canonical: "refines",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("refined_by"),
    },
    RelSpec {
        canonical: "supersedes",
        symmetry: Symmetry::Directed,
        transitive: true,
        inverse: Some("superseded_by"),
    },
    RelSpec {
        canonical: "conflicts_with",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
    },
    RelSpec {
        canonical: "co_changes",
        symmetry: Symmetry::Symmetric,
        transitive: false,
        inverse: None,
    },
    RelSpec {
        canonical: "anchored_to",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: Some("anchors"),
    },
    RelSpec {
        canonical: "implemented_in",
        symmetry: Symmetry::Directed,
        transitive: false,
        inverse: None,
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
}
