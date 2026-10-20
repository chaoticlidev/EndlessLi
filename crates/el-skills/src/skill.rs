//! Skill format: Markdown with YAML frontmatter (task 10.1).
//!
//! A skill is
//!
//! ```text
//! ---
//! name: Summarize
//! description: Condense a passage.
//! category: writing
//! parameters:
//!   - name: text
//!     required: true
//! ---
//! Summarize: {{text}}
//! ```
//!
//! [`parse_skill`] splits and validates it; [`render_template`] fills the
//! `{{name}}` placeholders from caller-supplied values.

use std::collections::HashMap;

use el_core::{AppError, AppResult};

use crate::frontmatter::{self, Parameter};
use crate::ParamValue;

/// A parsed, validated skill.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct SkillDoc {
    pub name: String,
    pub description: String,
    pub category: String,
    pub parameters: Vec<Parameter>,
    /// The prompt template (the body after the frontmatter).
    pub template: String,
}

/// Parses and validates a skill document.
pub fn parse_skill(document: &str) -> AppResult<SkillDoc> {
    let parts = frontmatter::split(document)?;
    let yaml = parts.yaml.ok_or_else(|| {
        AppError::InvalidInput(
            "skill is missing YAML frontmatter: the file must start with a '---' line".to_string(),
        )
    })?;
    let meta = frontmatter::parse_metadata(yaml, "skill")?;
    let doc = SkillDoc {
        name: meta.name,
        description: meta.description,
        category: meta.category,
        parameters: meta.parameters,
        template: parts.body.trim().to_string(),
    };
    validate_skill(&doc)?;
    Ok(doc)
}

/// Validates a parsed skill: non-blank name and body, unique parameters.
pub fn validate_skill(doc: &SkillDoc) -> AppResult<()> {
    if doc.name.is_empty() {
        return Err(AppError::InvalidInput(
            "skill name must not be blank".to_string(),
        ));
    }
    if doc.template.is_empty() {
        return Err(AppError::InvalidInput(
            "skill body (prompt template) must not be empty".to_string(),
        ));
    }
    frontmatter::validate_parameters(&doc.parameters)
}

/// Builds the repository input for a skill. The full document is stored as
/// `content` so the editor round-trips frontmatter and body verbatim; the
/// derived fields exist for listing and search.
pub fn to_new_skill(document: &str, doc: &SkillDoc) -> AppResult<el_core::NewSkill> {
    Ok(el_core::NewSkill {
        name: doc.name.clone(),
        description: doc.description.clone(),
        content: document.to_string(),
        parameters: serde_json::to_string(&doc.parameters)
            .map_err(|err| AppError::Internal(format!("encode skill parameters: {err}")))?,
        category: doc.category.clone(),
    })
}

/// Renders a template by substituting `{{name}}` tokens with parameter values.
///
/// Values fall back to the parameter's default, then to an empty string; a
/// required parameter with neither is an error. Tokens that do not name a
/// declared parameter are left verbatim so unrelated `{{...}}` text survives.
pub fn render_template(
    template: &str,
    parameters: &[Parameter],
    values: &[ParamValue],
) -> AppResult<String> {
    let provided: HashMap<&str, &str> = values
        .iter()
        .map(|value| (value.name.trim(), value.value.as_str()))
        .collect();
    let mut resolved: HashMap<&str, String> = HashMap::new();
    for param in parameters {
        let name = param.name.as_str();
        if let Some(value) = provided.get(name).filter(|value| !value.trim().is_empty()) {
            resolved.insert(name, (*value).to_string());
            continue;
        }
        if let Some(default) = &param.default {
            resolved.insert(name, default.clone());
        } else if param.required {
            return Err(AppError::InvalidInput(format!(
                "parameter '{name}' is required"
            )));
        } else {
            resolved.insert(name, String::new());
        }
    }
    Ok(substitute(template, &resolved))
}

/// Replaces `{{ name }}` tokens (whitespace-tolerant) with resolved values.
fn substitute(template: &str, values: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let token = after[..end].trim();
                match values.get(token) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("{{");
                        out.push_str(&after[..end]);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"---
name: Summarize
description: Condense a passage
category: writing
parameters:
  - name: text
    required: true
  - name: length
    default: 3 bullets
---
Summarize into {{length}}:

{{text}}
"#;

    fn value(name: &str, value: &str) -> ParamValue {
        ParamValue {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn parses_a_skill() {
        let doc = parse_skill(DOC).expect("parse");
        assert_eq!(doc.name, "Summarize");
        assert_eq!(doc.category, "writing");
        assert_eq!(doc.parameters.len(), 2);
        assert!(doc.parameters[0].required);
        assert_eq!(doc.parameters[1].default.as_deref(), Some("3 bullets"));
        assert!(doc.template.starts_with("Summarize into"));
    }

    #[test]
    fn category_defaults_when_omitted() {
        let doc = parse_skill("---\nname: X\n---\nbody").expect("parse");
        assert_eq!(doc.category, "general");
    }

    #[test]
    fn missing_frontmatter_is_rejected() {
        assert!(parse_skill("just a body").is_err());
    }

    #[test]
    fn blank_name_is_rejected() {
        assert!(parse_skill("---\nname: '  '\n---\nbody").is_err());
    }

    #[test]
    fn empty_body_is_rejected() {
        assert!(parse_skill("---\nname: X\n---\n   ").is_err());
    }

    #[test]
    fn duplicate_parameters_are_rejected() {
        let doc = "---\nname: X\nparameters:\n  - name: a\n  - name: a\n---\nbody";
        assert!(parse_skill(doc).is_err());
    }

    #[test]
    fn malformed_yaml_is_rejected() {
        assert!(parse_skill("---\nname: [unclosed\n---\nbody").is_err());
    }

    #[test]
    fn renders_values_defaults_and_keeps_unknown_tokens() {
        let doc = parse_skill(DOC).expect("parse");
        let out = render_template(
            &doc.template,
            &doc.parameters,
            &[value("text", "hello world")],
        )
        .expect("render");
        assert!(out.contains("Summarize into 3 bullets"));
        assert!(out.contains("hello world"));

        let out = render_template(
            &doc.template,
            &doc.parameters,
            &[value("text", "hi"), value("length", "one line")],
        )
        .expect("render");
        assert!(out.contains("Summarize into one line"));

        let out = render_template("Keep {{unknown}} this", &[], &[]).expect("render");
        assert_eq!(out, "Keep {{unknown}} this");
    }

    #[test]
    fn missing_required_parameter_errors() {
        let doc = parse_skill(DOC).expect("parse");
        let err = render_template(&doc.template, &doc.parameters, &[]).expect_err("must fail");
        assert!(err.to_string().contains("text"));
    }

    #[test]
    fn whitespace_tolerant_tokens() {
        let doc =
            parse_skill("---\nname: X\nparameters:\n  - name: a\n---\nA={{ a }}").expect("parse");
        let out = render_template(&doc.template, &doc.parameters, &[value("a", "b")]).expect("r");
        assert_eq!(out, "A=b");
    }
}
