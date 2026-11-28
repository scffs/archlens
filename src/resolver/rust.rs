use std::collections::HashSet;
use std::path::Path;

use crate::domain::model::{ImportKind, RawImport};
use crate::paths::{join_posix, parent_dir, path_is_under};
use crate::resolver::index::{PackageManifest, ProjectIndex, ResolveOutcome, RustCrateInfo};

/// Parse a `Cargo.toml` manifest if it defines a `[package]`.
pub fn load_cargo_manifest<S: std::hash::BuildHasher>(
    abs_path: &Path,
    dir: &str,
    known_files: &HashSet<String, S>,
) -> Option<(PackageManifest, RustCrateInfo)> {
    let raw = std::fs::read_to_string(abs_path).ok()?;
    let value: toml::Value = toml::from_str(&raw).ok()?;
    let package = value.get("package")?.as_table()?;
    let pkg_name = package.get("name")?.as_str()?.to_string();

    let lib_name = value
        .get("lib")
        .and_then(|v| v.get("name"))
        .and_then(toml::Value::as_str)
        .map_or_else(|| pkg_name.replace('-', "_"), ToString::to_string);

    let lib_path_custom = value
        .get("lib")
        .and_then(|v| v.get("path"))
        .and_then(toml::Value::as_str)
        .map(|p| join_posix(dir, p));

    let default_lib = join_posix(dir, "src/lib.rs");
    let lib_entry = lib_path_custom
        .filter(|p| known_files.contains(p))
        .or_else(|| known_files.contains(&default_lib).then_some(default_lib));

    let default_bin = join_posix(dir, "src/main.rs");
    let bin_entry = known_files.contains(&default_bin).then_some(default_bin);

    let src_dir = lib_entry
        .as_deref()
        .or(bin_entry.as_deref())
        .map_or_else(|| join_posix(dir, "src"), parent_dir);

    let manifest = PackageManifest {
        name: pkg_name,
        dir: dir.to_string(),
    };
    let crate_info = RustCrateInfo {
        crate_ident: lib_name.replace('-', "_"),
        dir: dir.to_string(),
        src_dir,
        lib_entry,
        bin_entry,
    };

    Some((manifest, crate_info))
}

/// Collect all known file paths belonging to a `#[cfg(test)] mod <mod_name>;` subtree.
pub fn collect_cfg_test_mod_files<S: std::hash::BuildHasher>(
    known_files: &HashSet<String, S>,
    from_file: &str,
    mod_name: &str,
) -> Vec<String> {
    let mut matched = Vec::new();
    let bases = [current_module_dir(from_file), parent_dir(from_file)];

    for base in bases {
        let single = join_posix(&base, &format!("{mod_name}.rs"));
        let subdir = join_posix(&base, mod_name);

        if known_files.contains(&single) {
            matched.push(single);
        }
        for file in known_files {
            if path_is_under(file, &subdir) {
                matched.push(file.clone());
            }
        }
        if !matched.is_empty() {
            break;
        }
    }

    matched
}

/// Resolve a Rust `use` or `mod` specifier to an internal file or external crate.
pub fn resolve_rust_import(
    index: &ProjectIndex,
    from_file: &str,
    raw: &RawImport,
) -> ResolveOutcome {
    resolve_rust_import_inner(index, from_file, raw, true)
}

/// Resolve a Rust import without following re-export tables (used when building the re-export table).
pub fn resolve_rust_import_direct(
    index: &ProjectIndex,
    from_file: &str,
    raw: &RawImport,
) -> ResolveOutcome {
    resolve_rust_import_inner(index, from_file, raw, false)
}

