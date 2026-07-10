use serde::Serialize;
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
}

#[derive(Serialize)]
pub struct Progress {
    pub id: i64,
    pub timestamp: String,
    pub status: String,
    pub description: String,
    pub parent_id: Option<i64>,
}

#[derive(Serialize)]
pub struct Pattern {
    pub id: i64,
    pub uuid: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Option<Value>,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct CustomData {
    pub id: i64,
    pub category: String,
    pub key: String,
    pub value: Value,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct Link {
    pub id: i64,
    pub source_item_type: String,
    pub source_item_id: String,
    pub target_item_type: String,
    pub target_item_id: String,
    pub relationship_type: String,
    pub description: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
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
