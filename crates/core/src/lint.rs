//! Single-source linting — the platform-independent heart of the engine,
//! shared by the CLI scan pipeline, the LSP, tests, and the WASM playground.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::diagnostics::{Diagnostic, FileContext, RawDiagnostic};
use crate::effect_imports::EffectImports;
use crate::runner::Runner;

/// Test files keep their diagnostics in the report but (mostly) out of the
/// score — deliberate rule-breaking is normal in tests.
pub(crate) fn classify_file(path: &str) -> FileContext {
    let lowered = path.to_ascii_lowercase();
    let is_test_path = lowered
        .split(['/', '\\'])
        .any(|segment| matches!(segment, "test" | "tests" | "__tests__" | "e2e"));
    if is_test_path
        || lowered.contains(".test.")
        || lowered.contains(".spec.")
        || lowered.contains("-test.")
    {
        return FileContext::Test;
    }
    FileContext::Production
}

/// Lint a single source text. Public so tests and the LSP host can lint
/// snippets without touching the filesystem.
pub fn lint_source(display_path: &str, source: &str, v4_active: bool) -> Vec<Diagnostic> {
    lint_source_with(display_path, source, v4_active, false)
}

/// `lint_source` with the experimental adoption rules toggled.
pub fn lint_source_with(
    display_path: &str,
    source: &str,
    v4_active: bool,
    adopt: bool,
) -> Vec<Diagnostic> {
    let allocator = Allocator::default();
    let source_type =
        SourceType::from_path(Path::new(display_path)).unwrap_or_else(|_| SourceType::ts());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return Vec::new();
    }
    let imports = EffectImports::from_program(&parsed.program);
    if !imports.has_any() {
        return Vec::new();
    }
    let ctx = Runner::new(imports, v4_active, adopt).run(&parsed.program);
    finalize(source, display_path, classify_file(display_path), ctx.raw)
}

/// Convert span-based raw diagnostics to line/column + source-line snippets.
fn finalize(
    source: &str,
    display_path: &str,
    file_context: FileContext,
    raw: Vec<RawDiagnostic>,
) -> Vec<Diagnostic> {
    if raw.is_empty() {
        return Vec::new();
    }
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        )
        .collect();

    raw.into_iter()
        .map(|diagnostic| {
            let offset = diagnostic.span.start as usize;
            let line_index = match line_starts.binary_search(&offset) {
                Ok(index) => index,
                Err(index) => index - 1,
            };
            let line_start = line_starts[line_index];
            let line_end = source[line_start..]
                .find('\n')
                .map(|relative| line_start + relative)
                .unwrap_or(source.len());
            Diagnostic {
                rule: diagnostic.meta.id,
                severity: diagnostic.meta.severity,
                category: diagnostic.meta.category,
                message: diagnostic.message,
                help: diagnostic.meta.help,
                file: display_path.to_string(),
                file_context,
                line: (line_index + 1) as u32,
                column: (offset - line_start + 1) as u32,
                snippet: source[line_start..line_end].trim_end().to_string(),
            }
        })
        .collect()
}