fn resolve_rust_import_inner(
    index: &ProjectIndex,
    from_file: &str,
    raw: &RawImport,
    follow_reexports: bool,
) -> ResolveOutcome {
    if raw.kind == ImportKind::ModuleDecl
        && let Some(mod_name) = raw.specifier.strip_prefix("mod::")
    {
        return resolve_mod_decl(index, from_file, mod_name);
    }

    let cleaned = raw.specifier.trim_start_matches("::");
    let segments: Vec<&str> = cleaned
        .split("::")
        .filter(|s| !s.is_empty() && *s != "*")
        .collect();
    if segments.is_empty() {
        return ResolveOutcome::External(String::new());
    }

    match segments[0] {
        "crate" => {
            let src_dir = crate_src_dir_for(index, from_file);
            if let Some((found, tail)) =
                resolve_module_segments_with_tail(index, &src_dir, &segments[1..])
            {
                let target = if follow_reexports {
                    follow_reexport_chain(index, &found, tail)
                } else {
                    found
                };
                return ResolveOutcome::Internal(target);
            }
            // Only resolve to crate entry (`lib.rs`/`main.rs`) if the referenced symbol is actually
            // declared or re-exported there — never as a blind fallback that creates false cycles.
            if let Some(entry) = crate_entry_file_for(index, from_file) {
                if let Some(sym) = segments.get(1) {
                    if follow_reexports
                        && let Some(reexp) = resolve_reexport_symbol(index, &entry, sym)
                    {
                        return ResolveOutcome::Internal(reexp);
                    }
                    if index.file_declares_symbol(&entry, sym) {
                        return ResolveOutcome::Internal(entry);
                    }
                } else {
                    return ResolveOutcome::Internal(entry);
                }
            }
            ResolveOutcome::Unresolved(raw.specifier.clone())
        }
        "self" => {
            let mod_dir = current_module_dir(from_file);
            if let Some((found, tail)) =
                resolve_module_segments_with_tail(index, &mod_dir, &segments[1..])
            {
                let target = if follow_reexports {
                    follow_reexport_chain(index, &found, tail)
                } else {
                    found
                };
                return ResolveOutcome::Internal(target);
            }
            ResolveOutcome::Unresolved(raw.specifier.clone())
        }
        "super" => {
            let mut ups = 0usize;
            while ups < segments.len() && segments[ups] == "super" {
                ups += 1;
            }
            let mut base = current_module_dir(from_file);
            for _ in 0..ups {
                base = parent_dir(&base);
            }
            if let Some((found, tail)) =
                resolve_module_segments_with_tail(index, &base, &segments[ups..])
            {
                let target = if follow_reexports {
                    follow_reexport_chain(index, &found, tail)
                } else {
                    found
                };
                return ResolveOutcome::Internal(target);
            }
            // Parent module file itself (e.g. `use super::SharedType;`).
            for candidate in [
                format!("{base}.rs"),
                join_posix(&base, "mod.rs"),
                join_posix(&base, "lib.rs"),
                join_posix(&base, "main.rs"),
            ] {
                if index.known_files.contains(&candidate) {
                    if let Some(sym) = segments.get(ups)
                        && follow_reexports
                        && let Some(reexp) = resolve_reexport_symbol(index, &candidate, sym)
                    {
                        return ResolveOutcome::Internal(reexp);
                    }
                    return ResolveOutcome::Internal(candidate);
                }
            }
            ResolveOutcome::Unresolved(raw.specifier.clone())
        }
        first => {
            if let Some(krate) = index.rust_crates.get(first) {
                if let Some((found, tail)) =
                    resolve_module_segments_with_tail(index, &krate.src_dir, &segments[1..])
                {
                    let target = if follow_reexports {
                        follow_reexport_chain(index, &found, tail)
                    } else {
                        found
                    };
                    return ResolveOutcome::Internal(target);
                }
                if let Some(entry) = krate.lib_entry.clone().or_else(|| krate.bin_entry.clone()) {
                    if let Some(sym) = segments.get(1)
                        && follow_reexports
                        && let Some(reexp) = resolve_reexport_symbol(index, &entry, sym)
                    {
                        return ResolveOutcome::Internal(reexp);
                    }
                    return ResolveOutcome::Internal(entry);
                }
            }
            // Local child module path without `self::`/`crate::` prefix (common in `main.rs` / `lib.rs`,
            // e.g. `controllers::setup::openapi()` or `telemetry::init(...)`).
            let mod_dir = current_module_dir(from_file);
            if let Some((found, tail)) =
                resolve_module_segments_with_tail(index, &mod_dir, &segments)
            {
                let target = if follow_reexports {
                    follow_reexport_chain(index, &found, tail)
                } else {
                    found
                };
                return ResolveOutcome::Internal(target);
            }
            ResolveOutcome::External(first.to_string())
        }
    }
}

