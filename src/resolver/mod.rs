pub mod go;
pub mod js_ts;
pub mod python;
pub mod rust;

mod dispatch;
mod index;
mod index_builder;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

pub(crate) use self::dispatch::builtin_resolver;
pub use self::index::{
    GoModuleInfo, ImportResolver, JsPackageInfo, PackageManifest, ProjectIndex, ResolveOutcome, RustCrateInfo,
    TsConfigInfo,
};
use crate::domain::model::{FileEdge, FileGraph, FileNode, ImportKind, ParsedFile};
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
    resolve_file_graph_with(root, parsed_files, include_mod_decls, include_tests, builtin_resolver)
}

pub(crate) fn resolve_file_graph_with<'r>(
    root: &Path,
    parsed_files: &[ParsedFile],
    include_mod_decls: bool,
    include_tests: bool,
    resolver_for: impl Fn(&ParsedFile) -> &'r dyn ImportResolver,
) -> FileGraph {
    let active_files: Vec<&ParsedFile> = if include_tests {
        parsed_files.iter().collect()
    } else {
        let all_paths: HashSet<String> = parsed_files.iter().map(|file| file.path.clone()).collect();
        let excluded_test_paths: HashSet<String> = parsed_files
            .iter()
            .flat_map(|file| resolver_for(file).test_only_files(&all_paths, file))
            .collect();
        parsed_files
            .iter()
            .filter(|file| !excluded_test_paths.contains(&file.path))
            .collect()
    };

    let index = ProjectIndex::build_for_files(root, active_files.iter().copied(), &resolver_for);
    let mut files = Vec::with_capacity(active_files.len());
    let mut edges = Vec::new();

    for pf in active_files {
        let resolver = resolver_for(pf);
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
            let outcome = resolver.resolve(&index, &pf.path, raw);

            match outcome {
                ResolveOutcome::Internal(target) if !index.known_files.contains(&target) => {
                    unresolved_set.insert(raw.specifier.clone());
                }
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
