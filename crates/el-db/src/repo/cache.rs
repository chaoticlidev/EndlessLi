//! Cache, token-usage, and settings repository.
//!
//! Cache keys are SHA-256 digests of `(type, model, normalized query)`.
//! Every write runs size-based LRU eviction afterwards.

use el_core::{AppError, AppResult, CacheEntry, CacheStats, TokenUsage, CACHE_TYPE_VALUES};
use rusqlite::{Connection, Row};
use sha2::{Digest, Sha256};

use super::{check_enum, hex, map_db, now_ts};

/// Default cache ceiling: 1 GB.
pub const DEFAULT_MAX_BYTES: i64 = 1024 * 1024 * 1024;
/// Eviction prunes down to this fraction of the ceiling.
pub const EVICTION_TARGET_FRACTION: f64 = 0.9;
/// Settings key overriding the cache ceiling.
pub const SETTING_CACHE_MAX_BYTES: &str = "cache.max_bytes";

/// Default time-to-live per cache type, in seconds. `None` never expires.
pub fn default_ttl(entry_type: &str) -> Option<i64> {
    match entry_type {
        "search" => Some(24 * 3600),
        "webpage" => Some(7 * 24 * 3600),
        _ => None,
    }
}

/// Normalizes a query for keying: trimmed, inner whitespace collapsed,
/// lowercased.
pub fn normalize_query(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Computes the cache key for `(type, model, query)`.
pub fn cache_key(entry_type: &str, model: &str, query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entry_type.as_bytes());
    hasher.update([0]);
    hasher.update(model.as_bytes());
    hasher.update([0]);
    hasher.update(normalize_query(query).as_bytes());
    hex(&hasher.finalize())
}

fn row_to_entry(row: &Row<'_>) -> rusqlite::Result<CacheEntry> {
    Ok(CacheEntry {
        id: row.get(0)?,
        entry_type: row.get(1)?,
        query: row.get(2)?,
        response: row.get(3)?,
        source_type: row.get(4)?,
        size_bytes: row.get(5)?,
        created_at: row.get(6)?,
        last_accessed_at: row.get(7)?,
        access_count: row.get(8)?,
        expires_at: row.get(9)?,
    })
}

/// Stores a response, refreshing TTL and stats on key repeat, then evicts.
pub fn put(
    conn: &Connection,
    entry_type: &str,
    query: &str,
    model: &str,
    response: &[u8],
    source_type: &str,
    ttl_secs: Option<i64>,
) -> AppResult<CacheEntry> {
    check_enum(entry_type, CACHE_TYPE_VALUES, "type")?;
    let id = cache_key(entry_type, model, query);
    let now = now_ts();
    let expires_at = ttl_secs.map(|ttl| now + ttl);
    let size_bytes = response.len() as i64;
    conn.execute(
        "INSERT INTO cache_entries(id, type, query, response, source_type, size_bytes, created_at, last_accessed_at, access_count, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1, ?8)
         ON CONFLICT(id) DO UPDATE SET response = excluded.response, source_type = excluded.source_type,
         size_bytes = excluded.size_bytes, last_accessed_at = excluded.last_accessed_at,
         access_count = access_count + 1, expires_at = excluded.expires_at",
        rusqlite::params![
            id,
            entry_type,
            query,
            response,
            source_type,
            size_bytes,
            now,
            expires_at
        ],
    )
    .map_err(map_db)?;
    evict_if_needed(conn)?;
    Ok(CacheEntry {
        id,
        entry_type: entry_type.to_string(),
        query: query.to_string(),
        response: response.to_vec(),
        source_type: source_type.to_string(),
        size_bytes,
        created_at: now,
        last_accessed_at: now,
        access_count: 1,
        expires_at,
    })
}

/// Fetches a cached response. Expired rows are removed and reported as a
/// miss. Hits bump the access count and timestamp for LRU order.
pub fn get(
    conn: &Connection,
    entry_type: &str,
    query: &str,
    model: &str,
) -> AppResult<Option<CacheEntry>> {
    let id = cache_key(entry_type, model, query);
    let Some(entry) = get_by_id(conn, &id)? else {
        return Ok(None);
    };
    if entry.expires_at.is_some_and(|exp| exp <= now_ts()) {
        conn.execute("DELETE FROM cache_entries WHERE id = ?1", [&id])
            .map_err(map_db)?;
        return Ok(None);
    }
    conn.execute(
        "UPDATE cache_entries SET last_accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
        rusqlite::params![now_ts(), id],
    )
    .map_err(map_db)?;
    get_by_id(conn, &id)
}

