-- EndlessLi initial schema (Phase 1).
-- SQLite is the source of truth. All timestamps are Unix epoch integers.
-- Array fields from the data model are join tables (note_tags,
-- library_item_tags, session_notes). Bidirectional note links are stored
-- once with a canonical ordering (note_a < note_b).

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  icon TEXT,
  color TEXT,
  system_prompt TEXT,
  encrypted INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE TABLE folders (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES folders(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL
);

CREATE TABLE tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  color TEXT
);

CREATE TABLE research_sessions (
  id TEXT PRIMARY KEY,
  query TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('light','deep')),
  status TEXT NOT NULL CHECK (status IN ('pending','in_progress','completed','failed','cancelled')),
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
  context_memory TEXT,
  tokens_in INTEGER NOT NULL DEFAULT 0,
  tokens_out INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  completed_at INTEGER
);

CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content TEXT NOT NULL DEFAULT '',
  project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
  folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
  author TEXT NOT NULL CHECK (author IN ('user','ai','import')),
  source_session_id TEXT REFERENCES research_sessions(id) ON DELETE SET NULL,
  created_from_note_id TEXT REFERENCES notes(id) ON DELETE SET NULL,
  embedding_model TEXT,
  embedding_hash TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE note_tags (
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (note_id, tag_id)
);

-- Bidirectional links stored once with a canonical ordering (a < b).
CREATE TABLE note_links (
  note_a TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  note_b TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  similarity REAL,
  origin TEXT NOT NULL CHECK (origin IN ('manual','auto_approved')),
  created_at INTEGER NOT NULL,
  PRIMARY KEY (note_a, note_b),
  CHECK (note_a < note_b)
);

-- Rejected suggestions, so they are not suggested again.
CREATE TABLE link_rejections (
  note_a TEXT NOT NULL, note_b TEXT NOT NULL,
  PRIMARY KEY (note_a, note_b)
);

CREATE TABLE research_steps (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
  parent_step_id TEXT REFERENCES research_steps(id) ON DELETE CASCADE,
  description TEXT NOT NULL,
  status TEXT NOT NULL,
  step_order INTEGER NOT NULL,
  details TEXT
);

CREATE TABLE research_results (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
  type TEXT NOT NULL CHECK (type IN ('summary','source','insight','citation')),
  content TEXT NOT NULL,
  source_url TEXT,
  source_title TEXT,
  result_order INTEGER NOT NULL,
  selected_for_note INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE session_notes (
  session_id TEXT NOT NULL REFERENCES research_sessions(id) ON DELETE CASCADE,
  note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  PRIMARY KEY (session_id, note_id)
);

CREATE TABLE memory_entries (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL CHECK (type IN ('global','project')),
  project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('user','ai_suggested')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER,
  is_active INTEGER NOT NULL DEFAULT 1,
  CHECK ((type = 'project') = (project_id IS NOT NULL))
);

CREATE TABLE memory_suggestions (
  id TEXT PRIMARY KEY,
  project_id TEXT,
  content TEXT NOT NULL,
  session_id TEXT,
  created_at INTEGER NOT NULL
);

CREATE TABLE library_items (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL CHECK (type IN ('pdf','article','book','video','dataset','podcast','paper')),
  title TEXT NOT NULL,
  authors TEXT,
  url TEXT,
  source TEXT,
  publication_date TEXT,
  content TEXT,
  file_path TEXT,
  note_id TEXT REFERENCES notes(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE library_item_tags (
  item_id TEXT NOT NULL REFERENCES library_items(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (item_id, tag_id)
);

CREATE TABLE skills (
  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL,
  content TEXT NOT NULL, parameters TEXT NOT NULL DEFAULT '{}', category TEXT NOT NULL,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);

CREATE TABLE scripts (
  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT NOT NULL,
  content TEXT NOT NULL, parameters TEXT NOT NULL DEFAULT '{}', category TEXT NOT NULL,
  allow_network INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);

CREATE TABLE cache_entries (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL CHECK (type IN ('search','llm_response','webpage','embedding')),
  query TEXT NOT NULL,
  response BLOB NOT NULL,
  source_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  last_accessed_at INTEGER NOT NULL,
  access_count INTEGER NOT NULL DEFAULT 1,
  expires_at INTEGER
);
CREATE INDEX idx_cache_lru ON cache_entries(last_accessed_at);

CREATE TABLE token_usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT, provider TEXT NOT NULL, model TEXT NOT NULL,
  tokens_in INTEGER NOT NULL, tokens_out INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Full-text search (external-content tables kept in sync by triggers).
CREATE VIRTUAL TABLE notes_fts USING fts5(title, content, content='notes', content_rowid='rowid');
CREATE VIRTUAL TABLE library_fts USING fts5(title, content, content='library_items', content_rowid='rowid');

CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
  INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;
CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
END;
CREATE TRIGGER notes_fts_update AFTER UPDATE ON notes BEGIN
  INSERT INTO notes_fts(notes_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, old.content);
  INSERT INTO notes_fts(rowid, title, content) VALUES (new.rowid, new.title, new.content);
END;

CREATE TRIGGER library_fts_insert AFTER INSERT ON library_items BEGIN
  INSERT INTO library_fts(rowid, title, content) VALUES (new.rowid, new.title, COALESCE(new.content, ''));
END;
CREATE TRIGGER library_fts_delete AFTER DELETE ON library_items BEGIN
  INSERT INTO library_fts(library_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, COALESCE(old.content, ''));
END;
CREATE TRIGGER library_fts_update AFTER UPDATE ON library_items BEGIN
  INSERT INTO library_fts(library_fts, rowid, title, content) VALUES ('delete', old.rowid, old.title, COALESCE(old.content, ''));
  INSERT INTO library_fts(rowid, title, content) VALUES (new.rowid, new.title, COALESCE(new.content, ''));
END;

CREATE INDEX idx_notes_project ON notes(project_id);
CREATE INDEX idx_notes_folder ON notes(folder_id);
CREATE INDEX idx_folders_project ON folders(project_id);
CREATE INDEX idx_steps_session ON research_steps(session_id, step_order);
CREATE INDEX idx_results_session ON research_results(session_id, result_order);
CREATE INDEX idx_memory_scope ON memory_entries(type, project_id, is_active);
