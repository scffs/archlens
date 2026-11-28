use std::path::PathBuf;

use ignore::WalkBuilder;

use crate::config::AnalysisConfig;
use crate::domain::model::Language;
use crate::error::{ArchLensError, Result};
pub use crate::paths::{build_glob_set, normalize_rel_path};

/// A discovered source file ready for parsing.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub abs_path: PathBuf,
    /// Normalized forward-slash path relative to analysis root.
    pub rel_path: String,
    pub language: Language,
}

/// Walk `config.root` respecting `.gitignore`, exclude patterns, and language filters.
pub fn scan_repository(config: &AnalysisConfig) -> Result<Vec<ScannedFile>> {
    if !config.root.exists() {
        return Err(ArchLensError::InvalidRoot(config.root.clone()));
    }

    let exclude_set = build_glob_set(&config.exclude)?;
    let mut files = Vec::new();

    let walker = WalkBuilder::new(&config.root)
        .standard_filters(true)
        .hidden(true)
        .build();

    for entry_result in walker {
        let Ok(entry) = entry_result else {
            continue;
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(language) = Language::from_extension(ext) else {
            continue;
        };

        if !config.languages.is_empty() && !config.languages.contains(&language) {
            continue;
        }

        let rel_path = normalize_rel_path(&config.root, path);
        if exclude_set.is_match(&rel_path) {
            continue;
        }

        if !config.include_tests && is_test_path(&rel_path) {
            continue;
        }

        files.push(ScannedFile {
            abs_path: path.to_path_buf(),
            rel_path,
            language,
        });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(files)
}

/// Heuristic check for dedicated test files and test directories across supported languages.
fn is_test_path(rel_path: &str) -> bool {
    let lower = rel_path.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').collect();

    for &seg in &segments[..segments.len().saturating_sub(1)] {
        if matches!(
            seg,
            "tests"
                | "test"
                | "__tests__"
                | "__mocks__"
                | "e2e"
                | "fixtures"
                | "benches"
                | "loadtests"
                | "storybook"
        ) {
            return true;
        }
    }

    let file_name = segments.last().copied().unwrap_or("");
    file_name.ends_with("_tests.rs")
        || file_name.ends_with("_test.rs")
        || file_name == "tests.rs"
        || file_name == "test_support.rs"
        || file_name == "test_utils.rs"
        || file_name.ends_with(".test.ts")
        || file_name.ends_with(".test.tsx")
        || file_name.ends_with(".test.js")
        || file_name.ends_with(".test.jsx")
        || file_name.ends_with(".spec.ts")
        || file_name.ends_with(".spec.tsx")
        || file_name.ends_with(".spec.js")
        || file_name.ends_with(".spec.jsx")
        || file_name.ends_with(".stories.ts")
        || file_name.ends_with(".stories.tsx")
        || file_name.ends_with(".stories.js")
        || file_name.ends_with("_test.go")
        || file_name.ends_with("_test.py")
        || file_name.starts_with("test_")
        || file_name == "conftest.py"
}
