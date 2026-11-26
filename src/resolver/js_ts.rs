use std::path::Path;

use serde_json::Value;

use crate::resolver::{
    JsPackageInfo, PackageManifest, ProjectIndex, ResolveOutcome, TsConfigInfo, join_posix,
    parent_dir, path_is_under,
};
use crate::scanner::normalize_rel_path;

const CODE_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "vue"];

const NON_CODE_ASSET_EXTENSIONS: &[&str] = &[
    "css", "scss", "sass", "less", "styl", "pcss", "svg", "png", "jpg", "jpeg", "gif", "webp",
    "avif", "ico", "woff", "woff2", "ttf", "eot", "otf", "json", "yaml", "yml", "toml", "md",
    "mdx", "txt", "html", "wasm", "mp4", "webm", "mp3", "wav",
];

/// Load a `package.json` manifest for monorepo package detection and cross-package resolution.
pub fn load_package_json(abs_path: &Path, dir: &str) -> Option<(PackageManifest, JsPackageInfo)> {
    let raw = std::fs::read_to_string(abs_path).ok()?;
    let json: Value = serde_json::from_str(&raw).ok()?;
    let name = json.get("name")?.as_str()?.to_string();

    let mut entry_candidates = Vec::new();

    if let Some(exports) = json.get("exports") {
        collect_export_entries(exports, &mut entry_candidates);
    }
    for key in ["source", "module", "main", "types"] {
        if let Some(val) = json.get(key).and_then(Value::as_str) {
            entry_candidates.push(val.to_string());
        }
    }
    entry_candidates.extend([
        "src/index.ts".to_string(),
        "src/index.tsx".to_string(),
        "src/index.js".to_string(),
        "src/main.ts".to_string(),
        "index.ts".to_string(),
        "index.js".to_string(),
    ]);

    let manifest = PackageManifest {
        name: name.clone(),
        dir: dir.to_string(),
    };
    let info = JsPackageInfo {
        name,
        dir: dir.to_string(),
        entry_candidates,
    };

    Some((manifest, info))
}

