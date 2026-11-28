use std::fmt;

use serde::{Deserialize, Serialize};

const TYPESCRIPT_EXTENSIONS: [&str; 4] = ["ts", "tsx", "mts", "cts"];
const JAVASCRIPT_EXTENSIONS: [&str; 4] = ["js", "jsx", "mjs", "cjs"];
const VUE_EXTENSION: &str = "vue";

/// Priority for extensionless JS, TS and Vue imports.
pub(crate) fn js_ts_extensions() -> impl Iterator<Item = &'static str> {
    TYPESCRIPT_EXTENSIONS
        .into_iter()
        .chain(JAVASCRIPT_EXTENSIONS)
        .chain([VUE_EXTENSION])
}

/// Supported programming languages for AST-based dependency and abstraction analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Vue,
    Python,
    Go,
}

impl Language {
    /// Detect language from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            ext if TYPESCRIPT_EXTENSIONS.contains(&ext) => Some(Self::TypeScript),
            ext if JAVASCRIPT_EXTENSIONS.contains(&ext) => Some(Self::JavaScript),
            VUE_EXTENSION => Some(Self::Vue),
            "py" | "pyi" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }

    /// Human-readable label shown in reports.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Vue => "Vue",
            Self::Python => "Python",
            Self::Go => "Go",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}
