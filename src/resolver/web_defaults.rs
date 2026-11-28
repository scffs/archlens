use std::path::Path;

use crate::domain::model::RawImport;
use crate::paths::join_posix;
use crate::resolver::index::{ImportResolver, ProjectIndex, ResolveOutcome};
use crate::resolver::js_ts::imports::{AliasFallback, resolve_js_ts_import_with_aliases};
use crate::resolver::nuxt;

pub(super) struct ProjectResolver<'a> {
    root: &'a Path,
}

impl<'a> ProjectResolver<'a> {
    pub(super) fn new(root: &'a Path) -> Self {
        Self { root }
    }
}

impl ImportResolver for ProjectResolver<'_> {
    fn resolve(&self, index: &ProjectIndex, from: &str, import: &RawImport) -> ResolveOutcome {
        resolve_js_ts_import_with_aliases(
            index,
            from,
            &import.specifier,
            |index, from, specifier| alias_candidates(Some(self.root), index, from, specifier),
        )
    }
}

/// Resolve a JS, TS, JSX, TSX, or Vue import using the analyzer's default alias adapters.
pub fn resolve_js_ts_import(
    index: &ProjectIndex,
    from_file: &str,
    specifier: &str,
) -> ResolveOutcome {
    resolve_js_ts_import_with_aliases(index, from_file, specifier, |index, from, specifier| {
        alias_candidates(None, index, from, specifier)
    })
}

fn alias_candidates(
    root: Option<&Path>,
    index: &ProjectIndex,
    from: &str,
    specifier: &str,
) -> Option<AliasFallback> {
    let package = index
        .enclosing_package(from)
        .map_or("", |package| package.dir.as_str());
    if let Some(rest) = specifier
        .strip_prefix("@@/")
        .or_else(|| specifier.strip_prefix("~~/"))
    {
        return Some(AliasFallback::Candidates(vec![join_posix(package, rest)]));
    }
    if let Some(rest) = specifier
        .strip_prefix("@/")
        .or_else(|| specifier.strip_prefix("~/"))
    {
        return Some(AliasFallback::Candidates(
            ["app", "src", ""]
                .into_iter()
                .map(|dir| join_posix(&join_posix(package, dir), rest))
                .collect(),
        ));
    }
    nuxt::alias_fallback(root, index, package, specifier)
}
