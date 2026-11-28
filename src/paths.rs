use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::error::{ArchLensError, Result};

/// Build a compiled `GlobSet` from pattern strings.
pub fn build_glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|source| ArchLensError::InvalidGlob {
            pattern: pattern.clone(),
            source,
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|source| ArchLensError::InvalidGlob {
            pattern: patterns.join(", "),
            source,
        })
}

/// Convert an absolute path under `root` to a normalized POSIX relative path.
pub fn normalize_rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Parent directory of a POSIX relative path (empty string for top-level files).
pub fn parent_dir(rel_path: &str) -> String {
    match rel_path.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    }
}

/// True if `rel_path` lives inside `dir` (empty `dir` matches everything).
pub fn path_is_under(rel_path: &str, dir: &str) -> bool {
    if dir.is_empty() {
        return true;
    }
    rel_path == dir
        || (rel_path.starts_with(dir) && rel_path.as_bytes().get(dir.len()) == Some(&b'/'))
}

/// Join and normalize two POSIX relative paths (resolving `.` and `..`).
pub fn join_posix(base: &str, rel: &str) -> String {
    let combined = if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rel}")
    };
    normalize_posix(&combined)
}

/// Collapse `.` and `..` segments in a POSIX relative path.
pub fn normalize_posix(raw: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in raw.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    stack.join("/")
}
