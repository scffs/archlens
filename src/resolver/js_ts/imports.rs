use crate::language::js_ts_extensions;
use crate::paths::{join_posix, parent_dir, path_is_under};
use crate::resolver::index::{ProjectIndex, ResolveOutcome};

const NON_CODE_ASSET_EXTENSIONS: &[&str] = &[
    "css", "scss", "sass", "less", "styl", "pcss", "svg", "png", "jpg", "jpeg", "gif", "webp",
    "avif", "ico", "woff", "woff2", "ttf", "eot", "otf", "json", "yaml", "yml", "toml", "md",
    "mdx", "txt", "html", "wasm", "mp4", "webm", "mp3", "wav",
];

/// Fallback policy for an alias after project-configured paths have been tried.
pub enum AliasFallback {
    /// Repository-relative candidates. An alias without a matching file remains unresolved.
    Candidates(Vec<String>),
    /// An alias supplied outside the scanned source graph.
    Virtual,
}

/// Resolve an import with caller-supplied fallback alias candidates.
///
/// Matching TypeScript paths take precedence. Return `None` when the fallback
/// does not recognize an alias.
pub fn resolve_js_ts_import_with_aliases(
    index: &ProjectIndex,
    from_file: &str,
    raw_specifier: &str,
    alias_candidates: impl FnOnce(&ProjectIndex, &str, &str) -> Option<AliasFallback>,
) -> ResolveOutcome {
    let raw_specifier = raw_specifier.trim_start();
    let private_alias = raw_specifier.starts_with('#');
    let suffix_search_start = usize::from(private_alias);
    let clean = raw_specifier[suffix_search_start..]
        .find(['?', '#'])
        .map_or(raw_specifier, |index| {
            &raw_specifier[..suffix_search_start + index]
        })
        .trim_end();

    if clean.is_empty() || clean.starts_with("http://") || clean.starts_with("https://") {
        return ResolveOutcome::External(clean.to_string());
    }

    if !private_alias && is_non_code_asset(clean) {
        return ResolveOutcome::External(format!("asset:{clean}"));
    }

    // 1. Relative path (`./...` or `../...`)
    if clean.starts_with("./") || clean.starts_with("../") || clean == "." || clean == ".." {
        let dir = parent_dir(from_file);
        let candidate = join_posix(&dir, clean);
        if let Some(found) = try_resolve_file_candidate(index, &candidate) {
            return ResolveOutcome::Internal(found);
        }
        return ResolveOutcome::Unresolved(clean.to_string());
    }

    // 2. Nearest enclosing tsconfig `paths` and `baseUrl`
    let mut configured_alias = false;
    for tsconfig in &index.tsconfigs {
        if !path_is_under(from_file, &tsconfig.dir) {
            continue;
        }
        for (pattern, replacements) in &tsconfig.paths {
            if let Some(matched_wildcard) = match_tsconfig_pattern(pattern, clean) {
                configured_alias = true;
                for repl in replacements {
                    let candidate = repl.replace('*', matched_wildcard);
                    if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                        return ResolveOutcome::Internal(found);
                    }
                }
            }
        }
        if !private_alias && tsconfig.has_base_url {
            let candidate = join_posix(&tsconfig.base_dir, clean);
            if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                return ResolveOutcome::Internal(found);
            }
        }
        break;
    }

    match alias_candidates(index, from_file, clean) {
        Some(AliasFallback::Virtual) => return ResolveOutcome::External(String::new()),
        Some(AliasFallback::Candidates(candidates)) => {
            if is_non_code_asset(clean) {
                return ResolveOutcome::External(format!("asset:{clean}"));
            }
            for candidate in candidates {
                if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                    return ResolveOutcome::Internal(found);
                }
            }
            return ResolveOutcome::Unresolved(clean.to_string());
        }
        None => {}
    }
    if private_alias {
        return if configured_alias {
            ResolveOutcome::Unresolved(clean.to_string())
        } else {
            ResolveOutcome::External(String::new())
        };
    }

    // Resolve monorepo workspace package imports (`@scope/pkg` or `@scope/pkg/subpath`)
    let pkg_name = extract_npm_package_name(clean);
    if let Some(pkg) = index.js_packages.get(pkg_name) {
        if clean == pkg_name {
            for entry in &pkg.entry_candidates {
                let candidate = join_posix(&pkg.dir, entry);
                if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                    return ResolveOutcome::Internal(found);
                }
            }
        } else if let Some(subpath) = clean.strip_prefix(&format!("{pkg_name}/")) {
            for base in [
                join_posix(&pkg.dir, subpath),
                join_posix(&join_posix(&pkg.dir, "src"), subpath),
            ] {
                if let Some(found) = try_resolve_file_candidate(index, &base) {
                    return ResolveOutcome::Internal(found);
                }
            }
        }
        for entry in &pkg.entry_candidates {
            let candidate = join_posix(&pkg.dir, entry);
            if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                return ResolveOutcome::Internal(found);
            }
        }
    }

    ResolveOutcome::External(pkg_name.to_string())
}

fn try_resolve_file_candidate(index: &ProjectIndex, candidate: &str) -> Option<String> {
    if index.known_files.contains(candidate) {
        return Some(candidate.to_string());
    }

    for (from_ext, replacements) in [
        (".js", &[".ts", ".tsx", ".mts", ".vue"][..]),
        (".jsx", &[".tsx", ".ts"][..]),
        (".mjs", &[".mts", ".ts"][..]),
        (".cjs", &[".cts", ".ts"][..]),
    ] {
        if let Some(stem) = candidate.strip_suffix(from_ext) {
            for repl in replacements {
                let alt = format!("{stem}{repl}");
                if index.known_files.contains(&alt) {
                    return Some(alt);
                }
            }
        }
    }

    for ext in js_ts_extensions() {
        let with_ext = format!("{candidate}.{ext}");
        if index.known_files.contains(&with_ext) {
            return Some(with_ext);
        }
    }

    for ext in js_ts_extensions() {
        let index_file = join_posix(candidate, &format!("index.{ext}"));
        if index.known_files.contains(&index_file) {
            return Some(index_file);
        }
    }

    None
}

fn match_tsconfig_pattern<'a>(pattern: &str, specifier: &'a str) -> Option<&'a str> {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        if specifier.starts_with(prefix)
            && specifier.ends_with(suffix)
            && specifier.len() >= prefix.len() + suffix.len()
        {
            return Some(&specifier[prefix.len()..specifier.len() - suffix.len()]);
        }
        None
    } else if pattern == specifier {
        Some("")
    } else {
        None
    }
}

fn extract_npm_package_name(specifier: &str) -> &str {
    if specifier.starts_with('@') {
        let mut parts = specifier.splitn(3, '/');
        if let (Some(scope), Some(name)) = (parts.next(), parts.next()) {
            let len = scope.len() + 1 + name.len();
            return &specifier[..len];
        }
    }
    specifier.split('/').next().unwrap_or(specifier)
}

fn is_non_code_asset(specifier: &str) -> bool {
    let Some((_, ext)) = specifier.rsplit_once('.') else {
        return false;
    };
    if ext.contains('/') {
        return false;
    }
    NON_CODE_ASSET_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}
