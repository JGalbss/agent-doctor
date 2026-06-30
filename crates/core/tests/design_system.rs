//! Design-system enforcement (`[design-system]` config). Drives the real scan
//! over a temp repo with a fake DS package so catalog discovery + the import
//! rule are exercised end to end.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use agent_doctor_core::{scan, Diagnostic, ScanOptions, ScanScope};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ad-ds-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, contents: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// Lay down a fake `@acme/ui` design system exporting `select` + `button`.
fn write_design_system(dir: &Path) {
    write(
        dir,
        "node_modules/@acme/ui/package.json",
        r#"{ "name": "@acme/ui", "exports": { "./select": "./select.tsx", "./button": "./button.tsx" } }"#,
    );
}

fn ds_findings(dir: &Path) -> Vec<Diagnostic> {
    let result = scan(&ScanOptions {
        root: dir.to_path_buf(),
        migrate: false,
        scope: ScanScope::Full,
        base: None,
        deep: false,
        adopt: false,
        agent: false,
        agent_strict: false,
        react: false,
    })
    .unwrap();
    result
        .diagnostics
        .into_iter()
        .filter(|d| d.rule == "ds-no-banned-import")
        .collect()
}

#[test]
fn flags_banned_import_and_suggests_catalog_component() {
    let dir = temp_dir();
    write_design_system(&dir);
    write(
        &dir,
        "agent-doctor.toml",
        "[design-system]\npackage = \"@acme/ui\"\nforbid-import-prefixes = [\"@radix-ui/\"]\n",
    );
    write(&dir, "src/Picker.tsx", "import * as Select from \"@radix-ui/react-select\"\nexport const Picker = () => null\n");

    let findings = ds_findings(&dir);
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0].message.contains("@acme/ui/select"),
        "should suggest the catalog component, got: {}",
        findings[0].message
    );
}

#[test]
fn using_the_design_system_is_clean() {
    let dir = temp_dir();
    write_design_system(&dir);
    write(
        &dir,
        "agent-doctor.toml",
        "[design-system]\npackage = \"@acme/ui\"\nforbid-import-prefixes = [\"@radix-ui/\"]\n",
    );
    write(&dir, "src/Picker.tsx", "import { Select } from \"@acme/ui/select\"\nexport const Picker = () => null\n");
    assert_eq!(ds_findings(&dir).len(), 0);
}

#[test]
fn no_config_means_no_design_system_findings() {
    let dir = temp_dir();
    write_design_system(&dir);
    write(&dir, "src/Picker.tsx", "import * as Select from \"@radix-ui/react-select\"\n");
    assert_eq!(ds_findings(&dir).len(), 0, "feature is opt-in via config");
}

#[test]
fn the_design_system_package_is_not_flagged_against_itself() {
    let dir = temp_dir();
    write_design_system(&dir);
    write(
        &dir,
        "agent-doctor.toml",
        "[design-system]\npackage = \"@acme/ui\"\nforbid-import-prefixes = [\"@radix-ui/\"]\n",
    );
    // The DS's own source legitimately wraps the primitive.
    write(&dir, "packages/ui/src/select.tsx", "import * as Select from \"@radix-ui/react-select\"\n");
    assert_eq!(ds_findings(&dir).len(), 0, "the DS dir must be exempt");
}
