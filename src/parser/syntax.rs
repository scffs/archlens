use tree_sitter::{Language, Parser, Tree};

use crate::domain::model::ParsedFile;
use crate::error::{ArchLensError, Result};

/// Contract for extracting imports and abstract/concrete declarations from source text.
pub trait LanguageParser {
    fn parse(&self, rel_path: &str, source: &str, include_tests: bool) -> Result<ParsedFile>;
}

pub(super) fn parse_tree(
    language: Language,
    label: &'static str,
    source: &str,
) -> Result<Option<Tree>> {
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|err| ArchLensError::ParserInit {
            language: label,
            message: err.to_string(),
        })?;
    Ok(parser.parse(source, None))
}

/// Count non-empty lines of code.
pub fn count_non_empty_lines(source: &str) -> usize {
    source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Helper to safely slice UTF-8 text for a Tree-sitter node.
pub fn node_text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Strip matching surrounding quotes (`'`, `"`, or `` ` ``) from a string literal.
pub fn unquote(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"')
            || (first == b'\'' && last == b'\'')
            || (first == b'`' && last == b'`')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}
