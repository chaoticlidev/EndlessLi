//! `el-memory`: user-controlled memory hierarchy and prompt assembly.

use std::time::{SystemTime, UNIX_EPOCH};

use el_core::AppResult;
use el_db::repo::memory as memory_repo;

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryCommand {
    Remember { project: bool, content: String },
    Forget { id: String },
    Show,
}

/// Parses a slash command. Returns `None` when the input is not a memory command.
pub fn parse(input: &str) -> Option<MemoryCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut parts = trimmed.splitn(2, ' ');
    let head = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();
    match head {
        "/remember" => {
            if rest.is_empty() {
                None
            } else {
                Some(MemoryCommand::Remember {
                    project: false,
                    content: rest.to_string(),
                })
            }
        }
        "/project" => {
            // /project remember ... | /project forget ...
            let mut inner = rest.splitn(2, ' ');
            match inner.next().unwrap_or("") {
                "remember" => {
                    let content = inner.next().unwrap_or("").trim();
                    if content.is_empty() {
                        None
                    } else {
                        Some(MemoryCommand::Remember {
                            project: true,
                            content: content.to_string(),
                        })
                    }
                }
                "forget" => {
                    let id = inner.next().unwrap_or("").trim();
                    if id.is_empty() {
                        None
                    } else {
                        Some(MemoryCommand::Forget {
                            id: id.to_string(),
                        })
                    }
                }
                _ => None,
            }
        }
        "/forget" => {
            if rest.is_empty() {
                None
            } else {
                Some(MemoryCommand::Forget {
                    id: rest.to_string(),
                })
            }
        }
        "/showmem" => Some(MemoryCommand::Show),
        _ => None,
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Assembles the memory block for prompt injection. Precedence:
/// session (temporary) -> project (overrides global) -> global.
/// Returns an empty string when memory is disabled or nothing applies.
pub fn build_context(
    conn: &rusqlite::Connection,
    project_id: Option<&str>,
    session_memory: Option<&str>,
    enabled: bool,
) -> AppResult<String> {
    if !enabled {
        return Ok(String::new());
    }
    if !memory_repo::is_enabled(conn)? {
        return Ok(String::new());
    }
    let now = now_ts();
    let mut lines: Vec<String> = Vec::new();
    // global
    for entry in memory_repo::list_global(conn)? {
        if entry
            .expires_at
            .map(|exp| exp > now)
            .unwrap_or(true)
        {
            lines.push(format!("- {}", entry.content));
        }
    }
    // project (overrides: appended after global so model sees project last)
    if let Some(pid) = project_id {
        for entry in memory_repo::list_for_project(conn, pid)? {
            if entry.expires_at.map(|exp| exp > now).unwrap_or(true) {
                lines.push(format!("- {}", entry.content));
            }
        }
    }
    // session memory (highest precedence, last)
    if let Some(session) = session_memory {
        let trimmed = session.trim();
        if !trimmed.is_empty() {
            lines.push(format!("- {}", trimmed));
        }
    }
    if lines.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("Memory:\n{}", lines.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remember_and_project() {
        assert_eq!(
            parse("/remember hello world"),
            Some(MemoryCommand::Remember {
                project: false,
                content: "hello world".to_string()
            })
        );
        assert_eq!(
            parse("/project remember foo"),
            Some(MemoryCommand::Remember {
                project: true,
                content: "foo".to_string()
            })
        );
        assert_eq!(
            parse("/forget abc"),
            Some(MemoryCommand::Forget { id: "abc".to_string() })
        );
        assert_eq!(parse("/showmem"), Some(MemoryCommand::Show));
        assert!(parse("not a command").is_none());
    }
}
