//! Output validation for language-model text.
//!
//! Model output is untrusted input: it may contain raw HTML, script tags, or
//! `javascript:` links (prompt-injected or echoed from sources). Everything
//! shown in the UI or saved to notes passes through [`sanitize_llm_output`],
//! which keeps text and safe formatting while removing active content.

/// Strips scripts, event handlers, and dangerous URLs from model output.
/// Plain text and safe inline formatting survive unchanged.
pub fn sanitize_llm_output(raw: &str) -> String {
    ammonia::Builder::default()
        .link_rel(Some("noreferrer"))
        .clean(raw)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts_but_keeps_text() {
        let out = sanitize_llm_output("Hello<script>alert(1)</script> world");
        assert!(out.contains("Hello"));
        assert!(out.contains("world"));
        assert!(!out.contains("script"));
        assert!(!out.contains("alert"));
    }

    #[test]
    fn strips_event_handlers_and_javascript_urls() {
        let out = sanitize_llm_output(
            r#"<a href="javascript:steal()">click</a><img src="x" onerror="steal()">"#,
        );
        assert!(!out.contains("javascript:"));
        assert!(!out.contains("onerror"));
        assert!(out.contains("click"));
    }

    #[test]
    fn plain_text_passes_through() {
        let text = "Just a summary with **markdown** and a list:\n- one\n- two";
        assert_eq!(sanitize_llm_output(text), text);
    }
}
