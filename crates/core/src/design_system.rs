//! Design-system enforcement. When `agent-doctor.toml` names a `[design-system]`
//! package, agent-doctor makes sure agents *use* it instead of reaching for the
//! raw primitive libraries it wraps. The component catalog is auto-discovered
//! from the package's `exports` map (the durable source of truth — it can't go
//! stale because it *is* the package); there is no manifest to maintain.
//!
//! Self-contained engine pass (like `react.rs` / `tsconfig.rs`): it parses files
//! itself and runs only when configured, so it is zero-cost otherwise and needs
//! no changes to the per-file rule engine.

use std::collections::BTreeSet;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::config::DesignSystem;
use crate::diagnostics::{Category, Diagnostic, RuleMeta, Severity};
use crate::lint::classify_file;
use crate::text::LineIndex;

static NO_BANNED_IMPORT: RuleMeta = RuleMeta {
    id: "ds-no-banned-import",
    severity: Severity::Warn,
    category: Category::DesignSystem,
    help: "This pulls in a primitive library that the design system already wraps. Import the design-system component instead of re-reaching for the underlying primitive — that's the whole point of having a design system.",
};

/// Metas for the design-system pass (engine-level, not per-file Rule dispatch).
pub fn design_system_metas() -> &'static [&'static RuleMeta] {
    static METAS: &[&RuleMeta] = &[&NO_BANNED_IMPORT];
    METAS
}

/// The design system's component catalog: the single-segment subpaths it exports
/// (e.g. `button`, `select`, `dialog`), discovered from its `package.json`.
pub struct Catalog {
    components: BTreeSet<String>,
}

impl Catalog {
    fn empty() -> Self {
        Catalog {
            components: BTreeSet::new(),
        }
    }

    /// The design-system subpath that replaces a banned import, if the catalog
    /// has one — e.g. `@radix-ui/react-select` → `select`.
    fn replacement_for(&self, import_source: &str) -> Option<&str> {
        let leaf = import_source.rsplit('/').next().unwrap_or(import_source);
        let bare = leaf.trim_start_matches("react-");
        self.components
            .iter()
            .find(|component| component.as_str() == bare)
            .map(String::as_str)
    }
}

/// Read the design-system package's `exports` and collect its single-segment
/// subpaths as the component catalog. Resolves the package from node_modules
/// first, then workspace `packages/*` and `apps/*` by name. An undiscoverable
/// package yields an empty catalog (the rule still fires, just with a generic
/// suggestion).
pub fn discover_catalog(root: &Path, package: &str) -> Catalog {
    let Some(manifest) = read_package_manifest(root, package) else {
        return Catalog::empty();
    };
    let Some(exports) = manifest.get("exports").and_then(|value| value.as_object()) else {
        return Catalog::empty();
    };
    let components = exports
        .keys()
        .filter_map(|key| key.strip_prefix("./"))
        .filter(|subpath| !subpath.contains('/') && !subpath.is_empty())
        .map(|subpath| subpath.to_string())
        .collect();
    Catalog { components }
}

fn read_package_manifest(root: &Path, package: &str) -> Option<serde_json::Value> {
    let node_modules = root.join("node_modules").join(package).join("package.json");
    if let Ok(text) = std::fs::read_to_string(&node_modules) {
        return serde_json::from_str(&text).ok();
    }
    for workspace in ["packages", "apps"] {
        let Ok(entries) = std::fs::read_dir(root.join(workspace)) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let manifest_path = entry.path().join("package.json");
            let Ok(text) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if value.get("name").and_then(|name| name.as_str()) == Some(package) {
                return Some(value);
            }
        }
    }
    None
}

/// Scan `files` for imports that should route through the design system instead.
pub fn run(root: &Path, config: &DesignSystem, files: &[std::path::PathBuf]) -> Vec<Diagnostic> {
    if config.forbid_import_prefixes.is_empty() {
        return Vec::new();
    }
    let catalog = discover_catalog(root, &config.package);
    let mut findings = Vec::new();
    for path in files {
        // Never flag the design system's own source for using its primitives.
        if is_inside_design_system(path, &config.package) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        if !config
            .forbid_import_prefixes
            .iter()
            .any(|prefix| source.contains(prefix))
        {
            continue;
        }
        let display_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        collect_file(&display_path, &source, config, &catalog, &mut findings);
    }
    findings
}

fn is_inside_design_system(path: &Path, package: &str) -> bool {
    // The unscoped package name (`@acme/ui` → `ui`) is the workspace dir.
    let dir = package.rsplit('/').next().unwrap_or(package);
    path.components().any(|component| {
        component.as_os_str().to_str() == Some(dir)
            || component.as_os_str().to_str() == Some(package)
    })
}

fn collect_file(
    display_path: &str,
    source: &str,
    config: &DesignSystem,
    catalog: &Catalog,
    findings: &mut Vec<Diagnostic>,
) {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(Path::new(display_path)).unwrap_or_else(|_| SourceType::tsx());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return;
    }
    let lines = LineIndex::new(source);
    for statement in &parsed.program.body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };
        let import_source = import.source.value.as_str();
        let Some(prefix) = config
            .forbid_import_prefixes
            .iter()
            .find(|prefix| import_source.starts_with(prefix.as_str()))
        else {
            continue;
        };
        let (line, column) = lines.line_col(import.span.start as usize);
        let suggestion = match catalog.replacement_for(import_source) {
            Some(component) => format!("{}/{}", config.package, component),
            None => config.package.clone(),
        };
        findings.push(Diagnostic {
            rule: NO_BANNED_IMPORT.id,
            severity: NO_BANNED_IMPORT.severity,
            category: NO_BANNED_IMPORT.category,
            message: format!(
                "`{import_source}` bypasses the design system ({prefix}) — import from {suggestion} instead"
            ),
            help: NO_BANNED_IMPORT.help,
            file: display_path.to_string(),
            file_context: classify_file(display_path),
            line,
            column,
            snippet: String::new(),
        });
    }
}
