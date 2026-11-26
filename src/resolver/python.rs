use std::path::Path;

use crate::resolver::{PackageManifest, ProjectIndex, ResolveOutcome, join_posix, parent_dir};

/// Load project name from a `pyproject.toml` manifest.
pub fn load_pyproject(abs_path: &Path, dir: &str) -> Option<PackageManifest> {
    let raw = std::fs::read_to_string(abs_path).ok()?;
    let value: toml::Value = toml::from_str(&raw).ok()?;
    let name = value
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            value
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
        })?
        .to_string();

    Some(PackageManifest {
        name,
        dir: dir.to_string(),
    })
}

/// Resolve a Python `import` or `from ... import ...` specifier.
///
/// The parser encodes `from mod import name` as `mod::name` so we can prefer
/// `mod/name.py` when `name` is a submodule and fall back to `mod.py` when it is a symbol.
pub fn resolve_python_import(
    index: &ProjectIndex,
    from_file: &str,
    raw_specifier: &str,
) -> ResolveOutcome {
    let (module_part, symbol_part) = match raw_specifier.split_once("::") {
        Some((m, s)) => (m, Some(s)),
        None => (raw_specifier, None),
    };

    // Relative import (e.g. `.`, `.utils`, `..domain.user`)
    if module_part.starts_with('.') {
        let dots = module_part.bytes().take_while(|&b| b == b'.').count();
        let rest = &module_part[dots..];

        let mut base = parent_dir(from_file);
        for _ in 1..dots {
            base = parent_dir(&base);
        }

        let mut parts: Vec<&str> = rest.split('.').filter(|s| !s.is_empty()).collect();
        if let Some(sym) = symbol_part {
            let mut with_sym = parts.clone();
            with_sym.extend(sym.split('.').filter(|s| !s.is_empty()));
            if let Some(found) = try_python_path(index, &base, &with_sym) {
                return ResolveOutcome::Internal(found);
            }
        }
        if !parts.is_empty()
            && let Some(found) = try_python_path(index, &base, &parts)
        {
            return ResolveOutcome::Internal(found);
        }
        // `from . import foo` where `foo` is in `__init__.py`
        parts.clear();
        let init_file = join_posix(&base, "__init__.py");
        if index.known_files.contains(&init_file) && init_file != from_file {
            return ResolveOutcome::Internal(init_file);
        }

        return ResolveOutcome::Unresolved(raw_specifier.to_string());
    }

    // Absolute import
    let parts: Vec<&str> = module_part.split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return ResolveOutcome::External(String::new());
    }

    let mut roots_to_try: Vec<String> = Vec::new();
    if let Some(pkg) = index.enclosing_package(from_file) {
        roots_to_try.push(join_posix(&pkg.dir, "src"));
        roots_to_try.push(pkg.dir.clone());
    }
    for root in &index.python_roots {
        if !roots_to_try.contains(root) {
            roots_to_try.push(root.clone());
        }
    }

    for root in &roots_to_try {
        if let Some(sym) = symbol_part {
            let mut with_sym = parts.clone();
            with_sym.extend(sym.split('.').filter(|s| !s.is_empty()));
            if let Some(found) = try_python_path(index, root, &with_sym) {
                return ResolveOutcome::Internal(found);
            }
        }
        if let Some(found) = try_python_path(index, root, &parts) {
            return ResolveOutcome::Internal(found);
        }
    }

    ResolveOutcome::External(parts[0].to_string())
}

fn try_python_path(index: &ProjectIndex, root: &str, parts: &[&str]) -> Option<String> {
    for len in (1..=parts.len()).rev() {
        let sub = parts[..len].join("/");
        for candidate in [
            join_posix(root, &format!("{sub}.py")),
            join_posix(root, &format!("{sub}/__init__.py")),
            join_posix(root, &format!("{sub}.pyi")),
        ] {
            if index.known_files.contains(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}