fn collect_export_entries(exports: &Value, out: &mut Vec<String>) {
    match exports {
        Value::String(s) => out.push(s.clone()),
        Value::Object(map) => {
            if let Some(dot) = map.get(".") {
                collect_export_entries(dot, out);
            } else {
                for key in ["import", "default", "types", "require"] {
                    if let Some(v) = map.get(key) {
                        collect_export_entries(v, out);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Load a `tsconfig.json` / `jsconfig.json`, supporting JSONC, `extends`, and project `references`
/// (such as Nuxt 4 `.nuxt/tsconfig.app.json`).
pub fn load_tsconfig(root: &Path, abs_path: &Path, dir: &str) -> Option<TsConfigInfo> {
    let mut base_dir: Option<String> = None;
    let mut paths: Vec<(String, Vec<String>)> = Vec::new();
    load_tsconfig_chain(root, abs_path, 0, &mut base_dir, &mut paths);

    let has_base_url = base_dir.is_some();
    let resolved_base = base_dir.unwrap_or_else(|| dir.to_string());

    if !has_base_url && paths.is_empty() {
        return None;
    }

    Some(TsConfigInfo {
        dir: dir.to_string(),
        base_dir: resolved_base,
        paths,
        has_base_url,
    })
}

fn load_tsconfig_chain(
    root: &Path,
    abs_path: &Path,
    depth: usize,
    base_dir: &mut Option<String>,
    paths: &mut Vec<(String, Vec<String>)>,
) {
    if depth > 6 {
        return;
    }
    let Ok(raw) = std::fs::read_to_string(abs_path) else {
        return;
    };
    let cleaned = strip_jsonc(&raw);
    let Ok(json) = serde_json::from_str::<Value>(&cleaned) else {
        return;
    };

    let cfg_dir = abs_path
        .parent()
        .map_or_else(String::new, |p| normalize_rel_path(root, p));

    // Follow `extends` chain.
    if let Some(extends_val) = json.get("extends").and_then(Value::as_str)
        && extends_val.starts_with('.')
        && let Some(parent) = abs_path.parent()
    {
        let mut target = parent.join(extends_val);
        if !target.is_file() && target.extension().is_none() {
            target.set_extension("json");
        }
        if target.starts_with(root) && target.is_file() {
            load_tsconfig_chain(root, &target, depth + 1, base_dir, paths);
        }
    }

    // Follow project `references` (e.g. Nuxt 4 `./.nuxt/tsconfig.app.json`, Vite `tsconfig.app.json`).
    if let Some(refs) = json.get("references").and_then(Value::as_array)
        && let Some(parent) = abs_path.parent()
    {
        for r in refs {
            if let Some(ref_path) = r.get("path").and_then(Value::as_str) {
                let mut target = parent.join(ref_path);
                if target.is_dir() {
                    target = target.join("tsconfig.json");
                } else if !target.is_file() && target.extension().is_none() {
                    target.set_extension("json");
                }
                if target.starts_with(root) && target.is_file() {
                    let mut ref_base: Option<String> = None;
                    let mut ref_paths: Vec<(String, Vec<String>)> = Vec::new();
                    load_tsconfig_chain(root, &target, depth + 1, &mut ref_base, &mut ref_paths);
                    let ref_cfg_dir = target
                        .parent()
                        .map_or_else(String::new, |p| normalize_rel_path(root, p));
                    let effective_ref_base = ref_base.unwrap_or(ref_cfg_dir);
                    for (pat, repls) in ref_paths {
                        let normalized_repls: Vec<String> = repls
                            .into_iter()
                            .map(|rep| join_posix(&effective_ref_base, &rep))
                            .collect();
                        if !paths.iter().any(|(existing, _)| existing == &pat) {
                            paths.push((pat, normalized_repls));
                        }
                    }
                }
            }
        }
    }

    if let Some(opts) = json.get("compilerOptions").and_then(Value::as_object) {
        let local_base = if let Some(bu) = opts.get("baseUrl").and_then(Value::as_str) {
            let joined = join_posix(&cfg_dir, bu);
            *base_dir = Some(joined.clone());
            joined
        } else {
            base_dir.clone().unwrap_or_else(|| cfg_dir.clone())
        };

        if let Some(paths_obj) = opts.get("paths").and_then(Value::as_object) {
            for (pattern, targets_val) in paths_obj {
                if let Some(arr) = targets_val.as_array() {
                    let targets: Vec<String> = arr
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|rep| join_posix(&local_base, rep))
                        .collect();
                    if !targets.is_empty() {
                        paths.retain(|(existing, _)| existing != pattern);
                        paths.push((pattern.clone(), targets));
                    }
                }
            }
        }
    }
}

/// Resolve a JS, TS, JSX, TSX, or Vue import specifier.
pub fn resolve_js_ts_import(
    index: &ProjectIndex,
    from_file: &str,
    raw_specifier: &str,
) -> ResolveOutcome {
    let clean = raw_specifier
        .split(['?', '#'])
        .next()
        .unwrap_or(raw_specifier)
        .trim();

    if clean.is_empty() || clean.starts_with("http://") || clean.starts_with("https://") {
        return ResolveOutcome::External(clean.to_string());
    }

    if is_non_code_asset(clean) {
        return ResolveOutcome::External(format!("asset:{clean}"));
    }

    // Nuxt / framework virtual modules (`#imports`, `#app`, `#build/...`, `#components`, `#i18n`).
    if clean.starts_with('#') && !clean.starts_with("#shared/") {
        return ResolveOutcome::External(clean.to_string());
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
    for tsconfig in &index.tsconfigs {
        if !path_is_under(from_file, &tsconfig.dir) {
            continue;
        }
        for (pattern, replacements) in &tsconfig.paths {
            if let Some(matched_wildcard) = match_tsconfig_pattern(pattern, clean) {
                for repl in replacements {
                    let candidate = repl.replace('*', matched_wildcard);
                    if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                        return ResolveOutcome::Internal(found);
                    }
                }
            }
        }
        if tsconfig.has_base_url {
            let candidate = join_posix(&tsconfig.base_dir, clean);
            if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                return ResolveOutcome::Internal(found);
            }
        }
        break;
    }

    // 3. Common Vite / Nuxt 4 / Vue aliases (`@/`, `~/`, `@@/`, `~~/`, `#shared/`)
    let pkg_dir = index
        .enclosing_package(from_file)
        .map_or("", |p| p.dir.as_str());

    if let Some(rest) = clean.strip_prefix("#shared/") {
        let candidate = join_posix(&join_posix(pkg_dir, "shared"), rest);
        if let Some(found) = try_resolve_file_candidate(index, &candidate) {
            return ResolveOutcome::Internal(found);
        }
        return ResolveOutcome::Unresolved(clean.to_string());
    }

    if let Some(rest) = clean
        .strip_prefix("@@/")
        .or_else(|| clean.strip_prefix("~~/"))
    {
        let candidate = join_posix(pkg_dir, rest);
        if let Some(found) = try_resolve_file_candidate(index, &candidate) {
            return ResolveOutcome::Internal(found);
        }
        return ResolveOutcome::Unresolved(clean.to_string());
    }

    if let Some(rest) = clean
        .strip_prefix("@/")
        .or_else(|| clean.strip_prefix("~/"))
    {
        for base in [
            join_posix(pkg_dir, "app"),
            join_posix(pkg_dir, "src"),
            pkg_dir.to_string(),
        ] {
            let candidate = join_posix(&base, rest);
            if let Some(found) = try_resolve_file_candidate(index, &candidate) {
                return ResolveOutcome::Internal(found);
            }
        }
        return ResolveOutcome::Unresolved(clean.to_string());
    }

    // 4. Monorepo workspace package imports (`@scope/pkg` or `@scope/pkg/subpath`)
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

    for ext in CODE_EXTENSIONS {
        let with_ext = format!("{candidate}.{ext}");
        if index.known_files.contains(&with_ext) {
            return Some(with_ext);
        }
    }

    for ext in CODE_EXTENSIONS {
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

/// Strip `//` line comments, `/* ... */` block comments, and trailing commas from JSONC text.
fn strip_jsonc(input: &str) -> String {
    let mut no_comments = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            no_comments.push(b as char);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if b == b'"' {
            in_string = true;
            no_comments.push('"');
            i += 1;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else {
            no_comments.push(b as char);
            i += 1;
        }
    }

    let chars: Vec<char> = no_comments.chars().collect();
    let mut out = String::with_capacity(no_comments.len());
    let mut idx = 0;
    let mut inside_str = false;
    let mut esc = false;

    while idx < chars.len() {
        let ch = chars[idx];
        if inside_str {
            out.push(ch);
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                inside_str = false;
            }
            idx += 1;
            continue;
        }
        if ch == '"' {
            inside_str = true;
            out.push(ch);
            idx += 1;
            continue;
        }
        if ch == ',' {
            let mut look = idx + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && matches!(chars[look], '}' | ']') {
                idx += 1;
                continue;
            }
        }
        out.push(ch);
        idx += 1;
    }

    out
}
