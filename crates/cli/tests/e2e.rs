//! End-to-end tests that build and run the actual `agent-doctor` binary against
//! real git repositories — covering the toolkit subcommands that unit tests
//! can't reach (process exit codes, the git merge driver, scaffolding).

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
fn gate_denies_protected_and_passes_clean() {
    let dir = temp_dir("gate");
    init_repo(&dir);
    write(&dir, "src/app.ts", "export const a = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/app.ts", "export const a = 2\n");
    git(&dir, &["commit", "-qam", "change"]);

    write(&dir, "deny.toml", "[protected]\nglobs = [\"src/app.ts\"]\n");
    let denied = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "deny.toml"])
        .output()
        .unwrap();
    assert!(!denied.status.success(), "expected non-zero exit on deny");

    write(&dir, "ok.toml", "[protected]\nglobs = [\"other/**\"]\n");
    let passed = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "ok.toml"])
        .output()
        .unwrap();
    assert!(passed.status.success(), "expected zero exit when clean");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gate_flags_layering_violation() {
    let dir = temp_dir("layer");
    init_repo(&dir);
    write(&dir, "src/ui/button.ts", "export const b = 1\n");
    write(&dir, "src/core/engine.ts", "export const e = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/core/engine.ts", "import { b } from '../ui/button'\nexport const e = b\n");
    git(&dir, &["commit", "-qam", "bad import"]);
    write(
        &dir,
        "layer.toml",
        "[[layer]]\nname = \"core\"\npath = \"src/core/**\"\nforbid_imports_from = [\"src/ui/**\"]\n",
    );
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--policy", "layer.toml", "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "layering violation must fail the gate");
    assert!(String::from_utf8_lossy(&out.stdout).contains("layering"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gate_enforces_leases_per_actor() {
    let dir = temp_dir("lease");
    init_repo(&dir);
    write(&dir, "src/auth/login.ts", "export const a = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/auth/login.ts", "export const a = 2\n");
    git(&dir, &["commit", "-qam", "change"]);
    write(
        &dir,
        "leases.json",
        "{\"leases\":[{\"actor\":\"agent-a\",\"task_id\":\"t1\",\"globs\":[\"src/auth/**\"]}]}",
    );

    let intruder = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--actor", "agent-b", "--leases", "leases.json"])
        .output()
        .unwrap();
    assert!(!intruder.status.success(), "agent-b is outside the lease");

    let owner = Command::new(BIN)
        .current_dir(&dir)
        .args(["gate", "--base", "HEAD~1", "--actor", "agent-a", "--leases", "leases.json"])
        .output()
        .unwrap();
    assert!(owner.status.success(), "agent-a owns the region");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn verify_passes_clean_and_blocks_on_policy() {
    let dir = temp_dir("verify");
    init_repo(&dir);
    write(&dir, "src/a.ts", "export const x = 1\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    write(&dir, "src/a.ts", "export const x = 2\n");
    git(&dir, &["commit", "-qam", "change"]);

    let ok = Command::new(BIN)
        .current_dir(&dir)
        .args(["verify", "--base", "HEAD~1"])
        .output()
        .unwrap();
    assert!(ok.status.success(), "clean diff should pass the gate");

    write(&dir, "deny.toml", "[protected]\nglobs = [\"src/a.ts\"]\n");
    let blocked = Command::new(BIN)
        .current_dir(&dir)
        .args(["verify", "--base", "HEAD~1", "--policy", "deny.toml"])
        .output()
        .unwrap();
    assert!(!blocked.status.success(), "policy violation must block verify");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn merge_driver_auto_resolves_additive_conflict() {
    let dir = temp_dir("merge");
    init_repo(&dir);
    write(&dir, "f.ts", "export function a() { return 1 }\n");
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);

    git(&dir, &["checkout", "-q", "-b", "feature"]);
    write(&dir, "f.ts", "export function a() { return 1 }\nexport function b() { return 2 }\n");
    git(&dir, &["commit", "-qam", "add b"]);
    git(&dir, &["checkout", "-q", "main"]);
    write(&dir, "f.ts", "export function a() { return 1 }\nexport function c() { return 3 }\n");
    git(&dir, &["commit", "-qam", "add c"]);

    git(&dir, &["config", "merge.ad.name", "agent-doctor"]);
    git(&dir, &["config", "merge.ad.driver", &format!("{BIN} merge %O %A %B")]);
    write(&dir, ".gitattributes", "*.ts merge=ad\n");
    git(&dir, &["add", ".gitattributes"]);
    git(&dir, &["commit", "-qm", "attrs"]);

    let merged = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["merge", "feature", "-m", "merge"])
        .output()
        .unwrap();
    assert!(merged.status.success(), "additive merge should be clean");
    let result = std::fs::read_to_string(dir.join("f.ts")).unwrap();
    assert!(result.contains("function b") && result.contains("function c"));
    assert!(!result.contains("<<<<<<<"), "no conflict markers");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn merge_real_conflict_writes_markers_and_exits_nonzero() {
    let dir = temp_dir("conflict");
    write(&dir, "base.ts", "export const x = 1\n");
    write(&dir, "ours.ts", "export const x = 2\n");
    write(&dir, "theirs.ts", "export const x = 3\n");
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["merge", "base.ts", "ours.ts", "theirs.ts", "--output", "out.ts"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "same-decl conflict must fail");
    assert!(std::fs::read_to_string(dir.join("out.ts")).unwrap().contains("<<<<<<<"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_scaffolds_and_is_idempotent() {
    let dir = temp_dir("init");
    init_repo(&dir);
    let first = Command::new(BIN).current_dir(&dir).arg("init").output().unwrap();
    assert!(first.status.success());
    assert!(dir.join("agent-doctor.policy.toml").exists());
    assert!(dir.join(".agent-doctor/.gitignore").exists());
    assert!(dir.join(".gitattributes").exists());

    let driver = Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["config", "--get", "merge.agent-doctor.driver"])
        .output()
        .unwrap();
    assert!(driver.status.success() && !driver.stdout.is_empty());

    let second = Command::new(BIN).current_dir(&dir).arg("init").output().unwrap();
    assert!(second.status.success());
    let attrs = std::fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert_eq!(attrs.matches("*.ts merge=agent-doctor").count(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_installs_skill_and_hook_with_flags() {
    let dir = temp_dir("skills");
    init_repo(&dir);
    let out = Command::new(BIN)
        .current_dir(&dir)
        .args(["init", "--skills", "--hooks"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let skill = dir.join(".claude/skills/agent-doctor/SKILL.md");
    assert!(skill.exists(), "skill file installed");
    assert!(std::fs::read_to_string(&skill).unwrap().contains("name: agent-doctor"));
    assert!(dir.join(".git/hooks/pre-push").exists(), "pre-push hook installed");
    std::fs::remove_dir_all(&dir).ok();
}
