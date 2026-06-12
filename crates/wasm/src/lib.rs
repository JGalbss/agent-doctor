//! Browser build of the linter for the docs-site playground.

use effect_doctor_core::{compute_score, lint_source_with};
use wasm_bindgen::prelude::*;

/// Lint a source string, returning `{ diagnostics, score }` as JSON.
#[wasm_bindgen]
pub fn lint(source: &str, v4: bool, adopt: bool) -> String {
    let diagnostics = lint_source_with("playground.ts", source, v4, adopt);
    let score = compute_score(&diagnostics);
    serde_json::json!({ "diagnostics": diagnostics, "score": score }).to_string()
}
