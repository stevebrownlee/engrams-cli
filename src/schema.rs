pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS product_context (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  content TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS active_context (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  content TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS product_context_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  version INTEGER NOT NULL, content TEXT NOT NULL,
  timestamp TEXT NOT NULL, change_source TEXT
);
CREATE TABLE IF NOT EXISTS active_context_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  version INTEGER NOT NULL, content TEXT NOT NULL,
  timestamp TEXT NOT NULL, change_source TEXT
);
CREATE TABLE IF NOT EXISTS decisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
  summary TEXT NOT NULL, rationale TEXT,
  implementation_details TEXT, tags TEXT
);
CREATE TABLE IF NOT EXISTS progress_entries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL, status TEXT NOT NULL, description TEXT NOT NULL,
  parent_id INTEGER REFERENCES progress_entries(id) ON DELETE SET NULL
);
CREATE TABLE IF NOT EXISTS system_patterns (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT UNIQUE NOT NULL, timestamp TEXT NOT NULL,
  name TEXT UNIQUE NOT NULL, description TEXT, tags TEXT
);
CREATE TABLE IF NOT EXISTS custom_data (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL, category TEXT NOT NULL, key TEXT NOT NULL,
  value TEXT NOT NULL, UNIQUE(category, key)
);
CREATE TABLE IF NOT EXISTS context_links (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_item_type TEXT NOT NULL, source_item_id TEXT NOT NULL,
  target_item_type TEXT NOT NULL, target_item_id TEXT NOT NULL,
  relationship_type TEXT NOT NULL, description TEXT, timestamp TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_links_source ON context_links(source_item_type, source_item_id);
CREATE INDEX IF NOT EXISTS ix_links_target ON context_links(target_item_type, target_item_id);

CREATE VIRTUAL TABLE IF NOT EXISTS decisions_fts USING fts5(
  summary, rationale, implementation_details, tags,
  content='decisions', content_rowid='id'
);
CREATE TRIGGER IF NOT EXISTS decisions_ai AFTER INSERT ON decisions BEGIN
  INSERT INTO decisions_fts(rowid, summary, rationale, implementation_details, tags)
  VALUES (new.id, new.summary, new.rationale, new.implementation_details, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS decisions_ad AFTER DELETE ON decisions BEGIN
  INSERT INTO decisions_fts(decisions_fts, rowid, summary, rationale, implementation_details, tags)
  VALUES ('delete', old.id, old.summary, old.rationale, old.implementation_details, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS decisions_au AFTER UPDATE ON decisions BEGIN
  INSERT INTO decisions_fts(decisions_fts, rowid, summary, rationale, implementation_details, tags)
  VALUES ('delete', old.id, old.summary, old.rationale, old.implementation_details, old.tags);
  INSERT INTO decisions_fts(rowid, summary, rationale, implementation_details, tags)
  VALUES (new.id, new.summary, new.rationale, new.implementation_details, new.tags);
END;

CREATE VIRTUAL TABLE IF NOT EXISTS custom_data_fts USING fts5(
  category, key, value, content='custom_data', content_rowid='id'
);
CREATE TRIGGER IF NOT EXISTS custom_data_ai AFTER INSERT ON custom_data BEGIN
  INSERT INTO custom_data_fts(rowid, category, key, value)
  VALUES (new.id, new.category, new.key, new.value);
END;
CREATE TRIGGER IF NOT EXISTS custom_data_ad AFTER DELETE ON custom_data BEGIN
  INSERT INTO custom_data_fts(custom_data_fts, rowid, category, key, value)
  VALUES ('delete', old.id, old.category, old.key, old.value);
END;
CREATE TRIGGER IF NOT EXISTS custom_data_au AFTER UPDATE ON custom_data BEGIN
  INSERT INTO custom_data_fts(custom_data_fts, rowid, category, key, value)
  VALUES ('delete', old.id, old.category, old.key, old.value);
  INSERT INTO custom_data_fts(rowid, category, key, value)
  VALUES (new.id, new.category, new.key, new.value);
END;
"#;
