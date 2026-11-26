use std::path::Path;

use crate::resolver::{GoModuleInfo, PackageManifest, ProjectIndex, ResolveOutcome, join_posix};

/// Load module path from `go.mod`.
pub fn load_go_mod(abs_path: &Path, dir: &str) -> Option<(PackageManifest, GoModuleInfo)> {
    let raw = std::fs::read_to_string(abs_path).ok()?;
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            let module_path = rest
                .split_whitespace()
                .next()?
                .trim_matches('"')
                .to_string();
            let short_name = module_path
                .rsplit('/')
                .next()
                .unwrap_or(&module_path)
                .to_string();
            let manifest = PackageManifest {
                name: short_name,
                dir: dir.to_string(),
            };
            let info = GoModuleInfo {
                module_path,
                dir: dir.to_string(),
            };
            return Some((manifest, info));
        }
    }
    None
}

/// Resolve a Go import path against discovered `go.mod` modules.
pub fn resolve_go_import(index: &ProjectIndex, specifier: &str) -> ResolveOutcome {
    for go_mod in &index.go_modules {
        let sub_dir = if specifier == go_mod.module_path {
            Some(go_mod.dir.clone())
        } else {
            specifier
                .strip_prefix(&format!("{}/", go_mod.module_path))
                .map(|rest| join_posix(&go_mod.dir, rest))
        };

        if let Some(target_dir) = sub_dir {
            if let Some(files) = index.dir_files.get(&target_dir)
                && let Some(go_file) = files.iter().find(|f| {
                    Path::new(f)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("go"))
                })
            {
                return ResolveOutcome::Internal(go_file.clone());
            }
            return ResolveOutcome::Unresolved(specifier.to_string());
        }
    }

    ResolveOutcome::External(specifier.to_string())
}
