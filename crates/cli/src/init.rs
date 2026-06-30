//! `agent-doctor init` — an interactive walkthrough that detects the project's
//! design system and the primitive UI libraries it wraps, then writes an
//! `agent-doctor.toml`. Runnable straight from npm: `npx @jgalbsss/agent-doctor init`.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// UI primitive libraries a design system typically wraps. Detected by dependency
/// prefix; any present become `forbid-import-prefixes` so app code routes through
/// the design system instead.
const KNOWN_PRIMITIVES: &[&str] = &[
    "@radix-ui/",
    "@base-ui/",
    "@base-ui-components/",
    "@mui/",
    "@emotion/",
    "@chakra-ui/",
    "@headlessui/",
    "styled-components",
    "class-variance-authority",
    "tailwind-variants",
];

struct Pkg {
    name: String,
    dep_prefixes: Vec<String>,
    component_exports: usize,
}

impl Pkg {
    fn depends_on_primitive(&self) -> bool {
        self.dep_prefixes
            .iter()
            .any(|dep| KNOWN_PRIMITIVES.iter().any(|p| dep.starts_with(p)))
    }

    fn name_looks_like_ui(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        n.ends_with("/ui") || n.ends_with("-ui") || n.contains("design-system") || n.contains("/components")
    }
}

/// `agent-doctor init`: detect + write config.
pub fn run(root: &Path, force: bool, yes: bool) -> ExitCode {
    let target = root.join("agent-doctor.toml");
    if target.exists() && !force {
        eprintln!("agent-doctor init: agent-doctor.toml already exists (use --force to overwrite)");
        return ExitCode::from(1);
    }

    let packages = collect_packages(root);
    let mut design_system = detect_design_system(&packages);
    let mut forbidden = detect_forbidden_prefixes(&packages);

    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal() && !yes;

    println!();
    println!("  agent-doctor init");
    println!();

    if interactive {
        design_system = confirm_design_system(design_system);
        forbidden = confirm_forbidden(forbidden);
    } else {
        match &design_system {
            Some(name) => println!("  design system: {name} (detected)"),
            None => println!("  design system: none detected (skipping [design-system])"),
        }
        if !forbidden.is_empty() {
            println!("  primitive libraries: {}", forbidden.join(", "));
        }
    }

    let strict = match interactive {
        true => prompt_yes_no("Enable the strict agent gate (errors + non-zero exit, for CI)?", false),
        false => false,
    };

    let contents = render_config(design_system.as_deref(), &forbidden, strict);
    if let Err(error) = std::fs::write(&target, contents) {
        eprintln!("agent-doctor init: could not write {}: {error}", target.display());
        return ExitCode::from(2);
    }

    println!();
    println!("  wrote {}", target.display());
    println!("  run `agent-doctor` to scan, or `agent-doctor explain ds-no-banned-import`.");
    println!();
    ExitCode::SUCCESS
}

/// Read package.json at the root plus one and two levels under `packages/` and
/// `apps/` (covers flat and nested workspaces).
fn collect_packages(root: &Path) -> Vec<Pkg> {
    let mut manifests: Vec<PathBuf> = vec![root.join("package.json")];
    for workspace in ["packages", "apps"] {
        let base = root.join(workspace);
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok).map(|e| e.path()) {
            if !entry.is_dir() {
                continue;
            }
            manifests.push(entry.join("package.json"));
            // one more level (e.g. packages/slides/*, apps/excel/*)
            if let Ok(nested) = std::fs::read_dir(&entry) {
                for child in nested.filter_map(Result::ok).map(|e| e.path()) {
                    if child.is_dir() {
                        manifests.push(child.join("package.json"));
                    }
                }
            }
        }
    }
    manifests.iter().filter_map(|path| read_pkg(path)).collect()
}

fn read_pkg(path: &Path) -> Option<Pkg> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = value.get("name")?.as_str()?.to_string();
    let mut dep_prefixes = Vec::new();
    for table in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(map) = value.get(table).and_then(|v| v.as_object()) {
            dep_prefixes.extend(map.keys().cloned());
        }
    }
    let component_exports = value
        .get("exports")
        .and_then(|v| v.as_object())
        .map(|exports| {
            exports
                .keys()
                .filter(|key| {
                    key.strip_prefix("./")
                        .is_some_and(|sub| !sub.is_empty() && !sub.contains('/'))
                })
                .count()
        })
        .unwrap_or(0);
    Some(Pkg {
        name,
        dep_prefixes,
        component_exports,
    })
}

/// The design system is the workspace package with the most component-style
/// exports that also wraps a primitive library (or is named like a UI package).
/// That distinguishes a real component library from a utils package that happens
/// to have many exports.
fn detect_design_system(packages: &[Pkg]) -> Option<String> {
    packages
        .iter()
        .filter(|pkg| pkg.component_exports >= 3 && (pkg.depends_on_primitive() || pkg.name_looks_like_ui()))
        .max_by_key(|pkg| pkg.component_exports)
        .map(|pkg| pkg.name.clone())
}

fn detect_forbidden_prefixes(packages: &[Pkg]) -> Vec<String> {
    KNOWN_PRIMITIVES
        .iter()
        .filter(|primitive| {
            packages
                .iter()
                .flat_map(|pkg| pkg.dep_prefixes.iter())
                .any(|dep| dep.starts_with(*primitive))
        })
        .map(|primitive| primitive.to_string())
        .collect()
}

fn confirm_design_system(detected: Option<String>) -> Option<String> {
    match detected {
        Some(name) => {
            if prompt_yes_no(&format!("Use `{name}` as your design system?"), true) {
                return Some(name);
            }
            prompt_line("Design-system package name (blank to skip):")
        }
        None => {
            println!("  no design-system package detected.");
            prompt_line("Design-system package name (blank to skip):")
        }
    }
}

fn confirm_forbidden(detected: Vec<String>) -> Vec<String> {
    if detected.is_empty() {
        return detected;
    }
    let shown = detected.join(", ");
    if prompt_yes_no(&format!("Route these through the design system: {shown}?"), true) {
        return detected;
    }
    Vec::new()
}

fn render_config(design_system: Option<&str>, forbidden: &[String], strict: bool) -> String {
    let mut out = String::new();
    out.push_str("# agent-doctor — https://github.com/JGalbss/agent-doctor\n");
    out.push_str("# Generated by `agent-doctor init`.\n\n");

    out.push_str("[tiers]\n");
    out.push_str("agent = true\n");
    match strict {
        true => out.push_str("agent_strict = true\n"),
        false => out.push_str("# agent_strict = true  # escalate to errors / non-zero exit (CI gate)\n"),
    }
    out.push('\n');

    match design_system {
        Some(package) => {
            out.push_str("[design-system]\n");
            out.push_str(&format!("package = \"{package}\"\n"));
            let list = forbidden
                .iter()
                .map(|prefix| format!("\"{prefix}\""))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("forbid-import-prefixes = [{list}]\n"));
        }
        None => {
            out.push_str("# [design-system]\n");
            out.push_str("# package = \"@your-scope/ui\"\n");
            out.push_str("# forbid-import-prefixes = [\"@radix-ui/\", \"@base-ui/\"]\n");
        }
    }
    out
}

fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("  {question} {hint} ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return default_yes;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    }
}

fn prompt_line(prompt: &str) -> Option<String> {
    print!("  {prompt} ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return None;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}
