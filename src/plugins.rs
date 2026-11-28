use std::collections::BTreeMap;
use std::path::Path;

use crate::language::Language;
use crate::parser::{LanguageParser, builtin_parser};

/// Extensions supplied by the caller for a single analysis pipeline.
#[derive(Default)]
pub struct AnalysisPlugins<'a> {
    parsers: BTreeMap<Language, &'a dyn LanguageParser>,
    sources: BTreeMap<String, SourceOverride<'a>>,
}

struct SourceOverride<'a> {
    language: Language,
    parser: Option<&'a dyn LanguageParser>,
}

impl<'a> AnalysisPlugins<'a> {
    /// Use `parser` for files detected as `language` in this pipeline.
    pub fn register_parser(&mut self, language: Language, parser: &'a dyn LanguageParser) {
        self.parsers.insert(language, parser);
    }

    /// Detect an additional extension as `language`. A leading dot is optional.
    ///
    /// Registration replaces any previous binding for this extension and takes precedence
    /// over the built-in mapping. The language also controls filtering and default resolution.
    pub fn register_extension(&mut self, extension: &str, language: Language) {
        self.sources.insert(
            extension_key(extension),
            SourceOverride {
                language,
                parser: None,
            },
        );
    }

    /// Scan and parse this extension using a dedicated parser and a stable report language.
    ///
    /// The parser applies only to this extension, before any per-language registration.
    pub fn register_source(
        &mut self,
        extension: &str,
        language: Language,
        parser: &'a dyn LanguageParser,
    ) {
        self.sources.insert(
            extension_key(extension),
            SourceOverride {
                language,
                parser: Some(parser),
            },
        );
    }

    pub(crate) fn language_for_extension(&self, extension: &str) -> Option<Language> {
        let extension = extension.to_ascii_lowercase();
        self.sources
            .get(&extension)
            .map(|source| source.language)
            .or_else(|| Language::from_normalized_extension(&extension))
    }

    pub(crate) fn parser(&self, language: Language, path: &Path) -> &dyn LanguageParser {
        let source_parser = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(|extension| self.sources.get(&extension.to_ascii_lowercase()))
            .and_then(|source| source.parser);
        source_parser
            .or_else(|| self.parsers.get(&language).copied())
            .unwrap_or_else(|| builtin_parser(language))
    }
}

fn extension_key(extension: &str) -> String {
    extension.trim_start_matches('.').to_ascii_lowercase()
}
