use oxc_span::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileContext {
    Production,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Correctness,
    Idiomatic,
    Architecture,
    Performance,
    V4Migration,
    /// `--deep` findings merged from @effect/language-service.
    TypeAware,
    /// `--adopt` (experimental): vanilla TS that should migrate to Effect.
    Adoption,
    /// `--agent` (experimental): non-Effect "slop" patterns LLM agents emit
    /// (if/else chains, ternaries, string-equality guards, raw loops, `let`,
    /// duplicated function bodies) that have a cleaner Effect/functional form.
    AgentHygiene,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Correctness => "Correctness",
            Category::Idiomatic => "Idiomatic",
            Category::Architecture => "Architecture",
            Category::Performance => "Performance",
            Category::V4Migration => "v4 Migration",
            Category::TypeAware => "Type-aware",
            Category::Adoption => "Effect Adoption",
            Category::AgentHygiene => "Agent hygiene",
        }
    }
}

/// Static metadata for a rule, shared by every diagnostic it emits.
#[derive(Debug)]
pub struct RuleMeta {
    pub id: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub help: &'static str,
}

/// Span-based diagnostic emitted while a file's AST is in memory.
/// Converted to a [`Diagnostic`] (line/col + snippet) by the engine.
pub struct RawDiagnostic {
    pub meta: &'static RuleMeta,
    pub span: Span,
    pub message: String,
    /// When set, overrides `meta.severity` for this single finding — used by
    /// the `--agent` family to escalate to `error` under `--agent-strict`.
    pub severity: Option<Severity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub help: &'static str,
    pub file: String,
    pub file_context: FileContext,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
}

impl Diagnostic {
    /// Build a diagnostic from its rule metadata plus the located fields.
    /// Shared by the cross-file passes so they don't each repeat the
    /// `rule`/`severity`/`category`/`help`-from-meta wiring.
    pub(crate) fn from_meta(
        meta: &'static RuleMeta,
        message: String,
        file: String,
        file_context: FileContext,
        line: u32,
        column: u32,
        snippet: String,
    ) -> Diagnostic {
        Diagnostic {
            rule: meta.id,
            severity: meta.severity,
            category: meta.category,
            message,
            help: meta.help,
            file,
            file_context,
            line,
            column,
            snippet,
        }
    }
}
