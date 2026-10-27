-- Synthesis jobs table (Phase 14: Synthesize tab).
--
-- Stores each synthesis run: inputs (k, media type, instructions, doc refs),
-- pipeline status, and the final generated output. Jobs persist so the
-- history panel can show completed syntheses.

CREATE TABLE synthesis_jobs (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending','in_progress','completed','failed','cancelled')),
  k INTEGER NOT NULL,
  media_type TEXT NOT NULL CHECK (media_type IN ('report','slides','flashcards','quiz','audio','dataset')),
  instructions TEXT,
  doc_refs TEXT NOT NULL,
  model TEXT,
  tokens_in INTEGER NOT NULL DEFAULT 0,
  tokens_out INTEGER NOT NULL DEFAULT 0,
  output TEXT,
  error_message TEXT,
  created_at INTEGER NOT NULL,
  completed_at INTEGER
);

CREATE INDEX idx_synthesis_status ON synthesis_jobs(status);
CREATE INDEX idx_synthesis_created ON synthesis_jobs(created_at DESC);
