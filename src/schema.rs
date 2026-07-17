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
  implementation_details TEXT, tags TEXT,
  status TEXT NOT NULL DEFAULT 'active',
  commit_sha TEXT
);
CREATE TABLE IF NOT EXISTS progress_entries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL, status TEXT NOT NULL, description TEXT NOT NULL,
  parent_id INTEGER REFERENCES progress_entries(id) ON DELETE SET NULL,
  commit_sha TEXT
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
  relationship_type TEXT NOT NULL, description TEXT, timestamp TEXT NOT NULL,
  origin TEXT NOT NULL DEFAULT 'manual', source TEXT, weight REAL NOT NULL DEFAULT 1.0
);
CREATE INDEX IF NOT EXISTS ix_links_source ON context_links(source_item_type, source_item_id);
CREATE INDEX IF NOT EXISTS ix_links_target ON context_links(target_item_type, target_item_id);
CREATE TABLE IF NOT EXISTS code_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  symbol TEXT NOT NULL DEFAULT '',
  first_seen TEXT NOT NULL,
  last_seen TEXT NOT NULL,
  UNIQUE(kind, path, symbol)
);
CREATE INDEX IF NOT EXISTS ix_code_nodes_path ON code_nodes(path);
CREATE TABLE IF NOT EXISTS graph_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_rebuild_at TEXT,
  last_ingest_sha TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS ix_links_derived_uniq
  ON context_links(source_item_type, source_item_id, target_item_type, target_item_id, relationship_type)
  WHERE origin = 'derived';

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
CREATE TABLE IF NOT EXISTS item_anchors (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_type TEXT NOT NULL,
  item_id INTEGER NOT NULL,
  path TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  UNIQUE(item_type, item_id, path)
);
CREATE INDEX IF NOT EXISTS ix_anchors_path ON item_anchors(path);

CREATE VIRTUAL TABLE IF NOT EXISTS system_patterns_fts USING fts5(
  name, description, tags, content='system_patterns', content_rowid='id'
);
CREATE TRIGGER IF NOT EXISTS system_patterns_ai AFTER INSERT ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(rowid, name, description, tags)
  VALUES (new.id, new.name, new.description, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS system_patterns_ad AFTER DELETE ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(system_patterns_fts, rowid, name, description, tags)
  VALUES ('delete', old.id, old.name, old.description, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS system_patterns_au AFTER UPDATE ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(system_patterns_fts, rowid, name, description, tags)
  VALUES ('delete', old.id, old.name, old.description, old.tags);
  INSERT INTO system_patterns_fts(rowid, name, description, tags)
  VALUES (new.id, new.name, new.description, new.tags);
END;
"#;
pub const MIGRATION_V2: &str = r#"
ALTER TABLE decisions ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE decisions ADD COLUMN commit_sha TEXT;
ALTER TABLE progress_entries ADD COLUMN commit_sha TEXT;
CREATE TABLE IF NOT EXISTS item_anchors (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  item_type TEXT NOT NULL,
  item_id INTEGER NOT NULL,
  path TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  UNIQUE(item_type, item_id, path)
);
CREATE INDEX IF NOT EXISTS ix_anchors_path ON item_anchors(path);
CREATE VIRTUAL TABLE IF NOT EXISTS system_patterns_fts USING fts5(
  name, description, tags, content='system_patterns', content_rowid='id'
);
INSERT INTO system_patterns_fts(rowid, name, description, tags)
  SELECT id, name, description, tags FROM system_patterns;
CREATE TRIGGER IF NOT EXISTS system_patterns_ai AFTER INSERT ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(rowid, name, description, tags)
  VALUES (new.id, new.name, new.description, new.tags);
END;
CREATE TRIGGER IF NOT EXISTS system_patterns_ad AFTER DELETE ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(system_patterns_fts, rowid, name, description, tags)
  VALUES ('delete', old.id, old.name, old.description, old.tags);
END;
CREATE TRIGGER IF NOT EXISTS system_patterns_au AFTER UPDATE ON system_patterns BEGIN
  INSERT INTO system_patterns_fts(system_patterns_fts, rowid, name, description, tags)
  VALUES ('delete', old.id, old.name, old.description, old.tags);
  INSERT INTO system_patterns_fts(rowid, name, description, tags)
  VALUES (new.id, new.name, new.description, new.tags);
END;
"#;
pub const MIGRATION_V3: &str = r#"
ALTER TABLE context_links ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE context_links ADD COLUMN source TEXT;
ALTER TABLE context_links ADD COLUMN weight REAL NOT NULL DEFAULT 1.0;
CREATE TABLE IF NOT EXISTS code_nodes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  symbol TEXT NOT NULL DEFAULT '',
  first_seen TEXT NOT NULL,
  last_seen TEXT NOT NULL,
  UNIQUE(kind, path, symbol)
);
CREATE INDEX IF NOT EXISTS ix_code_nodes_path ON code_nodes(path);
CREATE TABLE IF NOT EXISTS graph_meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_rebuild_at TEXT,
  last_ingest_sha TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS ix_links_derived_uniq
  ON context_links(source_item_type, source_item_id, target_item_type, target_item_id, relationship_type)
  WHERE origin = 'derived';
"#;
