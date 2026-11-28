use std::collections::{HashMap, HashSet};

use crate::domain::model::{ParsedFile, RawImport};
use crate::paths::path_is_under;

/// Result of attempting to resolve one raw import specifier from a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOutcome {
    /// Resolved to an internal scanned file path (POSIX relative).
    Internal(String),
    /// Resolved as a third-party, stdlib, or static asset dependency.
    External(String),
    /// Looks internal/local, but no matching scanned file was found.
    Unresolved(String),
}

/// Converts imports from parsed source into internal or external dependency facts.
pub trait ImportResolver {
    fn resolve(&self, index: &ProjectIndex, from: &str, import: &RawImport) -> ResolveOutcome;

    /// Resolve a re-export directly while the index's re-export table is being built.
    ///
    /// Return `None` when the language has no re-export indexing. The index has
    /// file and manifest information at this stage, but no computed re-exports.
    fn resolve_reexport(
        &self,
        index: &ProjectIndex,
        from: &str,
        import: &RawImport,
    ) -> Option<ResolveOutcome> {
        Some(self.resolve(index, from, import))
    }

    /// File paths belonging exclusively to test declarations in this source.
    fn test_only_files(&self, _paths: &HashSet<String>, _file: &ParsedFile) -> Vec<String> {
        Vec::new()
    }
}

/// Discovered package manifest metadata used for both import resolution and package-level views.
#[derive(Debug, Clone)]
pub struct PackageManifest {
    pub name: String,
    /// Relative directory of the manifest (empty string for repo root).
    pub dir: String,
}

/// Rust crate layout discovered from a `Cargo.toml`.
#[derive(Debug, Clone)]
pub struct RustCrateInfo {
    /// Normalized Rust crate identifier (`-` replaced by `_`).
    pub crate_ident: String,
    /// Package directory relative to repo root.
    pub dir: String,
    /// Source root directory (usually `<dir>/src`).
    pub src_dir: String,
    /// Main library entry file (`<dir>/src/lib.rs`), if present.
    pub lib_entry: Option<String>,
    /// Binary entry file (`<dir>/src/main.rs`), if present.
    pub bin_entry: Option<String>,
}

/// JS/TS workspace package discovered from `package.json`.
#[derive(Debug, Clone)]
pub struct JsPackageInfo {
    pub name: String,
    pub dir: String,
    pub entry_candidates: Vec<String>,
}

/// Parsed `tsconfig.json` / `jsconfig.json` path-mapping configuration.
#[derive(Debug, Clone)]
pub struct TsConfigInfo {
    /// Relative directory containing the tsconfig.
    pub dir: String,
    /// Resolved relative base directory for `baseUrl` imports.
    pub base_dir: String,
    /// `(pattern, pre-normalized repo-relative replacements)` from `compilerOptions.paths`.
    pub paths: Vec<(String, Vec<String>)>,
    /// Whether `compilerOptions.baseUrl` was explicitly configured.
    pub has_base_url: bool,
}

/// Go module discovered from `go.mod`.
#[derive(Debug, Clone)]
pub struct GoModuleInfo {
    pub module_path: String,
    pub dir: String,
}

/// Indexed repository metadata shared across all language resolvers.
#[derive(Debug, Default)]
pub struct ProjectIndex {
    pub known_files: HashSet<String>,
    pub dir_files: HashMap<String, Vec<String>>,
    pub packages: Vec<PackageManifest>,
    pub rust_crates: HashMap<String, RustCrateInfo>,
    pub rust_crates_by_dir: Vec<RustCrateInfo>,
    pub js_packages: HashMap<String, JsPackageInfo>,
    pub tsconfigs: Vec<TsConfigInfo>,
    pub go_modules: Vec<GoModuleInfo>,
    pub python_roots: Vec<String>,
    /// Top-level symbol names declared in each file.
    pub file_decls: HashMap<String, HashSet<String>>,
    /// Abstract symbol names (traits, interfaces, protocols) declared in each file.
    pub abstract_decls: HashMap<String, HashSet<String>>,
    /// Direct re-export map: `(facade_file, symbol_name) -> target_file`.
    pub reexports: HashMap<(String, String), String>,
    /// Wildcard re-export map: `facade_file -> Vec<target_file>`.
    pub wildcard_reexports: HashMap<String, Vec<String>>,
}

impl ProjectIndex {
    /// True if `file` declares a top-level symbol named `symbol`.
    pub fn file_declares_symbol(&self, file: &str, symbol: &str) -> bool {
        self.file_decls
            .get(file)
            .is_some_and(|set| set.contains(symbol))
    }

    /// True if `file` declares an abstract symbol (trait/interface/protocol) named `symbol`.
    pub fn file_declares_abstract(&self, file: &str, symbol: &str) -> bool {
        self.abstract_decls
            .get(file)
            .is_some_and(|set| set.contains(symbol))
    }

    /// Find the nearest enclosing package manifest for a file path.
    pub fn enclosing_package(&self, rel_path: &str) -> Option<&PackageManifest> {
        self.packages
            .iter()
            .find(|pkg| path_is_under(rel_path, &pkg.dir))
    }

    /// Find the nearest enclosing Rust crate for a file path.
    pub fn enclosing_rust_crate(&self, rel_path: &str) -> Option<&RustCrateInfo> {
        self.rust_crates_by_dir
            .iter()
            .find(|krate| path_is_under(rel_path, &krate.dir))
    }
}
