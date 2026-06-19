//! Decompose a TypeScript source into keyed top-level items for merging.

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Declaration, Statement};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};

/// One top-level item: a named declaration or a non-declaration chunk.
pub struct Item {
    /// `decl:<name>` for named declarations; `other:<normalised text>` else.
    pub key: String,
    /// Exact source text (used for output).
    pub raw: String,
    /// Whitespace-normalised text (used for equality, so reformatting and
    /// reindentation don't read as content changes).
    pub norm: String,
}

impl Item {
    /// Index items by key (last write wins on duplicate keys within a file).
    pub fn index(items: &[Item]) -> HashMap<&str, &Item> {
        items.iter().map(|item| (item.key.as_str(), item)).collect()
    }
}

/// Collapse all whitespace runs to single spaces and trim — the canonical form
/// for comparing two pieces of source ignoring layout.
pub fn normalize(source: &str) -> String {
    source.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse `source` and split it into top-level items, or `None` if it does not
/// parse (caller falls back to a coarse merge).
pub fn parse_items(source: &str) -> Option<Vec<Item>> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    if parsed.panicked {
        return None;
    }
    let items = parsed
        .program
        .body
        .iter()
        .map(|statement| item_from_statement(statement, source))
        .collect();
    Some(items)
}

fn item_from_statement(statement: &Statement, source: &str) -> Item {
    let span = statement.span();
    let raw = source[span.start as usize..span.end as usize].to_string();
    let norm = normalize(&raw);
    let key = match statement_name(statement) {
        Some(name) => format!("decl:{name}"),
        None => format!("other:{norm}"),
    };
    Item { key, raw, norm }
}

/// The declaration name introduced by a top-level statement, if any.
fn statement_name(statement: &Statement) -> Option<String> {
    match statement {
        Statement::FunctionDeclaration(func) => func.id.as_ref().map(|id| id.name.to_string()),
        Statement::ClassDeclaration(class) => class.id.as_ref().map(|id| id.name.to_string()),
        Statement::VariableDeclaration(decl) => first_binding_name(decl),
        Statement::ExportNamedDeclaration(export) => {
            export.declaration.as_ref().and_then(declaration_name)
        }
        Statement::ExportDefaultDeclaration(_) => Some("default".to_string()),
        Statement::TSTypeAliasDeclaration(alias) => Some(alias.id.name.to_string()),
        Statement::TSInterfaceDeclaration(interface) => Some(interface.id.name.to_string()),
        Statement::TSEnumDeclaration(enum_decl) => Some(enum_decl.id.name.to_string()),
        _ => None,
    }
}

fn declaration_name(declaration: &Declaration) -> Option<String> {
    match declaration {
        Declaration::FunctionDeclaration(func) => func.id.as_ref().map(|id| id.name.to_string()),
        Declaration::ClassDeclaration(class) => class.id.as_ref().map(|id| id.name.to_string()),
        Declaration::VariableDeclaration(decl) => first_binding_name(decl),
        Declaration::TSTypeAliasDeclaration(alias) => Some(alias.id.name.to_string()),
        Declaration::TSInterfaceDeclaration(interface) => Some(interface.id.name.to_string()),
        Declaration::TSEnumDeclaration(enum_decl) => Some(enum_decl.id.name.to_string()),
        _ => None,
    }
}

fn first_binding_name(decl: &oxc_ast::ast::VariableDeclaration) -> Option<String> {
    use oxc_ast::ast::BindingPattern;
    decl.declarations.first().and_then(|declarator| {
        match &declarator.id {
            BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
        }
    })
}
