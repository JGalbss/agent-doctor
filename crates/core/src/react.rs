//! React tier: the full react-doctor rule set, merged in automatically. When
//! the scan root looks like a React project, we shell out to the `react-doctor`
//! CLI (`--json`) and fold its diagnostics into our report as `rd/*` rules — the
//! same "orchestrate, never reimplement" approach as the `--deep` language-service
//! tier. react-doctor stays the source of truth for its ~500 rules.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::diagnostics::{Category, Diagnostic, Severity};

#[derive(Deserialize)]
struct ReactReport {
    #[serde(default)]
    diagnostics: Vec<ReactDiagnostic>,
}

#[derive(Deserialize)]
struct ReactDiagnostic {
    #[serde(rename = "filePath")]
    file_path: String,
    rule: String,
    severity: String,
    message: String,
    #[serde(default)]
    title: Option<String>,
    line: u32,
    column: u32,
}

fn map_severity(severity: &str) -> Severity {
    match severity {
        "error" => Severity::Error,
        "warning" => Severity::Warn,
        _ => Severity::Info,
    }
}

/// `Box::leak` for the rule id / help: react-doctor rule names arrive at runtime
/// but the Diagnostic schema uses `&'static str`. The set is bounded by the rule
/// catalog and lives for the process — leaking is the right call (mirrors the
/// `--deep` tier).
fn leak(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

/// True when the nearest package.json declares a `react` dependency — checked at
/// the scan root and one level of common workspace dirs (mirrors
/// [`crate::engine::detect_effect_major`]).
pub fn detect_react(root: &Path) -> bool {
    if package_has_react(&root.join("package.json")) {
        return true;
    }
    for workspace_dir in ["packages", "apps", "libs", "services"] {
        let Ok(entries) = fs::read_dir(root.join(workspace_dir)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            if package_has_react(&entry.path().join("package.json")) {
                return true;
            }
        }
    }
    false
}

fn package_has_react(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|table| {
            manifest
                .get(*table)
                .and_then(|deps| deps.get("react"))
                .is_some()
        })
}

/// Run react-doctor over `root` and return its findings as our [`Diagnostic`]s.
/// Errors (CLI missing, unparsable output) are returned to the caller, which
/// treats the tier as a silent no-op so a missing react-doctor never breaks the
/// core scan.
pub fn run_react_doctor(root: &Path) -> Result<Vec<Diagnostic>, String> {
    let output = Command::new("npx")
        .current_dir(root)
        .args(["--no-install", "react-doctor", ".", "--json", "--lint"])
        .output()
        .map_err(|error| format!("failed to run npx: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The CLI exits non-zero when it finds issues — only empty/unparsable
    // output is a real failure (e.g. react-doctor not installed).
    let report: ReactReport = serde_json::from_str(stdout.trim())
        .map_err(|_| "could not run react-doctor (install it: `npm i -D react-doctor`)".to_string())?;

    let root_display = root.to_string_lossy().into_owned();
    Ok(report
        .diagnostics
        .into_iter()
        .map(|diagnostic| {
            let file = diagnostic
                .file_path
                .strip_prefix(&root_display)
                .map(|stripped| stripped.trim_start_matches('/').to_string())
                .unwrap_or(diagnostic.file_path);
            let file_context = crate::lint::classify_file(&file);
            let help = diagnostic
                .title
                .map(|title| leak(format!("react-doctor: {title}")))
                .unwrap_or("React finding from react-doctor.");
            Diagnostic {
                rule: leak(format!("rd/{}", diagnostic.rule)),
                severity: map_severity(&diagnostic.severity),
                category: Category::React,
                message: diagnostic.message,
                help,
                file,
                file_context,
                line: diagnostic.line,
                column: diagnostic.column,
                snippet: String::new(),
            }
        })
        .collect())
}
