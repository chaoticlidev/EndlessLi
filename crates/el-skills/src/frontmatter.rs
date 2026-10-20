//! YAML frontmatter shared by both tiers.
//!
//! A skill is a Markdown file whose first line is `---`; a script is Python
//! whose leading docstring holds the same YAML. One splitter and one metadata
//! shape keep the two formats consistent.

use el_core::{AppError, AppResult};
use serde::Deserialize;

/// A document split into its optional YAML metadata and its body.
#[derive(Debug, PartialEq, Eq)]
pub struct Document<'a> {
    /// The YAML between the `---` fences, without the fences themselves.
    pub yaml: Option<&'a str>,
    /// Everything after the closing fence (or the whole input when there is
    /// no frontmatter).
    pub body: &'a str,
}

/// Splits Markdown frontmatter from its body.
///
/// The opening `---` must be the first non-blank line (a leading BOM and blank
/// lines are tolerated); the block closes at the next `---` or `...` line at
/// column zero. A start without a matching close is a validation error, not a
/// silent fallthrough, so a truncated file cannot be mistaken for prose.
pub fn split(document: &str) -> AppResult<Document<'_>> {
    let text = document.strip_prefix('\u{feff}').unwrap_or(document);
    let mut offset = 0usize;
    let mut yaml_start: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        match yaml_start {
            None => {
                if trimmed.trim().is_empty() {
                    offset += line.len();
                    continue;
                }
                if trimmed == "---" {
                    yaml_start = Some(offset + line.len());
                    offset += line.len();
                    continue;
                }
                // First real line is not a fence: no frontmatter at all.
                return Ok(Document {
                    yaml: None,
                    body: text,
                });
            }
            Some(start) => {
                if trimmed == "---" || trimmed == "..." {
                    let yaml = text.get(start..offset).unwrap_or("");
                    let body_start = offset + line.len();
                    let body = text.get(body_start..).unwrap_or("");
                    return Ok(Document {
                        yaml: Some(yaml),
                        body,
                    });
                }
                offset += line.len();
            }
        }
    }
    if yaml_start.is_some() {
        return Err(AppError::InvalidInput(
            "frontmatter opens with '---' but never closes with a matching '---' line".to_string(),
        ));
    }
    Ok(Document {
        yaml: None,
        body: text,
    })
}

/// One declared parameter, in the public/API shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct Parameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    /// Value used when the caller leaves the parameter empty.
    pub default: Option<String>,
}

/// The YAML-facing parameter shape: omitted fields are tolerated and
/// `default_value` is accepted as an alias. Kept separate from [`Parameter`]
/// so the exported type stays a single, symmetric shape for TypeScript.
#[derive(Debug, Clone, Deserialize)]
struct ParamSpec {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    required: bool,
    #[serde(default, alias = "default_value")]
    default: Option<String>,
}

impl ParamSpec {
    fn normalize(self) -> Parameter {
        Parameter {
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            required: self.required,
            default: self.default,
        }
    }
}

/// Normalized metadata common to both tiers. `allow_network` is ignored by
/// skills and honored by scripts.
#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: Vec<Parameter>,
    pub allow_network: bool,
}

/// The raw YAML shape, before trimming and normalization.
#[derive(Debug, Clone, Deserialize)]
struct RawMetadata {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    parameters: Vec<ParamSpec>,
    #[serde(default)]
    allow_network: bool,
}

/// Category used when the metadata omits one.
pub fn default_category() -> String {
    "general".to_string()
}

/// Parses and normalizes a YAML metadata block.
pub fn parse_metadata(yaml: &str, what: &str) -> AppResult<Metadata> {
    let raw: RawMetadata = serde_yaml::from_str(yaml)
        .map_err(|err| AppError::InvalidInput(format!("invalid {what} metadata: {err}")))?;
    Ok(Metadata {
        name: raw.name.trim().to_string(),
        description: raw.description.trim().to_string(),
        category: if raw.category.trim().is_empty() {
            default_category()
        } else {
            raw.category.trim().to_string()
        },
        parameters: raw
            .parameters
            .into_iter()
            .map(ParamSpec::normalize)
            .collect(),
        allow_network: raw.allow_network,
    })
}

/// Rejects blank or duplicate parameter names.
pub fn validate_parameters(parameters: &[Parameter]) -> AppResult<()> {
    let mut seen = std::collections::HashSet::new();
    for param in parameters {
        if param.name.is_empty() {
            return Err(AppError::InvalidInput(
                "parameter names must not be blank".to_string(),
            ));
        }
        if !seen.insert(param.name.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "duplicate parameter '{}'",
                param.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_and_body() {
        let doc = "---\nname: X\n---\nHello body\n";
        let parts = split(doc).expect("split");
        assert_eq!(parts.yaml, Some("name: X\n"));
        assert_eq!(parts.body, "Hello body\n");
    }

    #[test]
    fn tolerates_no_frontmatter() {
        let parts = split("just text\n").expect("split");
        assert_eq!(parts.yaml, None);
        assert_eq!(parts.body, "just text\n");
    }

    #[test]
    fn tolerates_bom_and_leading_blanks() {
        let parts = split("\u{feff}\n\n---\nname: X\n---\nbody").expect("split");
        assert_eq!(parts.yaml, Some("name: X\n"));
        assert_eq!(parts.body, "body");
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        assert!(split("---\nname: X\nbody").is_err());
    }

    #[test]
    fn crlf_endings_are_handled() {
        let parts = split("---\r\nname: X\r\n---\r\nbody").expect("split");
        assert_eq!(parts.yaml, Some("name: X\r\n"));
        assert_eq!(parts.body, "body");
    }

    #[test]
    fn metadata_normalizes_and_accepts_default_alias() {
        let meta = parse_metadata(
            "name: X\nparameters:\n  - name: a\n    default_value: 5\n",
            "skill",
        )
        .expect("parse");
        assert_eq!(meta.category, "general");
        assert_eq!(meta.parameters[0].default.as_deref(), Some("5"));
    }
}
