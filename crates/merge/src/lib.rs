//! Semantic (AST-level) 3-way merge for TypeScript (toolkit Layer 3).
//!
//! Files are decomposed into top-level *items* — named declarations
//! (function/class/const/…) keyed by name, and non-declaration chunks (imports,
//! statements) keyed by their normalised text. A standard 3-way merge then runs
//! per item, so the agent-fleet common case — two agents adding *different*
//! functions to the same file — merges with **zero conflict**, and a pure
//! reordering is not a conflict either. Only edits to the *same* declaration
//! conflict, and they're reported with semantic context.
//!
//! When any side fails to parse (or for non-TS callers), we fall back to a
//! coarse but *safe* whole-file 3-way that never fabricates a merge.

mod items;

use serde::Serialize;

use items::{normalize, parse_items, Item};

/// A single unresolved conflict.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Conflict {
    /// The item key (`decl:<name>` or `other:<text>`).
    pub key: String,
    /// Human-readable description of what collided.
    pub description: String,
}

/// The outcome of a merge.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MergeResult {
    /// The merged source. Conflicted items carry git-style markers.
    pub merged: String,
    /// Conflicts found (empty ⇒ clean merge).
    pub conflicts: Vec<Conflict>,
    /// Top-level declaration names whose resolution differs from base — the
    /// delta to feed impact-based test selection (Layer 2).
    pub changed_symbols: Vec<String>,
    /// Whether the coarse line/file fallback was used (parse failure).
    pub fell_back: bool,
}

impl MergeResult {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}

/// 3-way merge of `ours` and `theirs` against their common `base`.
pub fn merge(base: &str, ours: &str, theirs: &str) -> MergeResult {
    match (parse_items(base), parse_items(ours), parse_items(theirs)) {
        (Some(base_items), Some(ours_items), Some(theirs_items)) => {
            structural_merge(&base_items, &ours_items, &theirs_items)
        }
        _ => fallback_merge(base, ours, theirs),
    }
}

/// One item's merge decision.
enum Resolved {
    Take(String),
    Remove,
    Conflict {
        ours: Option<String>,
        theirs: Option<String>,
    },
}

fn structural_merge(base: &[Item], ours: &[Item], theirs: &[Item]) -> MergeResult {
    let base_map = Item::index(base);
    let ours_map = Item::index(ours);
    let theirs_map = Item::index(theirs);

    // Deterministic key order: base order, then ours-only adds, then theirs-only.
    let mut order: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in base.iter().chain(ours).chain(theirs) {
        if seen.insert(item.key.as_str()) {
            order.push(item.key.as_str());
        }
    }

    let mut blocks: Vec<String> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    let mut changed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for key in order {
        let base_item = base_map.get(key).copied();
        let ours_item = ours_map.get(key).copied();
        let theirs_item = theirs_map.get(key).copied();
        let (resolved, did_change) = resolve(base_item, ours_item, theirs_item);
        if did_change {
            if let Some(name) = decl_name(key) {
                changed.insert(name.to_string());
            }
        }
        match resolved {
            Resolved::Take(text) => blocks.push(text),
            Resolved::Remove => {}
            Resolved::Conflict { ours, theirs } => {
                conflicts.push(Conflict {
                    key: key.to_string(),
                    description: conflict_description(key, &ours, &theirs),
                });
                blocks.push(conflict_block(&ours, &theirs));
            }
        }
    }

    let mut merged = blocks.join("\n\n");
    if !merged.is_empty() {
        merged.push('\n');
    }
    MergeResult {
        merged,
        conflicts,
        changed_symbols: changed.into_iter().collect(),
        fell_back: false,
    }
}

/// Resolve one item across the three sides. Returns the decision and whether it
/// differs from base (a "change" worth reporting as a changed symbol).
fn resolve(
    base: Option<&Item>,
    ours: Option<&Item>,
    theirs: Option<&Item>,
) -> (Resolved, bool) {
    let eq = |a: &Item, b: &Item| a.norm == b.norm;
    match (base, ours, theirs) {
        // Present on all three.
        (Some(b), Some(o), Some(t)) => {
            if eq(o, t) {
                (Resolved::Take(o.raw.clone()), !eq(o, b))
            } else if eq(o, b) {
                (Resolved::Take(t.raw.clone()), true) // theirs modified
            } else if eq(t, b) {
                (Resolved::Take(o.raw.clone()), true) // ours modified
            } else {
                (conflict(Some(o), Some(t)), true) // both modified differently
            }
        }
        // In base + ours, theirs removed.
        (Some(b), Some(o), None) => {
            if eq(o, b) {
                (Resolved::Remove, true) // ours unchanged, theirs deleted
            } else {
                (conflict(Some(o), None), true) // ours modified vs theirs deleted
            }
        }
        // In base + theirs, ours removed.
        (Some(b), None, Some(t)) => {
            if eq(t, b) {
                (Resolved::Remove, true)
            } else {
                (conflict(None, Some(t)), true)
            }
        }
        // Removed by both.
        (Some(_), None, None) => (Resolved::Remove, true),
        // Added by both.
        (None, Some(o), Some(t)) => {
            if eq(o, t) {
                (Resolved::Take(o.raw.clone()), true)
            } else {
                (conflict(Some(o), Some(t)), true)
            }
        }
        // Added by one side only.
        (None, Some(o), None) => (Resolved::Take(o.raw.clone()), true),
        (None, None, Some(t)) => (Resolved::Take(t.raw.clone()), true),
        (None, None, None) => (Resolved::Remove, false),
    }
}

fn conflict(ours: Option<&Item>, theirs: Option<&Item>) -> Resolved {
    Resolved::Conflict {
        ours: ours.map(|item| item.raw.clone()),
        theirs: theirs.map(|item| item.raw.clone()),
    }
}

