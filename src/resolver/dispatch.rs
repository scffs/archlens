use std::collections::HashSet;
use std::path::Path;

use crate::domain::model::{Language, ParsedFile, RawImport};
use crate::resolver::index::{ImportResolver, ProjectIndex, ResolveOutcome};
use crate::resolver::web_defaults::ProjectResolver;
use crate::resolver::{go, python, rust};

type ResolveImport = fn(&ProjectIndex, &str, &RawImport) -> ResolveOutcome;
type TestFiles = fn(&HashSet<String>, &ParsedFile) -> Vec<String>;

struct BuiltinResolver {
    resolve: ResolveImport,
    reexport: Option<ResolveImport>,
    test_files: Option<TestFiles>,
}

impl ImportResolver for BuiltinResolver {
    fn resolve(&self, index: &ProjectIndex, from: &str, import: &RawImport) -> ResolveOutcome {
        (self.resolve)(index, from, import)
    }

    fn resolve_reexport(
        &self,
        index: &ProjectIndex,
        from: &str,
        import: &RawImport,
    ) -> Option<ResolveOutcome> {
        self.reexport.map(|resolve| resolve(index, from, import))
    }

    fn test_only_files(&self, paths: &HashSet<String>, file: &ParsedFile) -> Vec<String> {
        self.test_files
            .map_or_else(Vec::new, |find| find(paths, file))
    }
}

fn rust_test_files(paths: &HashSet<String>, file: &ParsedFile) -> Vec<String> {
    file.cfg_test_mods
        .iter()
        .flat_map(|name| rust::collect_cfg_test_mod_files(paths, &file.path, name))
        .collect()
}

pub(crate) struct BuiltinResolvers<'a> {
    javascript: ProjectResolver<'a>,
}

impl<'a> BuiltinResolvers<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self {
            javascript: ProjectResolver::new(root),
        }
    }

    pub(crate) fn for_file(&self, file: &ParsedFile) -> &dyn ImportResolver {
        const RUST: BuiltinResolver = BuiltinResolver {
            resolve: rust::resolve_rust_import,
            reexport: Some(rust::resolve_rust_import_direct),
            test_files: Some(rust_test_files),
        };
        const PYTHON: BuiltinResolver = BuiltinResolver {
            resolve: |index, from, import| {
                python::resolve_python_import(index, from, &import.specifier)
            },
            reexport: None,
            test_files: None,
        };
        const GO: BuiltinResolver = BuiltinResolver {
            resolve: |index, _, import| go::resolve_go_import(index, &import.specifier),
            reexport: None,
            test_files: None,
        };

        match file.language {
            Language::Rust => &RUST,
            Language::TypeScript | Language::JavaScript | Language::Vue => &self.javascript,
            Language::Python => &PYTHON,
            Language::Go => &GO,
        }
    }
}
