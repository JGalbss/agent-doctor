//! Inherit the workspace's TypeScript type setting. agent-doctor reads the
//! project's `tsconfig.json` and, if `strict` mode is not enabled, flags it —
//! strict mode is the foundation every other type-safety rule assumes, so an
//! agent working in a non-strict repo is writing on sand.

use std::path::Path;

use crate::diagnostics::{Category, Diagnostic, RuleMeta, Severity};
use crate::lint::classify_file;

static PREFER_STRICT: RuleMeta = RuleMeta {
    id: "prefer-strict-tsconfig",
    severity: Severity::Warn,
    category: Category::TypeSafety,
    help: "`compilerOptions.strict` is not enabled. Strict mode (strictNullChecks, noImplicitAny, …) is what makes the type checker — and every type-safety rule here — actually load-bearing. Turn it on so the compiler catches what the agent misses.",
};

/// Metas for the tsconfig rule — it fires from the engine pass (project-level,
/// not per-file), so it's appended to the catalog here.
pub fn tsconfig_metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&PREFER_STRICT];
    METAS
}

/// `Some(true)` if a root tsconfig enables strict, `Some(false)` if one exists
/// but does not, `None` if no tsconfig is found. Comment-tolerant: matches the
/// `"strict": true` text rather than JSON-parsing (tsconfigs allow comments and
/// trailing commas, which `serde_json` rejects).
pub fn detect_strict(root: &Path) -> Option<bool> {
    for name in ["tsconfig.json", "tsconfig.base.json"] {
        let Ok(text) = std::fs::read_to_string(root.join(name)) else {
            continue;
        };
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.contains("\"strict\":true") {
            return Some(true);
        }
        return Some(false);
    }
    None
}

/// A finding when the workspace tsconfig exists but isn't strict. `None` when
/// strict is on, or there's no tsconfig to inherit from.
pub fn strict_finding(root: &Path) -> Option<Diagnostic> {
    if detect_strict(root) != Some(false) {
        return None;
    }
    Some(Diagnostic {
        rule: PREFER_STRICT.id,
        severity: PREFER_STRICT.severity,
        category: PREFER_STRICT.category,
        message: "tsconfig.json does not enable `strict` — turn on strict mode".to_string(),
        help: PREFER_STRICT.help,
        file: "tsconfig.json".to_string(),
        file_context: classify_file("tsconfig.json"),
        line: 1,
        column: 1,
        snippet: String::new(),
    })
}
