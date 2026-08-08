use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct Decision {
    pub id: i64,
    pub uuid: String,
    pub summary: String,
    pub rationale: Option<String>,
    pub implementation_details: Option<String>,
    pub tags: Option<Value>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "is_active")]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pr_urls: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<String>,
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