fn follow_reexport_chain(index: &ProjectIndex, start_file: &str, tail: &[&str]) -> String {
    let Some(&sym) = tail.first() else {
        return start_file.to_string();
    };
    let mut current = start_file.to_string();
    for _ in 0..4 {
        if index.file_declares_symbol(&current, sym) {
            return current;
        }
        if let Some(next) = resolve_reexport_symbol(index, &current, sym)
            && next != current
        {
            current = next;
        } else {
            break;
        }
    }
    current
}

fn resolve_reexport_symbol(index: &ProjectIndex, file: &str, symbol: &str) -> Option<String> {
    if let Some(target) = index.reexports.get(&(file.to_string(), symbol.to_string())) {
        return Some(target.clone());
    }
    if let Some(wildcards) = index.wildcard_reexports.get(file) {
        for candidate in wildcards {
            if index.file_declares_symbol(candidate, symbol) {
                return Some(candidate.clone());
            }
        }
    }
    None
}

fn resolve_mod_decl(index: &ProjectIndex, from_file: &str, mod_name: &str) -> ResolveOutcome {
    let mod_dir = current_module_dir(from_file);
    let primary_candidates = [
        join_posix(&mod_dir, &format!("{mod_name}.rs")),
        join_posix(&mod_dir, &format!("{mod_name}/mod.rs")),
    ];
    for candidate in primary_candidates {
        if index.known_files.contains(&candidate) {
            return ResolveOutcome::Internal(candidate);
        }
    }

    let dir = parent_dir(from_file);
    for candidate in [
        join_posix(&dir, &format!("{mod_name}.rs")),
        join_posix(&dir, &format!("{mod_name}/mod.rs")),
    ] {
        if index.known_files.contains(&candidate) {
            return ResolveOutcome::Internal(candidate);
        }
    }

    ResolveOutcome::Unresolved(format!("mod {mod_name}"))
}

/// Directory containing child modules of `file_path`:
/// - `a/b/lib.rs`, `a/b/main.rs`, `a/b/mod.rs` -> `a/b`
/// - `a/b/foo.rs` -> `a/b/foo` (Rust 2018 submodule directory convention)
pub fn current_module_dir(file_path: &str) -> String {
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    if matches!(file_name, "lib.rs" | "main.rs" | "mod.rs") {
        parent_dir(file_path)
    } else if let Some(stem) = file_path.strip_suffix(".rs") {
        stem.to_string()
    } else {
        parent_dir(file_path)
    }
}

fn crate_src_dir_for(index: &ProjectIndex, from_file: &str) -> String {
    if let Some(krate) = index.enclosing_rust_crate(from_file) {
        return krate.src_dir.clone();
    }
    if let Some(idx) = from_file.find("/src/") {
        return from_file[..idx + 4].to_string();
    }
    if from_file.starts_with("src/") {
        return "src".to_string();
    }
    parent_dir(from_file)
}

fn crate_entry_file_for(index: &ProjectIndex, from_file: &str) -> Option<String> {
    if let Some(krate) = index.enclosing_rust_crate(from_file)
        && let Some(entry) = krate.lib_entry.clone().or_else(|| krate.bin_entry.clone())
    {
        return Some(entry);
    }
    let src = crate_src_dir_for(index, from_file);
    [join_posix(&src, "lib.rs"), join_posix(&src, "main.rs")]
        .into_iter()
        .find(|candidate| index.known_files.contains(candidate))
}

/// Longest-prefix matching of Rust path segments against `.rs` and `/mod.rs` files,
/// returning both the matched file and the trailing item segments.
fn resolve_module_segments_with_tail<'a>(
    index: &ProjectIndex,
    base_dir: &str,
    segments: &'a [&'a str],
) -> Option<(String, &'a [&'a str])> {
    for len in (1..=segments.len()).rev() {
        let sub = segments[..len].join("/");
        let file_candidate = join_posix(base_dir, &format!("{sub}.rs"));
        if index.known_files.contains(&file_candidate) {
            return Some((file_candidate, &segments[len..]));
        }
        let mod_candidate = join_posix(base_dir, &format!("{sub}/mod.rs"));
        if index.known_files.contains(&mod_candidate) {
            return Some((mod_candidate, &segments[len..]));
        }
    }
    None
}
