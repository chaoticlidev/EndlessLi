//! `el-skills`: two-tier skills (Markdown) and scripts (Python).
//!
//! Why this crate exists: skills render a prompt template through the LLM
//! while scripts run in a sandboxed child process (C-3, C-4). Parsing,
//! parameter validation, and the two execution contracts live here so both
//! tiers are tested without a running app.
//!
//! - [`skill`] — Markdown + YAML frontmatter parser/validator and template
//!   rendering (tasks 10.1).
//! - [`runner`] — renders a skill and completes it through an [`el_llm::LlmProvider`]
//!   (task 10.2).
//! - [`script`] — Python docstring metadata parser (task 10.4).
//! - [`sandbox`] — child-process runner with temp dir, timeout, and network
//!   opt-in (task 10.5).
//! - [`builtins`] — the shipped examples (task 10.7).

pub mod builtins;
pub mod frontmatter;
pub mod runner;
pub mod sandbox;
pub mod script;
pub mod skill;

pub use el_core::{AppError, AppResult};
pub use frontmatter::Parameter;
pub use runner::{run_skill, SkillRunOutcome};
pub use sandbox::{run_script, ScriptOutcome};
pub use script::{parse_script, ScriptDoc};
pub use skill::{parse_skill, render_template, SkillDoc};

/// One parameter value supplied by the caller when running a skill or script.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ParamValue {
    pub name: String,
    pub value: String,
}

/// Default script timeout in seconds.
pub const DEFAULT_SCRIPT_TIMEOUT_SECS: u32 = 60;
