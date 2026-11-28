pub mod go;
pub mod js_ts;
pub mod python;
pub mod rust;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use ignore::WalkBuilder;

use crate::domain::model::{FileEdge, FileGraph, FileNode, ImportKind, Language, ParsedFile};
use crate::domain::projection::assign_auto_module_ids;
use crate::paths::normalize_rel_path;
pub use crate::paths::{join_posix, normalize_posix, parent_dir, path_is_under};

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
    /// Build a repository index from `root` and the list of parsed source files.
    pub fn build(root: &Path, parsed_files: &[ParsedFile]) -> Self {
        let mut index = Self::default();

        for pf in parsed_files {
            index.known_files.insert(pf.path.clone());
            let dir = parent_dir(&pf.path);
            index
                .dir_files
                .entry(dir)
                .or_default()
                .push(pf.path.clone());

            let decl_set: HashSet<String> =
                pf.declarations.iter().map(|d| d.name.clone()).collect();
            let abs_set: HashSet<String> = pf
                .declarations
                .iter()
                .filter(|d| d.is_abstract)
                .map(|d| d.name.clone())
                .collect();
            index.file_decls.insert(pf.path.clone(), decl_set);
            index.abstract_decls.insert(pf.path.clone(), abs_set);
        }
        for files in index.dir_files.values_mut() {
            files.sort();
        }

        let mut py_roots = BTreeSet::new();
        py_roots.insert(String::new());

        let walker = WalkBuilder::new(root)
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
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            let rel_path = normalize_rel_path(root, path);
            if rel_path.contains("/node_modules/")
                || rel_path.starts_with("node_modules/")
                || rel_path.contains("/target/")
                || rel_path.starts_with("target/")
            {
                continue;
            }
            let dir = parent_dir(&rel_path);

            match file_name {
                "Cargo.toml" => {
                    if let Some((manifest, crate_info)) =
                        rust::load_cargo_manifest(path, &dir, &index.known_files)
                    {
                        index.packages.push(manifest);
                        index
                            .rust_crates
                            .insert(crate_info.crate_ident.clone(), crate_info.clone());
                        index.rust_crates_by_dir.push(crate_info);
                    }
                }
                "package.json" => {
                    if let Some((manifest, js_pkg)) = js_ts::load_package_json(path, &dir) {
                        index.packages.push(manifest);
                        index.js_packages.insert(js_pkg.name.clone(), js_pkg);
                    }
                }
                "tsconfig.json" | "jsconfig.json" | "tsconfig.app.json" => {
                    if let Some(tsconfig) = js_ts::load_tsconfig(root, path, &dir) {
                        index.tsconfigs.push(tsconfig);
                    }
                }
                "pyproject.toml" => {
                    if let Some(manifest) = python::load_pyproject(path, &dir) {
                        index.packages.push(manifest);
                    }
                    py_roots.insert(dir.clone());
                    let src_sub = join_posix(&dir, "src");
                    py_roots.insert(src_sub);
                }
                "go.mod" => {
                    if let Some((manifest, go_mod)) = go::load_go_mod(path, &dir) {
                        index.packages.push(manifest);
                        index.go_modules.push(go_mod);
                    }
                }
                _ => {}
            }
        }

        for pf in parsed_files {
            if pf.language == Language::Python {
                if let Some(idx) = pf.path.find("/src/") {
                    py_roots.insert(pf.path[..idx + 4].to_string());
                } else if pf.path.starts_with("src/") {
                    py_roots.insert("src".to_string());
                }
            }
        }

        index.packages.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| a.dir.cmp(&b.dir))
        });
        index.rust_crates_by_dir.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| a.dir.cmp(&b.dir))
        });
        index.tsconfigs.sort_by(|a, b| {
            b.dir
                .len()
                .cmp(&a.dir.len())
                .then_with(|| b.paths.len().cmp(&a.paths.len()))
        });
        index
            .go_modules
            .sort_by_key(|m| std::cmp::Reverse(m.module_path.len()));
        index.python_roots = py_roots.into_iter().collect();

        // Populate re-export index for Rust (`pub use`) and TS/JS (`export ... from`).
        index.build_reexport_table(parsed_files);

        index
    }

    fn build_reexport_table(&mut self, parsed_files: &[ParsedFile]) {
        let mut direct = Vec::new();
        let mut wildcards = Vec::new();

        for pf in parsed_files {
            for raw in &pf.raw_imports {
                if raw.kind != ImportKind::ReExport {
                    continue;
                }
                let outcome = match pf.language {
                    Language::Rust => rust::resolve_rust_import_direct(self, &pf.path, raw),
                    Language::TypeScript | Language::JavaScript | Language::Vue => {
                        js_ts::resolve_js_ts_import(self, &pf.path, &raw.specifier)
                    }
                    _ => continue,
                };
                if let ResolveOutcome::Internal(target) = outcome
                    && target != pf.path
                {
                    if raw.specifier.ends_with("::*") || raw.symbols.is_empty() {
                        wildcards.push((pf.path.clone(), target));
                    } else {
                        for sym in &raw.symbols {
                            direct.push(((pf.path.clone(), sym.clone()), target.clone()));
                        }
                    }
                }
            }
        }

        for (k, v) in direct {
            self.reexports.insert(k, v);
        }
        for (src, tgt) in wildcards {
            self.wildcard_reexports.entry(src).or_default().push(tgt);
        }
    }

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

