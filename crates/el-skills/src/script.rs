//! Script format: Python with a metadata docstring (task 10.4).
//!
//! A script begins with a module docstring holding the same YAML as a skill,
//! plus an explicit `allow_network` flag:
//!
//! ```python
//! """
//! name: Word Count
//! parameters:
//!   - name: text
//!     required: true
//! allow_network: false
//! """
//! ```
//!
//! Extraction is a focused lexer for the leading module docstring only: it does
//! not execute the source or import a Python parser.

use el_core::{AppError, AppResult};

use crate::frontmatter::{self, Parameter};

/// A parsed, validated script. The body (code) lives in the stored `content`
/// column, not here; this is only the metadata the UI and runner need.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ScriptDoc {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: Vec<Parameter>,
    pub allow_network: bool,
}

/// Parses and validates a script document.
pub fn parse_script(document: &str) -> AppResult<ScriptDoc> {
    let docstring = extract_module_docstring(document)?;
    let meta = frontmatter::parse_metadata(&docstring, "script")?;
    let doc = ScriptDoc {
        name: meta.name,
        description: meta.description,
        category: meta.category,
        parameters: meta.parameters,
        allow_network: meta.allow_network,
    };
    validate_script(&doc)?;
    Ok(doc)
}

/// Validates a parsed script: non-blank name, unique parameters.
pub fn validate_script(doc: &ScriptDoc) -> AppResult<()> {
    if doc.name.is_empty() {
        return Err(AppError::InvalidInput(
            "script name must not be blank".to_string(),
        ));
    }
    frontmatter::validate_parameters(&doc.parameters)
}

/// Builds the repository input for a script. The full source is stored as
/// `content` so the editor round-trips the docstring and code verbatim.
pub fn to_new_script(document: &str, doc: &ScriptDoc) -> AppResult<el_core::NewScript> {
    Ok(el_core::NewScript {
        name: doc.name.clone(),
        description: doc.description.clone(),
        content: document.to_string(),
        parameters: serde_json::to_string(&doc.parameters)
            .map_err(|err| AppError::Internal(format!("encode script parameters: {err}")))?,
        category: doc.category.clone(),
        allow_network: doc.allow_network,
    })
}

/// Returns the module docstring of a Python source file.
///
/// Leading blank lines, comments (including `#!` shebangs and encoding
/// declarations) are skipped; the first real statement must be a triple-quoted
/// string literal, possibly with a `r`/`u`/`b`/`f` prefix.
pub fn extract_module_docstring(source: &str) -> AppResult<String> {
    let text = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut offset = 0usize;
    while offset < text.len() {
        let line_end = text[offset..]
            .find('\n')
            .map_or(text.len(), |index| offset + index + 1);
        let line = &text[offset..line_end];
        let leading = line.trim_start_matches([' ', '\t']);
        if leading.trim().is_empty() || leading.starts_with('#') {
            offset = line_end;
            continue;
        }
        return capture_docstring(&text[offset..]);
    }
    Err(AppError::InvalidInput(
        "script has no module docstring; add a triple-quoted metadata block as the first statement"
            .to_string(),
    ))
}

/// Reads the triple-quoted string that must start the given source.
fn capture_docstring(source: &str) -> AppResult<String> {
    let first_line_end = source.find('\n').unwrap_or(source.len());
    let first_line = &source[..first_line_end];
    let Some((quote_index, quote)) = find_quote(first_line) else {
        return Err(AppError::InvalidInput(
            "script metadata must be a triple-quoted module docstring".to_string(),
        ));
    };
    let opener: String = std::iter::repeat_n(quote, 3).collect();
    if !first_line[quote_index..].starts_with(&opener) {
        return Err(AppError::InvalidInput(
            "script metadata must use a triple-quoted string".to_string(),
        ));
    }
    let content_start = quote_index + 3;
    let Some(end) = source[content_start..].find(&opener) else {
        return Err(AppError::InvalidInput(
            "script module docstring is not closed with matching triple quotes".to_string(),
        ));
    };
    Ok(dedent(&source[content_start..content_start + end]))
}

/// Finds the opening quote of a string literal on the first code line,
/// skipping indentation and string-prefix letters.
fn find_quote(line: &str) -> Option<(usize, char)> {
    for (index, ch) in line.char_indices() {
        match ch {
            ' ' | '\t' => continue,
            'r' | 'R' | 'u' | 'U' | 'b' | 'B' | 'f' | 'F' => continue,
            '"' | '\'' => return Some((index, ch)),
            _ => return None,
        }
    }
    None
}

/// Removes the common leading indentation so an indented docstring still
/// parses as top-level YAML.
fn dedent(raw: &str) -> String {
    let indent = raw
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    raw.split('\n')
        .map(|line| strip_indent(line, indent))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Strips up to `count` leading spaces/tabs from a line.
fn strip_indent(line: &str, count: usize) -> &str {
    let mut removed = 0;
    for (index, ch) in line.char_indices() {
        if removed >= count {
            return &line[index..];
        }
        if ch == ' ' || ch == '\t' {
            removed += 1;
        } else {
            return &line[index..];
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#""""
name: Text Stats
description: Count things
category: text
parameters:
  - name: text
    required: true
  - name: top
    default: "10"
allow_network: false
"""

import os
print("hi")
"#;

    #[test]
    fn parses_a_script() {
        let doc = parse_script(DOC).expect("parse");
        assert_eq!(doc.name, "Text Stats");
        assert_eq!(doc.category, "text");
        assert_eq!(doc.parameters.len(), 2);
        assert!(!doc.allow_network);
    }

    #[test]
    fn extracts_docstring_only() {
        let docstring = extract_module_docstring(DOC).expect("extract");
        assert!(docstring.starts_with("name: Text Stats"));
        assert!(docstring.ends_with("allow_network: false"));
    }

    #[test]
    fn skips_shebang_encoding_and_comments() {
        let source = "#!/usr/bin/env python\n# -*- coding: utf-8 -*-\n\n\"\"\"\nname: X\n\"\"\"\n";
        let doc = parse_script(source).expect("parse");
        assert_eq!(doc.name, "X");
    }

    #[test]
    fn single_line_docstring() {
        let doc = parse_script("\"\"\"name: One Liner\"\"\"\n").expect("parse");
        assert_eq!(doc.name, "One Liner");
    }

    #[test]
    fn raw_prefix_docstring() {
        let doc = parse_script("r\"\"\"\nname: Raw\nallow_network: true\n\"\"\"\n").expect("parse");
        assert!(doc.allow_network);
    }

    #[test]
    fn indented_docstring_is_dedented() {
        let source = "\"\"\"\n  name: Indented\n  parameters:\n    - name: a\n      required: true\n\"\"\"\n";
        let doc = parse_script(source).expect("parse");
        assert_eq!(doc.name, "Indented");
        assert_eq!(doc.parameters[0].name, "a");
    }

    #[test]
    fn missing_docstring_is_rejected() {
        assert!(parse_script("import os\nprint('hi')\n").is_err());
    }

    #[test]
    fn non_string_first_statement_is_rejected() {
        assert!(parse_script("x = 1\n").is_err());
    }

    #[test]
    fn unterminated_docstring_is_rejected() {
        assert!(parse_script("\"\"\"\nname: X\n").is_err());
    }
}
