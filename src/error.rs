use std::path::PathBuf;
use thiserror::Error;

/// Errors produced during repository scanning, parsing, graph analysis, or report generation.
#[derive(Debug, Error)]
pub enum ArchLensError {
    #[error("root path does not exist or is not accessible: {0}")]
    InvalidRoot(PathBuf),

    #[error("failed to read file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config file {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidGlob {
        pattern: String,
        #[source]
        source: globset::Error,
    },

    #[error("tree-sitter initialization failed for {language}: {message}")]
    ParserInit {
        language: &'static str,
        message: String,
    },

    #[error("failed to serialize report: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ArchLensError>;
