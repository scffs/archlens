pub mod go;
pub mod js_ts;
pub mod python;
pub mod rust;

mod index;
mod index_builder;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

pub use self::index::{
    GoModuleInfo, JsPackageInfo, PackageManifest, ProjectIndex, ResolveOutcome, RustCrateInfo,
    TsConfigInfo,
};
use crate::domain::model::{FileEdge, FileGraph, FileNode, ImportKind, Language, ParsedFile};
use crate::domain::projection::assign_auto_module_ids;
pub use crate::paths::{join_posix, normalize_posix, parent_dir, path_is_under};

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

    let index = ProjectIndex::build_for_files(root, active_files.iter().copied());
    let mut files = Vec::with_capacity(active_files.len());
    let mut edges = Vec::new();

    for pf in active_files {
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