fn get_by_id(conn: &Connection, id: &str) -> AppResult<Option<CacheEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, type, query, response, source_type, size_bytes,
             created_at, last_accessed_at, access_count, expires_at
             FROM cache_entries WHERE id = ?1",
        )
        .map_err(map_db)?;
    let mut rows = stmt.query([id]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row_to_entry(row).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Reports entry count and total bytes.
pub fn stats(conn: &Connection) -> AppResult<CacheStats> {
    let (entries, bytes): (i64, Option<i64>) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0) FROM cache_entries",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(map_db)?;
    Ok(CacheStats {
        entries,
        bytes: bytes.unwrap_or(0),
    })
}

/// Removes every cached row.
pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let count = conn
        .execute("DELETE FROM cache_entries", [])
        .map_err(map_db)?;
    Ok(count)
}

/// Removes every row of one cache type.
pub fn clear_by_type(conn: &Connection, entry_type: &str) -> AppResult<usize> {
    check_enum(entry_type, CACHE_TYPE_VALUES, "type")?;
    let count = conn
        .execute("DELETE FROM cache_entries WHERE type = ?1", [entry_type])
        .map_err(map_db)?;
    Ok(count)
}

/// Deletes LRU rows until usage drops to 90% of the ceiling.
fn evict_if_needed(conn: &Connection) -> AppResult<()> {
    let max_bytes = match get_setting(conn, SETTING_CACHE_MAX_BYTES)? {
        Some(raw) => raw
            .parse::<i64>()
            .map_err(|_| AppError::Database(format!("invalid {SETTING_CACHE_MAX_BYTES}: {raw}")))?,
        None => DEFAULT_MAX_BYTES,
    };
    let target = (max_bytes as f64 * EVICTION_TARGET_FRACTION) as i64;
    loop {
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM cache_entries",
                [],
                |row| row.get(0),
            )
            .map_err(map_db)?;
        if total <= max_bytes {
            return Ok(());
        }
        let removed = conn
            .execute(
                "DELETE FROM cache_entries WHERE id IN (
                   SELECT id FROM cache_entries ORDER BY last_accessed_at ASC LIMIT 10)",
                [],
            )
            .map_err(map_db)?;
        if removed == 0 {
            return Ok(());
        }
        let total: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(size_bytes), 0) FROM cache_entries",
                [],
                |row| row.get(0),
            )
            .map_err(map_db)?;
        if total <= target {
            return Ok(());
        }
    }
}

/// Logs one LLM call for budget accounting.
pub fn log_usage(
    conn: &Connection,
    session_id: Option<&str>,
    provider: &str,
    model: &str,
    tokens_in: i64,
    tokens_out: i64,
) -> AppResult<TokenUsage> {
    if tokens_in < 0 || tokens_out < 0 {
        return Err(AppError::InvalidInput(
            "token counts must not be negative".to_string(),
        ));
    }
    conn.execute(
        "INSERT INTO token_usage(session_id, provider, model, tokens_in, tokens_out, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![session_id, provider, model, tokens_in, tokens_out, now_ts()],
    )
    .map_err(map_db)?;
    let id = conn.last_insert_rowid();
    Ok(TokenUsage {
        id,
        session_id: session_id.map(str::to_string),
        provider: provider.to_string(),
        model: model.to_string(),
        tokens_in,
        tokens_out,
        created_at: now_ts(),
    })
}

/// Sums tokens logged since `since_ts` (daily/monthly budget windows).
pub fn sum_since(conn: &Connection, since_ts: i64) -> AppResult<(i64, i64)> {
    let (tokens_in, tokens_out): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT SUM(tokens_in), SUM(tokens_out) FROM token_usage WHERE created_at >= ?1",
            [since_ts],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(map_db)?;
    Ok((tokens_in.unwrap_or(0), tokens_out.unwrap_or(0)))
}

/// Sums tokens for one session.
pub fn sum_for_session(conn: &Connection, session_id: &str) -> AppResult<(i64, i64)> {
    let (tokens_in, tokens_out): (Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT SUM(tokens_in), SUM(tokens_out) FROM token_usage WHERE session_id = ?1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(map_db)?;
    Ok((tokens_in.unwrap_or(0), tokens_out.unwrap_or(0)))
}

/// Reads a setting value, or `None` when unset.
pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    let mut stmt = conn
        .prepare("SELECT value FROM settings WHERE key = ?1")
        .map_err(map_db)?;
    let mut rows = stmt.query([key]).map_err(map_db)?;
    match rows.next().map_err(map_db)? {
        Some(row) => Ok(Some(row.get(0).map_err(map_db)?)),
        None => Ok(None),
    }
}

/// Writes a setting value (JSON), replacing any previous value.
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    .map_err(map_db)?;
    Ok(())
}

/// Removes a setting.
pub fn delete_setting(conn: &Connection, key: &str) -> AppResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])
        .map_err(map_db)?;
    Ok(())
}
