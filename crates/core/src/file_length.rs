//! File-length hygiene: a single, AST-free check that flags oversized source
//! files. Unlike the per-file [`crate::rules::Rule`] dispatch (which only runs
//! on files importing `effect`), this fires on every scanned TypeScript file —
//! a 1,000-line module is a problem whether or not it touches Effect. It runs
//! from the engine pass under the `--agent` family, escalating to `error` with
//! `--agent-strict`.

use crate::diagnostics::{Category, Diagnostic, RuleMeta, Severity};
use crate::lint::classify_file;

/// Files longer than this many lines are flagged. Past ~650 lines a TypeScript
/// module is almost always doing several jobs and should be split.
pub const MAX_FILE_LINES: usize = 650;

static MAX_FILE_LENGTH: RuleMeta = RuleMeta {
    id: "agent-max-file-length",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "This file is over 650 lines — too long to hold in context and a sign it does several jobs. Split it into focused modules (one clear purpose each) so agents and humans can reason about it.",
};

/// Metas for the file-length rule — appended to the global catalog since it
/// fires from the engine pass, not the per-file [`crate::rules::Rule`] dispatch.
pub fn file_length_metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&MAX_FILE_LENGTH];
    METAS
}

/// Flag `source` when it exceeds [`MAX_FILE_LINES`]. Returns `None` for files
/// within the limit. `agent_strict` escalates the finding to `error`.
pub fn check(display_path: &str, source: &str, agent_strict: bool) -> Option<Diagnostic> {
    let lines = source.lines().count();
    if lines <= MAX_FILE_LINES {
        return None;
    }
    let severity = match agent_strict {
        true => Severity::Error,
        false => MAX_FILE_LENGTH.severity,
    };
    Some(Diagnostic {
        rule: MAX_FILE_LENGTH.id,
        severity,
        category: MAX_FILE_LENGTH.category,
        message: format!("file is {lines} lines (limit {MAX_FILE_LINES}) — split it into focused modules"),
        help: MAX_FILE_LENGTH.help,
        file: display_path.to_string(),
        file_context: classify_file(display_path),
        line: (MAX_FILE_LINES + 1) as u32,
        column: 1,
        snippet: format!("// {lines} lines total"),
    })
}
