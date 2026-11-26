use std::fmt;

use serde::{Deserialize, Serialize};

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
            "ts" | "tsx" | "mts" | "cts" => Some(Self::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "vue" => Some(Self::Vue),
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

/// Kind of symbol declared at top level in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Trait,
    Interface,
    AbstractClass,
    Protocol,
    TypeAlias,
    Struct,
    Enum,
    Class,
    Function,
    Component,
}

/// Visibility level of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Internal,
    Private,
}

/// A top-level symbol declaration extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDecl {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub is_abstract: bool,
    pub line: usize,
}

/// Counts of abstract and concrete declarations found in a source file.
///
/// Used to compute Robert C. Martin's Abstractness metric `A = abstract_count / total`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolCounts {
    /// Exported/public interfaces, traits, abstract classes, protocols, or port types.
    pub abstract_declarations: usize,
    /// Exported/public concrete classes, structs, enums, components, or functions.
    pub concrete_declarations: usize,
    /// Non-exported private helpers (tracked for diagnostics, excluded from `A` when public items exist).
    #[serde(default)]
    pub private_declarations: usize,
}

impl SymbolCounts {
    pub fn total(self) -> usize {
        self.abstract_declarations + self.concrete_declarations
    }

    pub fn add_assign(&mut self, other: Self) {
        self.abstract_declarations += other.abstract_declarations;
        self.concrete_declarations += other.concrete_declarations;
        self.private_declarations += other.private_declarations;
    }

    /// Compute public-first abstract/concrete counts from extracted declarations.
    ///
    /// If a file exports at least one public/internal symbol, only non-private declarations count
    /// toward Robert C. Martin's `A = Na / Nc` (preventing private helper functions from drowning
    /// out public ports). If a file has only private declarations (e.g. a binary entrypoint), all
    /// declarations are counted so it is not misclassified as entity-free.
    pub fn from_declarations(decls: &[SymbolDecl]) -> Self {
        let mut counts = Self::default();
        let has_exported = decls.iter().any(|d| d.visibility != Visibility::Private);

        for decl in decls {
            if has_exported && decl.visibility == Visibility::Private {
                counts.private_declarations += 1;
            } else if decl.is_abstract {
                counts.abstract_declarations += 1;
            } else {
                counts.concrete_declarations += 1;
            }
        }
        counts
    }
}

/// Kind of dependency reference extracted from source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// Standard import/use statement (e.g. `import x from '...'`, `use crate::foo`).
    Import,
    /// Re-export statement (e.g. `export * from '...'`, `pub use ...`).
    ReExport,
    /// Child module declaration (e.g. Rust `mod child;`).
    ModuleDecl,
    /// Dynamic import or `require(...)`.
    Dynamic,
    /// Type-only import (e.g. TypeScript `import type { ... }`).
    TypeOnly,
    /// Dependency-inversion implementation (`impl Trait for Struct`, `implements Interface`).
    TraitImpl,
}

/// Unresolved dependency specifier extracted by a language parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawImport {
    /// Raw module path or specifier as written in source code.
    pub specifier: String,
    /// Syntactic kind of import.
    pub kind: ImportKind,
    /// 1-based line number in the source file.
    pub line: usize,
    /// Specific symbol names referenced by this import (when syntactically visible).
    #[serde(default)]
    pub symbols: Vec<String>,
}

/// Result of parsing a single source file before import resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedFile {
    /// Relative POSIX-style path from the analyzed root (e.g. `src/domain/model.rs`).
    pub path: String,
    pub language: Language,
    /// Non-empty lines of code.
    pub loc: usize,
    pub symbols: SymbolCounts,
    #[serde(default)]
    pub declarations: Vec<SymbolDecl>,
    pub raw_imports: Vec<RawImport>,
    /// External Rust submodule names declared under `#[cfg(test)] mod <name>;`.
    #[serde(default)]
    pub cfg_test_mods: Vec<String>,
    /// Trait or interface names implemented inside this file `(name, line)`.
    #[serde(default)]
    pub implemented_traits: Vec<(String, usize)>,
}

/// Git behavioral metrics collected per file when analyzing a Git repository.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileGitMetrics {
    /// Number of commits touching this file within the lookback window.
    pub commits: usize,
    /// Total lines added across those commits.
    pub lines_added: usize,
    /// Total lines deleted across those commits.
    pub lines_deleted: usize,
    /// Distinct commit authors touching this file.
    pub authors: usize,
    /// Share of commits authored by the single most frequent contributor (`0.0..=1.0`).
    pub top_author_share: f64,
    /// Primary author name (highest commit count).
    pub top_author: String,
    /// Days since the last commit modifying this file (relative to latest analyzed commit).
    pub days_since_last_change: u64,
    /// Hotspot score combining code size/coupling and commit frequency (`0.0..=1.0`).
    pub hotspot_score: f64,
}

/// Metadata for a source file node in the resolved dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// Relative POSIX-style path from the analyzed root.
    pub path: String,
    pub language: Language,
    /// Logical package/crate name discovered from manifests (`Cargo.toml`, `package.json`, etc.), if any.
    pub package: Option<String>,
    /// Directory of the enclosing package relative to root, if any.
    pub package_root: Option<String>,
    /// Auto-inferred architectural module identifier (used for hierarchical compound graphs).
    #[serde(default)]
    pub module_id: String,
    pub loc: usize,
    pub symbols: SymbolCounts,
    #[serde(default)]
    pub declarations: Vec<SymbolDecl>,
    /// External third-party packages referenced by this file.
    pub external_imports: Vec<String>,
    /// Local-looking imports that could not be resolved to a scanned file.
    pub unresolved_imports: Vec<String>,
    /// Optional Git churn & ownership metrics for this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<FileGitMetrics>,
}

/// Directed internal dependency edge between two scanned files (`from` depends on `to`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileEdge {
    pub from: String,
    pub to: String,
    pub kind: ImportKind,
    /// Primary 1-based line number in `from`.
    pub line: usize,
    /// All 1-based line numbers where `from` references `to`.
    #[serde(default)]
    pub lines: Vec<usize>,
    /// Specific symbol names referenced across this edge.
    #[serde(default)]
    pub symbols: Vec<String>,
    /// True if this edge implements or depends purely on an abstract contract/type (DIP).
    #[serde(default)]
    pub is_dip: bool,
}

/// Complete file-level dependency graph extracted from the repository.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileGraph {
    pub files: Vec<FileNode>,
    pub edges: Vec<FileEdge>,
}
