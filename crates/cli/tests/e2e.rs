//! End-to-end tests that build and run the actual `agent-doctor` binary against
//! real files — covering the linter CLI surface that unit tests can't reach
//! (process exit codes, JSON output, diff scoping over a real git repo).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Path to the built binary, provided by cargo to integration tests.
const BIN: &str = env!("CARGO_BIN_EXE_agent-doctor");

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ad-e2e-{tag}-{}-{}", std::process::id(), unique));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.co"]);
    git(dir, &["config", "user.name", "t"]);
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

#[test]
fn rules_lists_catalog_and_json_is_valid() {
    let out = Command::new(BIN).args(["rules", "--json"]).output().unwrap();
    assert!(out.status.success(), "rules --json should succeed");
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(parsed.as_array().is_some_and(|catalog| !catalog.is_empty()));
}

#[test]
fn explain_known_rule_succeeds_and_unknown_fails() {
    let known = Command::new(BIN)
        .args(["explain", "require-yield-star"])
        .output()
        .unwrap();
    assert!(known.status.success(), "explain of a real rule should succeed");

    let unknown = Command::new(BIN)
        .args(["explain", "no-such-rule"])
        .output()
        .unwrap();
    assert!(!unknown.status.success(), "explain of an unknown rule should fail");
}

#[test]
fn scan_emits_json_report() {
    let dir = temp_dir("scan-json");
    write(&dir, "ok.ts", "export const x = 1\n");
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let _: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
}

#[test]
fn agent_strict_exits_nonzero_on_slop() {
    let dir = temp_dir("agent-strict");
    // a string-equality guard + if/else chain — classic non-Effect agent slop.
    // (the linter only analyzes files that import from `effect`.)
    write(
        &dir,
        "slop.ts",
        "import { Effect } from \"effect\"\nexport function pick(kind: string) {\n  if (kind === \"a\") { return 1 } else { return 2 }\n}\n",
    );
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["--agent-strict"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "--agent-strict must exit non-zero when slop is present"
    );
}

#[test]
fn max_file_length_fires_on_plain_typescript() {
    let dir = temp_dir("max-len");
    // 700 lines, no `effect` import — the length rule is import-independent.
    let body: String = (1..=700).map(|i| format!("export const v{i} = {i}\n")).collect();
    write(&dir, "big.ts", &body);

    let report = Command::new(BIN)
        .current_dir(&dir)
        .args(["--agent", "--json"])
        .output()
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    let fired = parsed["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["rule"] == "agent-max-file-length");
    assert!(fired, "the length rule should fire on a 700-line file");

    // --agent-strict turns it into a hard failure.
    let strict = Command::new(BIN)
        .current_dir(&dir)
        .args(["--agent-strict"])
        .output()
        .unwrap();
    assert!(!strict.status.success(), "--agent-strict must fail on an oversized file");
}

#[test]
fn no_react_flag_skips_the_react_tier() {
    let dir = temp_dir("no-react");
    // A React project (react in deps) — the tier would normally auto-run.
    write(
        &dir,
        "package.json",
        "{ \"name\": \"x\", \"dependencies\": { \"react\": \"^18.0.0\" } }",
    );
    write(&dir, "src/App.tsx", "export const App = () => null\n");

    // --no-react must deterministically skip react-doctor: success, no rd/* rules,
    // regardless of whether react-doctor happens to be installed.
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["--no-react", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let react_findings = parsed["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["rule"].as_str().is_some_and(|rule| rule.starts_with("rd/")))
        .count();
    assert_eq!(react_findings, 0, "--no-react must not emit rd/* findings");
}

#[test]
fn scope_changed_limits_to_diff() {
    let dir = temp_dir("scope-changed");
    init_repo(&dir);
    write(&dir, "clean.ts", "export const x = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "base"]);

    // Add a new file with slop; only it is in the diff vs HEAD.
    write(
        &dir,
        "new.ts",
        "import { Effect } from \"effect\"\nexport function pick(k: string) {\n  if (k === \"a\") { return 1 } else { return 2 }\n}\n",
    );
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["--scope", "changed", "--base", "HEAD", "--agent-strict"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "changed-scope scan should still see the new file's slop"
    );
}
