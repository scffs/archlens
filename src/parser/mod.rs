pub mod go;
pub mod js_ts;
pub mod python;
pub mod rust;

use std::path::Path;

use crate::domain::model::{Language, ParsedFile};
use crate::error::{ArchLensError, Result};

/// Contract for extracting imports and abstract/concrete declarations from source text.
pub trait LanguageParser {
    fn parse(&self, rel_path: &str, source: &str, include_tests: bool) -> Result<ParsedFile>;
}

/// Parse a source file on disk according to its detected language.
pub fn parse_file(
    abs_path: &Path,
    rel_path: &str,
    language: Language,
    include_tests: bool,
) -> Result<ParsedFile> {
    let source = std::fs::read_to_string(abs_path).map_err(|source| ArchLensError::Io {
        path: abs_path.to_path_buf(),
        source,
    })?;
    parse_source(rel_path, &source, language, include_tests)
}

/// Parse an in-memory source string using the appropriate Tree-sitter parser.
pub fn parse_source(
    rel_path: &str,
    source: &str,
    language: Language,
    include_tests: bool,
) -> Result<ParsedFile> {
    match language {
        Language::Rust => rust::RustParser.parse(rel_path, source, include_tests),
        Language::TypeScript => {
            let is_tsx = Path::new(rel_path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tsx"));
            js_ts::JsTsParser::typescript(is_tsx).parse(rel_path, source, include_tests)
        }
        Language::JavaScript => {
            js_ts::JsTsParser::javascript().parse(rel_path, source, include_tests)
        }
        Language::Vue => js_ts::VueParser.parse(rel_path, source, include_tests),
        Language::Python => python::PythonParser.parse(rel_path, source, include_tests),
        Language::Go => go::GoParser.parse(rel_path, source, include_tests),
    }
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