struct EdgeAccumulator {
    lines: BTreeSet<usize>,
    symbols: BTreeSet<String>,
    kinds: Vec<ImportKind>,
}

/// Resolve all parsed files into a complete internal `FileGraph`.
pub fn resolve_file_graph(
    root: &Path,
    parsed_files: &[ParsedFile],
    include_mod_decls: bool,
    include_tests: bool,
) -> FileGraph {
    // Filter out Rust files declared solely under `#[cfg(test)] mod <name>;` when excluding tests.
    let active_files: Vec<&ParsedFile> = if include_tests {
        parsed_files.iter().collect()
    } else {
        let all_paths: HashSet<String> = parsed_files.iter().map(|p| p.path.clone()).collect();
        let mut excluded_test_paths = HashSet::new();
        for pf in parsed_files {
            if pf.language == Language::Rust {
                for mod_name in &pf.cfg_test_mods {
                    for hit in rust::collect_cfg_test_mod_files(&all_paths, &pf.path, mod_name) {
                        excluded_test_paths.insert(hit);
                    }
                }
            }
        }
        parsed_files
            .iter()
            .filter(|pf| !excluded_test_paths.contains(&pf.path))
            .collect()
    };

    let owned_active: Vec<ParsedFile> = active_files.into_iter().cloned().collect();
    let index = ProjectIndex::build(root, &owned_active);
    let mut files = Vec::with_capacity(owned_active.len());
    let mut edges = Vec::new();

    for pf in &owned_active {
        let pkg = index.enclosing_package(&pf.path);
        let impl_trait_names: HashSet<&str> = pf
            .implemented_traits
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();

        let mut target_acc: BTreeMap<String, EdgeAccumulator> = BTreeMap::new();
        let mut external_set = BTreeSet::new();
        let mut unresolved_set = BTreeSet::new();

        for raw in &pf.raw_imports {
            if raw.kind == ImportKind::ModuleDecl && !include_mod_decls {
                continue;
            }
            let outcome = match pf.language {
                Language::Rust => rust::resolve_rust_import(&index, &pf.path, raw),
                Language::TypeScript | Language::JavaScript | Language::Vue => {
                    js_ts::resolve_js_ts_import(&index, &pf.path, &raw.specifier)
                }
                Language::Python => python::resolve_python_import(&index, &pf.path, &raw.specifier),
                Language::Go => go::resolve_go_import(&index, &raw.specifier),
            };

            match outcome {
                ResolveOutcome::Internal(target) => {
                    if target != pf.path {
                        let entry = target_acc.entry(target).or_insert_with(|| EdgeAccumulator {
                            lines: BTreeSet::new(),
                            symbols: BTreeSet::new(),
                            kinds: Vec::new(),
                        });
                        entry.lines.insert(raw.line);
                        entry.kinds.push(raw.kind);
                        for sym in &raw.symbols {
                            entry.symbols.insert(sym.clone());
                        }
                    }
                }
                ResolveOutcome::External(dep) => {
                    if !dep.is_empty() {
                        external_set.insert(dep);
                    }
                }
                ResolveOutcome::Unresolved(spec) => {
                    unresolved_set.insert(spec);
                }
            }
        }

        for (target, acc) in target_acc {
            let lines: Vec<usize> = acc.lines.into_iter().collect();
            let symbols: Vec<String> = acc.symbols.into_iter().collect();
            let primary_line = lines.first().copied().unwrap_or(1);

            let has_trait_impl = acc.kinds.contains(&ImportKind::TraitImpl)
                || symbols
                    .iter()
                    .any(|s| impl_trait_names.contains(s.as_str()));
            let all_type_only = !acc.kinds.is_empty()
                && acc.kinds.iter().all(|&k| k == ImportKind::TypeOnly);
            let only_abstract_symbols = !symbols.is_empty()
                && symbols.iter().any(|s| index.file_declares_abstract(&target, s))
                && symbols.iter().all(|s| {
                    !index.file_declares_symbol(&target, s)
                        || index.file_declares_abstract(&target, s)
                });

            let kind = if has_trait_impl {
                ImportKind::TraitImpl
            } else if all_type_only {
                ImportKind::TypeOnly
            } else if acc.kinds.contains(&ImportKind::Import) {
                ImportKind::Import
            } else if acc.kinds.contains(&ImportKind::ReExport) {
                ImportKind::ReExport
            } else if acc.kinds.contains(&ImportKind::Dynamic) {
                ImportKind::Dynamic
            } else {
                acc.kinds.first().copied().unwrap_or(ImportKind::Import)
            };

            let is_dip = has_trait_impl || all_type_only || only_abstract_symbols;

            edges.push(FileEdge {
                from: pf.path.clone(),
                to: target,
                kind,
                line: primary_line,
                lines,
                symbols,
                is_dip,
            });
        }

        files.push(FileNode {
            path: pf.path.clone(),
            language: pf.language,
            package: pkg.map(|p| p.name.clone()),
            package_root: pkg.map(|p| p.dir.clone()),
            module_id: String::new(),
            loc: pf.loc,
            symbols: pf.symbols,
            declarations: pf.declarations.clone(),
            external_imports: external_set.into_iter().collect(),
            unresolved_imports: unresolved_set.into_iter().collect(),
            git: None,
        });
    }

    let mut graph = FileGraph { files, edges };
    assign_auto_module_ids(&mut graph);
    graph
}
