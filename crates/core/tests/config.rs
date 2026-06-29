//! Workspace config (`agent-doctor.toml`) + tsconfig strict inheritance. These
//! need the real scan pipeline (filesystem), so they drive `scan` over a temp dir.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use agent_doctor_core::{scan, ScanOptions, ScanScope, Severity};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ad-config-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, name: &str, contents: &str) {
    std::fs::write(dir.join(name), contents).unwrap();
}

fn options(dir: &std::path::Path) -> ScanOptions {
    ScanOptions {
        root: dir.to_path_buf(),
        migrate: false,
        scope: ScanScope::Full,
        base: None,
        deep: false,
        adopt: false,
        agent: false,
        agent_strict: false,
        react: false,
    }
}

fn has_rule(dir: &std::path::Path, rule: &str) -> bool {
    scan(&options(dir))
        .unwrap()
        .diagnostics
        .iter()
        .any(|d| d.rule == rule)
}

#[test]
fn config_pins_severity() {
    let dir = temp_dir();
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nexport function p(x: any) { return x }\n");
    write(&dir, "agent-doctor.toml", "[rules]\nno-explicit-any = \"error\"\n");
    let any = scan(&options(&dir))
        .unwrap()
        .diagnostics
        .into_iter()
        .find(|d| d.rule == "no-explicit-any")
        .expect("any finding");
    assert_eq!(any.severity, Severity::Error);
}

#[test]
fn config_turns_rule_off() {
    let dir = temp_dir();
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nexport function p(x: any) { return x }\n");
    assert!(has_rule(&dir, "no-explicit-any"));
    write(&dir, "agent-doctor.toml", "[rules]\nno-explicit-any = \"off\"\n");
    assert!(!has_rule(&dir, "no-explicit-any"), "off should drop the rule");
}

#[test]
fn config_enables_agent_tier_by_default() {
    let dir = temp_dir();
    // a default export only fires under the agent tier.
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nconst x = 1\nexport default x\n");
    assert!(!has_rule(&dir, "agent-no-default-export"), "off by default");
    write(&dir, "agent-doctor.toml", "[tiers]\nagent = true\n");
    assert!(has_rule(&dir, "agent-no-default-export"), "config enables --agent");
}

#[test]
fn non_strict_tsconfig_is_flagged() {
    let dir = temp_dir();
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nexport const x = 1\n");
    write(&dir, "tsconfig.json", "{ \"compilerOptions\": { \"strict\": false } }\n");
    assert!(has_rule(&dir, "prefer-strict-tsconfig"));
}

#[test]
fn strict_tsconfig_is_silent() {
    let dir = temp_dir();
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nexport const x = 1\n");
    write(&dir, "tsconfig.json", "{ \"compilerOptions\": { \"strict\": true } }\n");
    assert!(!has_rule(&dir, "prefer-strict-tsconfig"));
}

#[test]
fn no_tsconfig_is_silent() {
    let dir = temp_dir();
    write(&dir, "a.ts", "import { Effect } from \"effect\"\nexport const x = 1\n");
    assert!(!has_rule(&dir, "prefer-strict-tsconfig"));
}