fn conflict_block(ours: &Option<String>, theirs: &Option<String>) -> String {
    format!(
        "<<<<<<< ours\n{}\n=======\n{}\n>>>>>>> theirs",
        ours.as_deref().unwrap_or(""),
        theirs.as_deref().unwrap_or("")
    )
}

fn conflict_description(key: &str, ours: &Option<String>, theirs: &Option<String>) -> String {
    let what = decl_name(key)
        .map(|name| format!("declaration `{name}`"))
        .unwrap_or_else(|| "a top-level item".to_string());
    match (ours.is_some(), theirs.is_some()) {
        (true, true) => format!("both sides changed {what}"),
        (true, false) => format!("ours changed {what} but theirs deleted it"),
        (false, true) => format!("theirs changed {what} but ours deleted it"),
        (false, false) => format!("conflicting change to {what}"),
    }
}

/// The declaration name for a `decl:<name>` key.
fn decl_name(key: &str) -> Option<&str> {
    key.strip_prefix("decl:")
}

/// Coarse, safe fallback: never fabricate a merge. Resolve only the trivial
/// cases; otherwise emit a single whole-file conflict block.
fn fallback_merge(base: &str, ours: &str, theirs: &str) -> MergeResult {
    let (nb, no, nt) = (normalize(base), normalize(ours), normalize(theirs));
    let trivial = if no == nt {
        Some(ours.to_string())
    } else if no == nb {
        Some(theirs.to_string())
    } else if nt == nb {
        Some(ours.to_string())
    } else {
        None
    };
    match trivial {
        Some(merged) => MergeResult {
            merged,
            conflicts: Vec::new(),
            changed_symbols: Vec::new(),
            fell_back: true,
        },
        None => MergeResult {
            merged: conflict_block(&Some(ours.to_string()), &Some(theirs.to_string())),
            conflicts: vec![Conflict {
                key: "<file>".to_string(),
                description: "unparseable file changed on both sides".to_string(),
            }],
            changed_symbols: Vec::new(),
            fell_back: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disjoint_additions_merge_cleanly() {
        let base = "export function a() { return 1 }\n";
        let ours = "export function a() { return 1 }\nexport function b() { return 2 }\n";
        let theirs = "export function a() { return 1 }\nexport function c() { return 3 }\n";
        let result = merge(base, ours, theirs);
        assert!(result.is_clean(), "conflicts: {:?}", result.conflicts);
        assert!(result.merged.contains("function b"));
        assert!(result.merged.contains("function c"));
    }

    #[test]
    fn one_sided_modification_takes_that_side() {
        let base = "export const x = 1\n";
        let ours = "export const x = 42\n";
        let theirs = "export const x = 1\n";
        let result = merge(base, ours, theirs);
        assert!(result.is_clean());
        assert!(result.merged.contains("42"));
        assert_eq!(result.changed_symbols, vec!["x".to_string()]);
    }

    #[test]
    fn same_decl_modified_both_sides_conflicts() {
        let base = "export const x = 1\n";
        let ours = "export const x = 2\n";
        let theirs = "export const x = 3\n";
        let result = merge(base, ours, theirs);
        assert!(!result.is_clean());
        assert_eq!(result.conflicts.len(), 1);
        assert!(result.merged.contains("<<<<<<< ours"));
        assert!(result.merged.contains(">>>>>>> theirs"));
    }

    #[test]
    fn pure_reorder_is_not_a_conflict() {
        let base = "export const a = 1\nexport const b = 2\n";
        let ours = "export const b = 2\nexport const a = 1\n"; // reordered
        let theirs = "export const a = 1\nexport const b = 2\n";
        let result = merge(base, ours, theirs);
        assert!(result.is_clean(), "conflicts: {:?}", result.conflicts);
    }

    #[test]
    fn formatting_only_change_is_not_a_modification() {
        let base = "export const x = 1\n";
        let ours = "export   const    x = 1\n"; // whitespace only
        let theirs = "export const x = 99\n"; // real change
        let result = merge(base, ours, theirs);
        // ours is whitespace-equal to base ⇒ theirs' real change wins, no conflict.
        assert!(result.is_clean(), "conflicts: {:?}", result.conflicts);
        assert!(result.merged.contains("99"));
    }

    #[test]
    fn deletion_vs_unchanged_removes() {
        let base = "export const a = 1\nexport const b = 2\n";
        let ours = "export const a = 1\n"; // deleted b
        let theirs = "export const a = 1\nexport const b = 2\n";
        let result = merge(base, ours, theirs);
        assert!(result.is_clean());
        assert!(!result.merged.contains("b = 2"));
    }

    #[test]
    fn deletion_vs_modification_conflicts() {
        let base = "export const b = 2\n";
        let ours = ""; // deleted
        let theirs = "export const b = 22\n"; // modified
        let result = merge(base, ours, theirs);
        assert!(!result.is_clean());
    }

    #[test]
    fn unparseable_falls_back_safely() {
        let base = "const x = (";
        let ours = "const x = ("; // unchanged
        let theirs = "const x = 1\n"; // theirs fixed it
        let result = merge(base, ours, theirs);
        assert!(result.fell_back);
        assert!(result.is_clean());
        assert!(result.merged.contains("x = 1"));
    }

    #[test]
    fn imports_merge_as_items() {
        let base = "import { a } from './a'\nexport const z = 1\n";
        let ours = "import { a } from './a'\nimport { b } from './b'\nexport const z = 1\n";
        let theirs = "import { a } from './a'\nexport const z = 1\n";
        let result = merge(base, ours, theirs);
        assert!(result.is_clean());
        assert!(result.merged.contains("from './b'"));
    }
}
