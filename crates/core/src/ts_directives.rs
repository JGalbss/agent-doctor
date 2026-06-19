//! `@ts-ignore` / `@ts-expect-error` / `@ts-nocheck` detection. These live in
//! comments, not AST nodes, so the per-node rules can't see them; the lint
//! pipeline scans a file's comments and merges these in under `--agent`.

use oxc_ast::Comment;

use crate::diagnostics::{Category, RawDiagnostic, RuleMeta, Severity};

static TS_IGNORE: RuleMeta = RuleMeta {
    id: "agent-no-ts-ignore",
    severity: Severity::Warn,
    category: Category::AgentHygiene,
    help: "`@ts-ignore` / `@ts-expect-error` / `@ts-nocheck` silence the type checker and let real errors through. Fix the underlying type, narrow it, or decode with Schema. `strict: true` is on everywhere — don't weaken it.",
};

/// Catalog metadata for the comment-scanned ts-directive rule.
pub fn metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&TS_IGNORE];
    METAS
}

const DIRECTIVES: &[&str] = &["@ts-ignore", "@ts-expect-error", "@ts-nocheck"];

/// Emit a finding for every comment carrying a ts-suppression directive.
/// `agent_strict` escalates from `warn` to `error`.
pub fn scan(source: &str, comments: &[Comment], agent_strict: bool) -> Vec<RawDiagnostic> {
    let severity = agent_strict.then_some(Severity::Error);
    let mut findings = Vec::new();
    for comment in comments {
        let start = comment.span.start as usize;
        let end = (comment.span.end as usize).min(source.len());
        if start >= end {
            continue;
        }
        let text = &source[start..end];
        if let Some(directive) = DIRECTIVES
            .iter()
            .find(|directive| text.contains(**directive))
        {
            findings.push(RawDiagnostic {
                meta: &TS_IGNORE,
                span: comment.span,
                message: format!(
                    "{directive} — fix the underlying type instead of silencing the checker"
                ),
                severity,
            });
        }
    }
    findings
}
