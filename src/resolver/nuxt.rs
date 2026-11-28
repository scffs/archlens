use std::path::Path;

use crate::language::js_ts_extensions;
use crate::paths::join_posix;
use crate::resolver::index::ProjectIndex;
use crate::resolver::js_ts::imports::AliasFallback;

/// Built-in source and generated aliases for a package with a Nuxt configuration.
pub(super) fn alias_fallback(
    root: Option<&Path>,
    index: &ProjectIndex,
    package: &str,
    specifier: &str,
) -> Option<AliasFallback> {
    let fallback = if let Some(rest) = specifier.strip_prefix("#shared/") {
        AliasFallback::Candidates(vec![join_posix(&join_posix(package, "shared"), rest)])
    } else if let Some(rest) = specifier.strip_prefix("#server/") {
        AliasFallback::Candidates(vec![join_posix(&join_posix(package, "server"), rest)])
    } else if matches!(
        specifier,
        "#imports" | "#components" | "#build" | "#app" | "#internal/nuxt/paths"
    ) || specifier.starts_with("#build/")
        || specifier.starts_with("#app/")
    {
        AliasFallback::Virtual
    } else {
        return None;
    };

    js_ts_extensions()
        .filter(|extension| *extension != "vue")
        .any(|extension| {
            let path = join_posix(package, &format!("nuxt.config.{extension}"));
            index.known_files.contains(&path) || root.is_some_and(|root| root.join(&path).is_file())
        })
        .then_some(fallback)
}
