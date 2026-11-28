pub mod go;
pub mod js_ts;
pub mod python;
pub mod rust;

mod syntax;

use std::path::Path;

use crate::domain::model::{Language, ParsedFile};
use crate::error::{ArchLensError, Result};

pub use self::syntax::{LanguageParser, count_non_empty_lines, node_text, unquote};

/// Parse a source file on disk according to its detected language.
pub fn parse_file(
    abs_path: &Path,
    rel_path: &str,
    language: Language,
    include_tests: bool,
) -> Result<ParsedFile> {
    parse_file_with(abs_path, rel_path, builtin_parser(language), include_tests)
}

pub(crate) fn parse_file_with(
    abs_path: &Path,
    rel_path: &str,
    parser: &dyn LanguageParser,
    include_tests: bool,
) -> Result<ParsedFile> {
    let source = std::fs::read_to_string(abs_path).map_err(|source| ArchLensError::Io {
        path: abs_path.to_path_buf(),
        source,
    })?;
    parser.parse(rel_path, &source, include_tests)
}

/// Parse an in-memory source string using the appropriate Tree-sitter parser.
pub fn parse_source(
    rel_path: &str,
    source: &str,
    language: Language,
    include_tests: bool,
) -> Result<ParsedFile> {
    builtin_parser(language).parse(rel_path, source, include_tests)
}

pub(crate) fn builtin_parser(language: Language) -> &'static dyn LanguageParser {
    const JAVASCRIPT: js_ts::JsTsParser = js_ts::JsTsParser::javascript();
    match language {
        Language::Rust => &rust::RustParser,
        Language::TypeScript => &js_ts::TypeScriptParser,
        Language::JavaScript => &JAVASCRIPT,
        Language::Vue => &js_ts::VueParser,
        Language::Python => &python::PythonParser,
        Language::Go => &go::GoParser,
    }
}
