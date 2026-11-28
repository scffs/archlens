use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::model::ParsedFile;
use crate::language::Language;
use crate::parser::{LanguageParser, builtin_parser};
use crate::resolver::{ImportResolver, builtin_resolver};

/// Extensions supplied by the caller for a single analysis pipeline.
#[derive(Default)]
pub struct AnalysisPlugins<'a> {
    parsers: BTreeMap<Language, &'a dyn LanguageParser>,
    sources: BTreeMap<String, SourceOverride<'a>>,
    resolvers: BTreeMap<Language, &'a dyn ImportResolver>,
    source_resolvers: BTreeMap<String, &'a dyn ImportResolver>,
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

    /// Use this resolver for a report language's imports and re-exports.
    pub fn register_resolver(&mut self, language: Language, resolver: &'a dyn ImportResolver) {
        self.resolvers.insert(language, resolver);
    }

    /// Use a dedicated resolver for files with this extension, before any language override.
    pub fn register_source_resolver(&mut self, extension: &str, resolver: &'a dyn ImportResolver) {
        self.source_resolvers
            .insert(extension_key(extension), resolver);
    }

    /// Register a complete parser and resolver for a supported language and its extensions.
    ///
    /// Named extensions replace their previous individual parser and resolver overrides.
    pub fn register_language(
        &mut self,
        language: Language,
        extensions: &[&str],
        parser: &'a dyn LanguageParser,
        resolver: &'a dyn ImportResolver,
    ) {
        self.register_parser(language, parser);
        self.register_resolver(language, resolver);
        for extension in extensions {
            self.register_extension(extension, language);
            self.source_resolvers.remove(&extension_key(extension));
        }
    }

    pub(crate) fn resolver(&self, file: &ParsedFile) -> &dyn ImportResolver {
        source_extension(Path::new(&file.path))
            .and_then(|extension| self.source_resolvers.get(&extension).copied())
            .or_else(|| self.resolvers.get(&file.language).copied())
            .unwrap_or_else(|| builtin_resolver(file))
    }

    pub(crate) fn language_for_extension(&self, extension: &str) -> Option<Language> {
        let extension = extension.to_ascii_lowercase();
        self.sources
            .get(&extension)
            .map(|source| source.language)
            .or_else(|| Language::from_normalized_extension(&extension))
    }

    pub(crate) fn parser(&self, language: Language, path: &Path) -> &dyn LanguageParser {
        let source_parser = source_extension(path)
            .and_then(|extension| self.sources.get(&extension))
            .and_then(|source| source.parser);
        source_parser
            .or_else(|| self.parsers.get(&language).copied())
            .unwrap_or_else(|| builtin_parser(language))
    }
}

fn extension_key(extension: &str) -> String {
    extension.trim_start_matches('.').to_ascii_lowercase()
}

fn source_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}
