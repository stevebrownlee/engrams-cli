use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Single source of truth for the decisions row projection. SELECT lists must
/// embed this (or `decision_cols_qualified()`) so column order can never drift
/// from the name-based row parsers.
pub(crate) const DECISION_COLS: &str = "id, uuid, summary, rationale, implementation_details, tags, timestamp, status, commit_sha, importance, access_count, last_accessed_at, archived, contract";

/// `DECISION_COLS` qualified with the `d` table alias, built without a
/// throwaway Vec.
pub(crate) fn decision_cols_qualified() -> String {
    let mut out = String::with_capacity(DECISION_COLS.len() + 14 * 2);
    for (i, col) in DECISION_COLS.split(", ").enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str("d.");
        out.push_str(col);
    }
    out
}
#[derive(Serialize)]
pub struct Decision {
    pub id: i64,
    pub uuid: String,
    pub summary: String,
    pub rationale: Option<String>,
    pub implementation_details: Option<String>,
    pub tags: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "is_active")]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pr_urls: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(skip_serializing_if = "is_default_importance")]
    pub importance: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub access_count: i64,
    #[serde(skip_serializing_if = "is_zero")]
    pub archived: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Serialize)]
pub struct Progress {
    pub id: i64,
    pub timestamp: String,
    pub status: String,
    pub description: String,
    pub parent_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
}

#[derive(Serialize)]
pub struct Pattern {
    pub id: i64,
    pub uuid: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Value>,
    pub timestamp: String,
    /// Machine-checkable expression kind: "regex" | "ast" (NULL = prose-only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_kind: Option<String>,
    /// The check expression (regex source or ast-grep pattern); NULL when prose-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_expr: Option<String>,
    /// Enforcement severity: "info" | "warn" | "error".
    pub severity: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pr_urls: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
    #[serde(skip_serializing_if = "is_default_importance")]
    pub importance: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub access_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(skip_serializing_if = "is_zero")]
    pub archived: i64,
    /// Stored consolidation confidence in (0, 1]; 1.0 = full trust.
    pub confidence: f64,
    /// When confidence was last confirmed; NULL = treat creation timestamp as anchor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed_at: Option<String>,
    /// Read-time confidence: stored value decayed from its last confirmation.
    pub effective_confidence: f64,
}

#[derive(Serialize)]
pub struct CustomData {
    pub id: i64,
    pub category: String,
    pub key: String,
    pub value: Value,
    pub timestamp: String,
}

fn default_link_origin() -> String {
    "manual".to_string()
}

fn default_link_weight() -> f64 {
    1.0
}

/// Direction of a link relative to a queried item, reported by `link list`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    #[serde(rename = "outgoing")]
    Outgoing,
    #[serde(rename = "incoming")]
    Incoming,
}

#[derive(Serialize, Deserialize)]
pub struct Link {
    pub id: i64,
    pub source_item_type: String,
    pub source_item_id: String,
    pub target_item_type: String,
    pub target_item_id: String,
    pub relationship_type: String,
    #[serde(default)]
    pub description: Option<String>,
    pub timestamp: String,
    #[serde(default = "default_link_origin")]
    pub origin: String,
    #[serde(default = "default_link_weight")]
    pub weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
}

#[derive(Serialize)]
pub struct ContextDoc {
    pub content: Value,
    pub version: i64,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct HistoryRow {
    pub version: i64,
    pub content: Value,
    pub timestamp: String,
    pub change_source: Option<String>,
}

fn is_active(s: &String) -> bool {
    s == "active"
}

fn is_default_importance(n: &i64) -> bool {
    *n == 5
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}
