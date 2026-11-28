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
