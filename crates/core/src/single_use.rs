//! Cross-file single-use-helper detection (engine `--agent` pass). opencode's
//! flagship rule: "do not extract single-use helpers preemptively." Using the
//! repo-wide [`SymbolGraph`], flag an **exported** function/const that exactly
//! **one** other module imports — a co-location / inline candidate, unless it's
//! a genuine reusable boundary. `info` only; never scored.

use std::collections::{BTreeSet, HashMap};

use crate::diagnostics::{Category, Diagnostic, FileContext, RuleMeta, Severity};
use crate::lint::is_test_file;
use crate::symbol_graph::{SymbolGraph, SymbolKind};

static SINGLE_USE: RuleMeta = RuleMeta {
    id: "agent-no-single-use-helper",
    severity: Severity::Info,
    category: Category::AgentHygiene,
    help: "An exported helper imported by exactly one other module is usually premature extraction. Inline it at the call site or co-locate it, unless it hides a genuinely complex boundary or has a clear independent name. (opencode: \"do not extract single-use helpers preemptively\".)",
};

/// Catalog metadata for the cross-file single-use rule (fires from the engine
/// pass, not per-file dispatch).
pub fn metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&SINGLE_USE];
    METAS
}

/// An exported definition with a single importing module.
pub struct SingleUseHit {
    pub file: String,
    pub name: String,
    pub line: u32,
    pub column: u32,
    pub importer: String,
}

/// Find exported function/const definitions imported by exactly one other file.
pub fn analyze(graph: &SymbolGraph) -> Vec<SingleUseHit> {
    // (defining file, symbol name) -> set of importing files.
    let mut consumers: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
    for edge in graph.import_edges() {
        for name in &edge.names {
            consumers
                .entry((edge.to.clone(), name.clone()))
                .or_default()
                .insert(edge.from.clone());
        }
    }
    let mut hits = Vec::new();
    for file in graph.files() {
        for def in &file.defs {
            if !def.exported || !is_helper(def.kind) {
                continue;
            }
            let Some(importers) = consumers.get(&(file.path.clone(), def.name.clone())) else {
                continue;
            };
            if importers.len() != 1 {
                continue;
            }
            hits.push(SingleUseHit {
                file: file.path.clone(),
                name: def.name.clone(),
                line: def.line,
                column: def.column,
                importer: importers.iter().next().cloned().unwrap_or_default(),
            });
        }
    }
    hits.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    hits
}

fn is_helper(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Const)
}

/// Build a [`Diagnostic`] for a hit; `snippet` is the source line of the
/// definition (the engine reads it, since the graph doesn't retain source).
pub fn to_diagnostic(hit: &SingleUseHit, snippet: String) -> Diagnostic {
    let file_context = match is_test_file(&hit.file) {
        true => FileContext::Test,
        false => FileContext::Production,
    };
    Diagnostic {
        rule: SINGLE_USE.id,
        severity: SINGLE_USE.severity,
        category: SINGLE_USE.category,
        message: format!(
            "`{}` is exported but imported by only one module ({}) — inline or co-locate it unless it's a real reusable boundary",
            hit.name, hit.importer
        ),
        help: SINGLE_USE.help,
        file: hit.file.clone(),
        file_context,
        line: hit.line,
        column: hit.column,
        snippet,
    }
}
