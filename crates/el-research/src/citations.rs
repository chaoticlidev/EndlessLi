//! Citation validation and renumbering.
//!
//! Synthesis prompts require `[n]` markers against an explicit source list,
//! but models stray: they cite unlisted numbers or keep counting past the
//! list. Validation strips unknown markers and reports whether any valid
//! citation survived, so traceability is checked, not assumed. Renumbering
//! maps child-local markers onto a merged global source list for Deep runs.

use regex::Regex;
use std::sync::OnceLock;

fn citation_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\[(\d+)\]").expect("valid citation pattern"))
}

/// Strips markers outside `1..=source_count`, returning the cleaned text
/// and whether at least one valid citation remains.
pub fn validate_citations(text: &str, source_count: usize) -> (String, bool) {
    let mut valid = false;
    let cleaned = citation_pattern()
        .replace_all(text, |captures: &regex::Captures| {
            let number: usize = captures[1].parse().unwrap_or(0);
            if number >= 1 && number <= source_count {
                valid = true;
                captures[0].to_string()
            } else {
                String::new()
            }
        })
        .to_string();
    (cleaned, valid)
}

/// Rewrites child-local markers onto global numbers: `mapping[i - 1]` is the
/// global number for local marker `[i]`. Local markers without a mapping are
/// stripped, matching [`validate_citations`] semantics.
pub fn renumber_citations(text: &str, mapping: &[usize]) -> String {
    citation_pattern()
        .replace_all(text, |captures: &regex::Captures| {
            let number: usize = captures[1].parse().unwrap_or(0);
            if number >= 1 && number <= mapping.len() {
                format!("[{}]", mapping[number - 1])
            } else {
                String::new()
            }
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_valid_markers() {
        let (cleaned, valid) = validate_citations("See [1] and [2].", 2);
        assert_eq!(cleaned, "See [1] and [2].");
        assert!(valid);
    }

    #[test]
    fn strips_unknown_markers_and_flags_empty() {
        let (cleaned, valid) = validate_citations("See [1] and [9].", 2);
        assert_eq!(cleaned, "See [1] and .");
        assert!(valid);
        let (cleaned, valid) = validate_citations("See [7].", 2);
        assert_eq!(cleaned, "See .");
        assert!(!valid);
    }

    #[test]
    fn renumbers_through_mapping() {
        assert_eq!(renumber_citations("A [1] B [2]", &[3, 5]), "A [3] B [5]");
        assert_eq!(renumber_citations("A [9]", &[3]), "A ");
    }
}
